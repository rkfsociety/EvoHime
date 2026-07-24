//! Minimal Chrome DevTools Protocol client with per-task sessions (Stage 7.100, wave 1).
//!
//! EvoHime attaches to an already-running browser exposed via
//! `EVOHIME_BROWSER_CDP_URL` (e.g. `chrome --remote-debugging-port=9222`).
//! One task owns one tab: the session registry keys websocket connections by
//! `task_id`, so page state survives between tool calls of the same task.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

pub const CDP_URL_ENV: &str = "EVOHIME_BROWSER_CDP_URL";
const MAX_SESSIONS: usize = 4;
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// Trusted operator-provided DevTools endpoint; `None` disables the feature.
pub fn cdp_base_url() -> Option<String> {
    let raw = std::env::var(CDP_URL_ENV).ok()?;
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

#[derive(Debug)]
pub enum CdpFrame {
    Response { id: u64, result: Value, error: Option<String> },
    Event { method: String },
    Other,
}

pub fn build_command(id: u64, method: &str, params: Value) -> String {
    json!({ "id": id, "method": method, "params": params }).to_string()
}

pub fn parse_frame(text: &str) -> CdpFrame {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return CdpFrame::Other;
    };
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        let error = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string);
        return CdpFrame::Response {
            id,
            result: value.get("result").cloned().unwrap_or(Value::Null),
            error,
        };
    }
    if let Some(method) = value.get("method").and_then(Value::as_str) {
        return CdpFrame::Event { method: method.to_string() };
    }
    CdpFrame::Other
}

pub struct CdpSession {
    base: String,
    target_id: String,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
    load_fired: bool,
    pub last_used: Instant,
}

impl CdpSession {
    pub async fn open(base: &str, timeout: Duration) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| format!("cdp client setup failed: {error}"))?;
        let target: Value = client
            .put(format!("{base}/json/new?url=about:blank"))
            .send()
            .await
            .map_err(|error| format!("cdp target create failed: {error}"))?
            .json()
            .await
            .map_err(|error| format!("cdp target response invalid: {error}"))?;
        let target_id = target
            .get("id")
            .and_then(Value::as_str)
            .ok_or("cdp target response missing id")?
            .to_string();
        let ws_url = target
            .get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .ok_or("cdp target response missing webSocketDebuggerUrl")?
            .to_string();

        let (ws, _) = tokio::time::timeout(timeout, connect_async(&ws_url))
            .await
            .map_err(|_| "cdp websocket connect timed out".to_string())?
            .map_err(|error| format!("cdp websocket connect failed: {error}"))?;

        let mut session = Self {
            base: base.to_string(),
            target_id,
            ws,
            next_id: 0,
            load_fired: false,
            last_used: Instant::now(),
        };
        session.command("Page.enable", json!({}), COMMAND_TIMEOUT).await?;
        Ok(session)
    }

    async fn command(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.ws
            .send(Message::Text(build_command(id, method, params).into()))
            .await
            .map_err(|error| format!("cdp send failed: {error}"))?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| format!("cdp command timed out: {method}"))?;
            let frame = tokio::time::timeout(remaining, self.ws.next())
                .await
                .map_err(|_| format!("cdp command timed out: {method}"))?
                .ok_or("cdp websocket closed")?
                .map_err(|error| format!("cdp read failed: {error}"))?;
            let Message::Text(text) = frame else { continue };
            match parse_frame(&text) {
                CdpFrame::Response { id: frame_id, result, error } if frame_id == id => {
                    return match error {
                        Some(error) => Err(format!("cdp {method} failed: {error}")),
                        None => Ok(result),
                    };
                }
                CdpFrame::Event { method } if method == "Page.loadEventFired" => {
                    self.load_fired = true;
                }
                _ => {}
            }
        }
    }

    /// Navigate and wait for `Page.loadEventFired`; SPA pages that never fire
    /// the event fall through gracefully once `timeout` elapses.
    pub async fn navigate(&mut self, url: &str, timeout: Duration) -> Result<(), String> {
        self.load_fired = false;
        self.command("Page.navigate", json!({ "url": url }), COMMAND_TIMEOUT)
            .await?;
        let deadline = Instant::now() + timeout;
        while !self.load_fired {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let Ok(Some(Ok(Message::Text(text)))) =
                tokio::time::timeout(remaining, self.ws.next()).await
            else {
                break;
            };
            if let CdpFrame::Event { method } = parse_frame(&text) {
                if method == "Page.loadEventFired" {
                    self.load_fired = true;
                }
            }
        }
        Ok(())
    }

    async fn eval(&mut self, expression: &str) -> Result<Value, String> {
        let result = self
            .command(
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true }),
                COMMAND_TIMEOUT,
            )
            .await?;
        Ok(result
            .get("result")
            .and_then(|inner| inner.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Current page url/title/text without re-navigation. The expression is
    /// fixed — no model-provided JS reaches the page.
    pub async fn page_snapshot(&mut self, max_chars: usize) -> Result<Value, String> {
        let expression = format!(
            "JSON.stringify({{ url: location.href, title: document.title, \
             text: (document.body ? document.body.innerText : '').slice(0, {max_chars}) }})"
        );
        let raw = self.eval(&expression).await?;
        let text = raw.as_str().ok_or("cdp snapshot returned no string")?;
        serde_json::from_str(text).map_err(|error| format!("cdp snapshot parse failed: {error}"))
    }

    /// Click `document.querySelector(selector)`; `Ok(false)` when not found.
    /// The selector is embedded as a JSON string literal — it cannot escape
    /// into script context.
    pub async fn click(&mut self, selector: &str) -> Result<bool, String> {
        let literal = serde_json::to_string(selector)
            .map_err(|error| format!("selector encode failed: {error}"))?;
        let expression = format!(
            "(() => {{ const el = document.querySelector({literal}); \
             if (!el) return false; el.click(); return true; }})()"
        );
        Ok(self.eval(&expression).await?.as_bool().unwrap_or(false))
    }

    pub async fn close(mut self) {
        let _ = self.ws.close(None).await;
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        {
            let _ = client
                .get(format!("{}/json/close/{}", self.base, self.target_id))
                .send()
                .await;
        }
    }
}

type SessionMap = HashMap<Uuid, std::sync::Arc<Mutex<CdpSession>>>;

fn sessions() -> &'static Mutex<SessionMap> {
    static SESSIONS: OnceLock<Mutex<SessionMap>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reuse the task's session or open a new one; enforces the session cap and
/// lazily evicts idle sessions.
pub async fn session_for_task(
    task_id: Uuid,
    base: &str,
    timeout: Duration,
) -> Result<std::sync::Arc<Mutex<CdpSession>>, String> {
    let mut map = sessions().lock().await;
    let mut stale = Vec::new();
    for (key, session) in map.iter() {
        if let Ok(guard) = session.try_lock() {
            if guard.last_used.elapsed() > IDLE_TIMEOUT {
                stale.push(*key);
            }
        }
    }
    for key in stale {
        if key != task_id {
            if let Some(session) = map.remove(&key) {
                spawn_close(session);
            }
        }
    }

    if let Some(session) = map.get(&task_id) {
        return Ok(session.clone());
    }
    if map.len() >= MAX_SESSIONS {
        let oldest = map
            .iter()
            .filter_map(|(key, session)| {
                session.try_lock().ok().map(|guard| (*key, guard.last_used))
            })
            .min_by_key(|(_, last_used)| *last_used)
            .map(|(key, _)| key);
        match oldest {
            Some(key) => {
                if let Some(session) = map.remove(&key) {
                    spawn_close(session);
                }
            }
            None => return Err("cdp session cap reached and all sessions are busy".into()),
        }
    }

    let session = std::sync::Arc::new(Mutex::new(CdpSession::open(base, timeout).await?));
    map.insert(task_id, session.clone());
    Ok(session)
}

/// Remove the task's session from the registry for explicit close.
pub async fn take_session(task_id: Uuid) -> Option<std::sync::Arc<Mutex<CdpSession>>> {
    sessions().lock().await.remove(&task_id)
}

fn spawn_close(session: std::sync::Arc<Mutex<CdpSession>>) {
    tokio::spawn(async move {
        if let Ok(session) = std::sync::Arc::try_unwrap(session) {
            session.into_inner().close().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_serialized_with_id_method_and_params() {
        let text = build_command(7, "Page.navigate", json!({ "url": "https://example.com" }));
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["method"], "Page.navigate");
        assert_eq!(value["params"]["url"], "https://example.com");
    }

    #[test]
    fn frames_are_classified_as_response_event_or_other() {
        match parse_frame(r#"{"id":3,"result":{"frameId":"f"}}"#) {
            CdpFrame::Response { id, result, error } => {
                assert_eq!(id, 3);
                assert_eq!(result["frameId"], "f");
                assert!(error.is_none());
            }
            other => panic!("unexpected frame: {other:?}"),
        }
        match parse_frame(r#"{"id":4,"error":{"message":"boom"}}"#) {
            CdpFrame::Response { error, .. } => assert_eq!(error.as_deref(), Some("boom")),
            other => panic!("unexpected frame: {other:?}"),
        }
        match parse_frame(r#"{"method":"Page.loadEventFired","params":{}}"#) {
            CdpFrame::Event { method } => assert_eq!(method, "Page.loadEventFired"),
            other => panic!("unexpected frame: {other:?}"),
        }
        assert!(matches!(parse_frame("not json"), CdpFrame::Other));
        assert!(matches!(parse_frame("{}"), CdpFrame::Other));
    }

    #[test]
    fn selector_is_embedded_as_json_literal() {
        let hostile = "a\"); alert(1); (\"";
        let literal = serde_json::to_string(hostile).unwrap();
        // Round-trips losslessly and every interior quote is escaped, so the
        // literal cannot terminate early inside the click expression.
        assert_eq!(serde_json::from_str::<String>(&literal).unwrap(), hostile);
        let interior = &literal[1..literal.len() - 1];
        let unescaped_quotes = interior
            .char_indices()
            .filter(|(index, c)| *c == '"' && !interior[..*index].ends_with('\\'))
            .count();
        assert_eq!(unescaped_quotes, 0);
    }
}

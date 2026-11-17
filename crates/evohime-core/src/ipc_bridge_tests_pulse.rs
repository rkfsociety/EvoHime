use super::*;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::eva_pulse::{
    EvaPulseDaemon, EvaPulseOptions, PulseCycleReport, PulseProbe, PulseTaskSink,
};
use crate::hive_state::{HIVE_PULSE_ACTIVE, HIVE_PULSE_DISABLED};

struct CalmProbe {
    cycles: AtomicUsize,
}

impl PulseProbe for CalmProbe {
    fn observe(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PulseCycleReport, String>> + Send + '_>> {
        self.cycles.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(PulseCycleReport {
                interval_secs: 1,
                daily_cost_nano_usd: Some(0),
                daily_budget_nano_usd: 500_000_000,
                provider_window_exhausted: false,
                provider_window_wake_at_ms: None,
                rate_limited_until_ms: None,
                failed_workers: Vec::new(),
                route_is_primary: true,
            })
        })
    }
}

struct NoopSink;

impl PulseTaskSink for NoopSink {
    fn dispatch_recovery(
        &self,
        _failed_workers: &[String],
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async move { Err("not_used".into()) })
    }
}

#[tokio::test]
async fn pulse_daemon_starts_and_stops_via_ipc() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal = EventJournal::open(directory.path().join("pulse-ipc.db")).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(16, None, journal.clone());
    let probe = Arc::new(CalmProbe {
        cycles: AtomicUsize::new(0),
    });
    let daemon = EvaPulseDaemon::spawn(
        Arc::clone(&probe) as Arc<dyn PulseProbe>,
        Arc::new(NoopSink) as Arc<dyn PulseTaskSink>,
        EvaPulseOptions {
            interval_override: Some(Duration::from_millis(10)),
        },
        None,
    );
    daemon.set_enabled(false);
    daemon.run();
    let bridge = IpcBridge::with_coordinator(journal, coordinator)
        .with_pulse_daemon(Arc::new(daemon.clone()));

    let (event_type, payload) = super::ambient::ambient_call(
        &bridge,
        generated::command_envelope::Command::TogglePulse(generated::TogglePulseCommand {
            schema_version: 1,
            request_id: "pulse-off".into(),
            enabled: false,
        }),
    )
    .await;
    assert_eq!(event_type, "pulse.status");
    assert_eq!(payload["status"], HIVE_PULSE_DISABLED);
    assert_eq!(payload["enabled"], false);

    let stopped_at = probe.cycles.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        probe.cycles.load(Ordering::SeqCst),
        stopped_at,
        "a disabled daemon must not run cycles"
    );

    let (event_type, payload) = super::ambient::ambient_call(
        &bridge,
        generated::command_envelope::Command::TogglePulse(generated::TogglePulseCommand {
            schema_version: 1,
            request_id: "pulse-on".into(),
            enabled: true,
        }),
    )
    .await;
    assert_eq!(event_type, "pulse.status");
    assert_eq!(payload["status"], HIVE_PULSE_ACTIVE);
    assert_eq!(payload["enabled"], true);

    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        probe.cycles.load(Ordering::SeqCst) > stopped_at,
        "an enabled daemon must run cycles"
    );

    let snapshot = bridge.hive_state_snapshot().await;
    assert_eq!(snapshot.pulse_status, HIVE_PULSE_ACTIVE);
    daemon.shutdown();
}

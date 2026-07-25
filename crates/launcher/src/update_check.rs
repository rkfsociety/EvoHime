//! Проверка обновлений через Atom-фид релизов (раздел V плана).
//!
//! Критичная проблема REST API: анонимный лимит GitHub REST API — 60
//! запросов/час на IP. В офисном сценарии за одним NAT (несколько
//! пользователей EvoHime, каждый проверяет раз в 30 мин) лимит
//! исчерпывается почти мгновенно — Launcher начнёт получать 403 Forbidden
//! для всех в этой сети. `releases.atom` — обычный веб-запрос, не GitHub
//! API, под эти лимиты не подпадает. REST API используется только один
//! раз — в момент реального клика "Обновить сейчас" (см. update_apply.rs),
//! для получения точных ссылок на ассеты и SHA256.

use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, thiserror::Error)]
pub enum AtomFeedError {
    #[error("no <entry> found in feed")]
    NoEntries,
    #[error("entry has no <title>")]
    MissingTitle,
    #[error(transparent)]
    Xml(#[from] quick_xml::Error),
    #[error(transparent)]
    Encoding(#[from] quick_xml::encoding::EncodingError),
    #[error(transparent)]
    Escape(#[from] quick_xml::escape::EscapeError),
}

/// Возвращает URL Atom-фида релизов для `owner/repo`.
pub fn releases_atom_url(github_repo: &str) -> String {
    format!("https://github.com/{github_repo}/releases.atom")
}

/// Извлекает тег версии (`<title>` первого `<entry>` — самый свежий релиз,
/// GitHub упорядочивает фид от нового к старому) из Atom-фида GitHub
/// Releases.
pub fn latest_version_from_atom(xml: &str) -> Result<String, AtomFeedError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut in_entry = false;
    let mut in_title = false;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let qname = e.name();
                let name = local_name(qname.as_ref());
                if name == b"entry" {
                    in_entry = true;
                } else if in_entry && name == b"title" {
                    in_title = true;
                }
            }
            Event::End(e) => {
                let qname = e.name();
                let name = local_name(qname.as_ref());
                if name == b"entry" {
                    if in_entry {
                        return Err(AtomFeedError::MissingTitle);
                    }
                } else if name == b"title" {
                    in_title = false;
                }
            }
            Event::Text(text) if in_entry && in_title => {
                let decoded = text.decode()?;
                return Ok(quick_xml::escape::unescape(&decoded)?.into_owned());
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Err(AtomFeedError::NoEntries)
}

/// `true`, если `remote_version` отличается от `local_version` — прямое
/// строковое сравнение тегов (`v0.4.2` vs `v0.4.1`), без семантического
/// разбора версий: Launcher не решает, "новее ли" версия, только "другая
/// ли" — публикует релизы только сам проект, порядок доверяем GitHub'у.
pub fn is_update_available(local_version: &str, remote_version: &str) -> bool {
    local_version.trim() != remote_version.trim()
}

fn local_name(qualified: &[u8]) -> &[u8] {
    match qualified.iter().rposition(|&b| b == b':') {
        Some(idx) => &qualified[idx + 1..],
        None => qualified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en-US">
  <id>tag:github.com,2008:https://github.com/user/EvoHime/releases</id>
  <link type="text/html" rel="alternate" href="https://github.com/user/EvoHime/releases"/>
  <title>Release notes from EvoHime</title>
  <entry>
    <id>tag:github.com,2008:Repository/000/v0.4.2</id>
    <updated>2026-07-25T10:00:00Z</updated>
    <link rel="alternate" type="text/html" href="https://github.com/user/EvoHime/releases/tag/v0.4.2"/>
    <title>v0.4.2</title>
    <content type="html">&lt;p&gt;Changelog here&lt;/p&gt;</content>
  </entry>
  <entry>
    <id>tag:github.com,2008:Repository/000/v0.4.1</id>
    <updated>2026-07-20T10:00:00Z</updated>
    <link rel="alternate" type="text/html" href="https://github.com/user/EvoHime/releases/tag/v0.4.1"/>
    <title>v0.4.1</title>
    <content type="html">&lt;p&gt;Older changelog&lt;/p&gt;</content>
  </entry>
</feed>
"#;

    #[test]
    fn extracts_latest_version_as_first_entry_title() {
        let version = latest_version_from_atom(SAMPLE_FEED).unwrap();
        assert_eq!(version, "v0.4.2");
    }

    #[test]
    fn errors_on_feed_with_no_entries() {
        let empty_feed = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Empty</title></feed>"#;
        assert!(matches!(
            latest_version_from_atom(empty_feed),
            Err(AtomFeedError::NoEntries)
        ));
    }

    #[test]
    fn errors_on_entry_without_title() {
        let feed = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><id>x</id></entry></feed>"#;
        assert!(matches!(
            latest_version_from_atom(feed),
            Err(AtomFeedError::MissingTitle)
        ));
    }

    #[test]
    fn is_update_available_detects_difference() {
        assert!(is_update_available("v0.4.1", "v0.4.2"));
        assert!(!is_update_available("v0.4.2", "v0.4.2"));
    }

    #[test]
    fn is_update_available_ignores_surrounding_whitespace() {
        assert!(!is_update_available("v0.4.2\n", " v0.4.2"));
    }

    #[test]
    fn builds_expected_atom_url() {
        assert_eq!(
            releases_atom_url("user/EvoHime"),
            "https://github.com/user/EvoHime/releases.atom"
        );
    }
}

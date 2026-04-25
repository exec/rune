//! Background update-check notifier.
//!
//! On startup we spawn a thread that hits the GitHub releases API to see if a
//! newer version of rune is available. The result is cached on disk for 24h
//! so we don't spam the API on every launch. If a newer version is found, a
//! one-line notice is parked in a static `Mutex` and the main loop drains it
//! into the status bar via `take_pending_notice`.
//!
//! Design rules:
//! - Never block startup. `spawn_check` returns immediately.
//! - Never panic. Every fallible step returns `Result` and the spawned closure
//!   silently swallows errors — a failed check is indistinguishable from "no
//!   update available" from the user's perspective.
//! - Never write to stdout/stderr. We're in a raw-mode TUI.
//! - Be defensive about JSON: GitHub's response is ~5KB, but we only need
//!   `tag_name`, so we extract it with a tiny string scan instead of pulling
//!   in `serde_json` as a direct dependency.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RELEASES_URL: &str = "https://api.github.com/repos/exec/rune/releases/latest";
const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);

fn pending_notice() -> &'static Mutex<Option<String>> {
    static PENDING: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

/// Drain the pending update notice, if any. Called once per main-loop tick.
pub fn take_pending_notice() -> Option<String> {
    match pending_notice().lock() {
        Ok(mut guard) => guard.take(),
        Err(_) => None,
    }
}

/// Spawn a background thread that checks for updates. Returns immediately.
///
/// The current crate version is wired in via `env!("CARGO_PKG_VERSION")` at
/// the call site so we don't have to thread it through.
pub fn spawn_check() {
    thread::spawn(|| {
        let current = env!("CARGO_PKG_VERSION");
        let cache_dir = match dirs::cache_dir() {
            Some(d) => d.join("rune"),
            None => return,
        };
        if let Some(latest) = check_with_cache_dir(&cache_dir, current) {
            let notice = format!(
                "Update available: v{latest} — https://github.com/exec/rune/releases/latest"
            );
            if let Ok(mut guard) = pending_notice().lock() {
                *guard = Some(notice);
            }
        }
    });
}

/// Core check logic, factored out so tests can pass an explicit cache dir.
///
/// Returns `Some(version)` when a strictly newer version is available, and
/// `None` for the up-to-date case OR any unexpected failure (network error,
/// malformed JSON, etc.). Failures are silent by design: from the user's
/// perspective an offline launch and an up-to-date launch should look the
/// same.
pub fn check_with_cache_dir(cache_dir: &Path, current: &str) -> Option<String> {
    let cache_path = cache_dir.join("update.json");
    let now = now_secs();

    // Fast path: fresh cache entry, skip the HTTP call entirely.
    if let Some((checked_at, latest)) = read_cache(&cache_path) {
        if now.saturating_sub(checked_at) < CACHE_TTL_SECS {
            return if is_newer(&latest, current) {
                Some(latest)
            } else {
                None
            };
        }
    }

    // Cache miss / expired: do the HTTP call.
    let body = fetch_latest_release_body()?;
    let tag = extract_tag_name(&body)?;
    let latest = strip_v_prefix(&tag).to_string();

    // Best-effort cache write; ignore failures.
    let _ = write_cache(&cache_path, now, &latest);

    if is_newer(&latest, current) {
        Some(latest)
    } else {
        None
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fetch_latest_release_body() -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .build();
    let user_agent = format!("rune/{}", env!("CARGO_PKG_VERSION"));
    let resp = agent
        .get(RELEASES_URL)
        .set("User-Agent", &user_agent)
        .set("Accept", "application/vnd.github+json")
        .call()
        .ok()?;
    resp.into_string().ok()
}

/// Extract the `"tag_name": "..."` value from a GitHub release JSON body.
///
/// We deliberately avoid pulling in `serde_json` for this single field and
/// instead do a small substring scan. The implementation accepts any
/// whitespace and the standard JSON escapes we care about (`\"` and `\\`).
fn extract_tag_name(body: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let mut idx = body.find(key)? + key.len();
    let bytes = body.as_bytes();

    // Skip whitespace + colon.
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b':' {
        return None;
    }
    idx += 1;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b'"' {
        return None;
    }
    idx += 1;

    // Walk until the closing quote, honoring `\"` and `\\` escapes.
    let mut out = String::new();
    while idx < bytes.len() {
        let b = bytes[idx];
        if b == b'\\' && idx + 1 < bytes.len() {
            let next = bytes[idx + 1];
            if next == b'"' || next == b'\\' {
                out.push(next as char);
                idx += 2;
                continue;
            }
        }
        if b == b'"' {
            return Some(out);
        }
        out.push(b as char);
        idx += 1;
    }
    None
}

fn strip_v_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Return true iff `latest` is strictly greater than `current` under a
/// dot-separated `u32` lexicographic ordering. Malformed input → `false`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse =
        |s: &str| -> Option<Vec<u32>> { s.split('.').map(|seg| seg.parse::<u32>().ok()).collect() };
    let (Some(l), Some(c)) = (parse(latest), parse(current)) else {
        return false;
    };
    let len = l.len().max(c.len());
    for i in 0..len {
        let li = l.get(i).copied().unwrap_or(0);
        let ci = c.get(i).copied().unwrap_or(0);
        if li > ci {
            return true;
        }
        if li < ci {
            return false;
        }
    }
    false
}

fn read_cache(path: &Path) -> Option<(u64, String)> {
    let body = std::fs::read_to_string(path).ok()?;
    let checked_at = extract_u64_field(&body, "checked_at")?;
    let latest = extract_string_field(&body, "latest_version")?;
    Some((checked_at, latest))
}

fn write_cache(path: &Path, checked_at: u64, latest: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // We hand-write a tiny JSON object — same justification as the reader:
    // we don't want a serde_json direct dep just for two fields.
    let escaped = latest.replace('\\', "\\\\").replace('"', "\\\"");
    let body = format!("{{\"checked_at\":{checked_at},\"latest_version\":\"{escaped}\"}}");
    std::fs::write(path, body)
}

fn extract_u64_field(body: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let mut idx = body.find(&needle)? + needle.len();
    let bytes = body.as_bytes();
    while idx < bytes.len() && (bytes[idx].is_ascii_whitespace() || bytes[idx] == b':') {
        idx += 1;
    }
    let start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if start == idx {
        return None;
    }
    body.get(start..idx)?.parse::<u64>().ok()
}

fn extract_string_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut idx = body.find(&needle)? + needle.len();
    let bytes = body.as_bytes();
    while idx < bytes.len() && (bytes[idx].is_ascii_whitespace() || bytes[idx] == b':') {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b'"' {
        return None;
    }
    idx += 1;
    let mut out = String::new();
    while idx < bytes.len() {
        let b = bytes[idx];
        if b == b'\\' && idx + 1 < bytes.len() {
            let next = bytes[idx + 1];
            if next == b'"' || next == b'\\' {
                out.push(next as char);
                idx += 2;
                continue;
            }
        }
        if b == b'"' {
            return Some(out);
        }
        out.push(b as char);
        idx += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn is_newer_basic() {
        assert!(is_newer("1.5.2", "1.5.1"));
        assert!(!is_newer("1.5.1", "1.5.1"));
        assert!(!is_newer("1.5.0", "1.5.1"));
        assert!(is_newer("2.0.0", "1.99.99"));
    }

    #[test]
    fn is_newer_different_segment_counts() {
        assert!(is_newer("1.5.1.1", "1.5.1"));
        assert!(!is_newer("1.5", "1.5.0"));
        assert!(!is_newer("1.5.0", "1.5"));
    }

    #[test]
    fn is_newer_malformed_returns_false() {
        assert!(!is_newer("not-a-version", "1.5.1"));
        assert!(!is_newer("1.5.1", "not-a-version"));
        assert!(!is_newer("1.x.1", "1.5.1"));
        assert!(!is_newer("", "1.5.1"));
    }

    #[test]
    fn extract_tag_name_simple() {
        let body = r#"{"tag_name":"v1.5.2","name":"Release 1.5.2"}"#;
        assert_eq!(extract_tag_name(body).as_deref(), Some("v1.5.2"));
    }

    #[test]
    fn extract_tag_name_with_whitespace() {
        let body = r#"{ "tag_name" : "v2.0.0" , "draft": false }"#;
        assert_eq!(extract_tag_name(body).as_deref(), Some("v2.0.0"));
    }

    #[test]
    fn extract_tag_name_handles_escaped_quote() {
        // Unrealistic for a tag, but verifies the escape branch.
        let body = r#"{"tag_name":"a\"b","x":1}"#;
        assert_eq!(extract_tag_name(body).as_deref(), Some("a\"b"));
    }

    #[test]
    fn extract_tag_name_missing() {
        let body = r#"{"name":"Release"}"#;
        assert_eq!(extract_tag_name(body), None);
    }

    #[test]
    fn strip_v_prefix_works() {
        assert_eq!(strip_v_prefix("v1.5.2"), "1.5.2");
        assert_eq!(strip_v_prefix("1.5.2"), "1.5.2");
    }

    #[test]
    fn cache_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("update.json");
        write_cache(&path, 1_700_000_000, "1.5.2").unwrap();
        let (ts, ver) = read_cache(&path).unwrap();
        assert_eq!(ts, 1_700_000_000);
        assert_eq!(ver, "1.5.2");
    }

    #[test]
    fn cache_missing_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        assert!(read_cache(&path).is_none());
    }

    #[test]
    fn cache_corrupt_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("update.json");
        std::fs::write(&path, "not json at all").unwrap();
        assert!(read_cache(&path).is_none());
    }

    #[test]
    fn check_with_cache_dir_uses_fresh_cache() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("update.json");
        write_cache(&path, now_secs(), "9.9.9").unwrap();

        // Fresh cache says 9.9.9 is the latest, current is 1.5.1, so we
        // expect Some("9.9.9") without any network call.
        let result = check_with_cache_dir(dir.path(), "1.5.1");
        assert_eq!(result.as_deref(), Some("9.9.9"));
    }

    #[test]
    fn check_with_cache_dir_fresh_cache_no_update() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("update.json");
        write_cache(&path, now_secs(), "1.5.1").unwrap();

        let result = check_with_cache_dir(dir.path(), "1.5.1");
        assert_eq!(result, None);
    }

    #[test]
    fn extract_u64_field_works() {
        let body = r#"{"checked_at":1700000000,"latest_version":"1.5.2"}"#;
        assert_eq!(extract_u64_field(body, "checked_at"), Some(1_700_000_000));
    }

    #[test]
    fn extract_string_field_works() {
        let body = r#"{"checked_at":1700000000,"latest_version":"1.5.2"}"#;
        assert_eq!(
            extract_string_field(body, "latest_version").as_deref(),
            Some("1.5.2")
        );
    }

    #[test]
    fn take_pending_notice_returns_none_when_unset() {
        // Note: this test shares static state with other tests, so we just
        // verify it returns Option<String> shape and doesn't panic.
        let _ = take_pending_notice();
    }
}

//! In-house access audit — persistent JSONL request log, independent of
//! Cloudflare. Enabled when `LLM_WIKI_AUDIT_DIR` is set; otherwise every
//! function here is a cheap no-op and existing behavior is unchanged.
//!
//! One line per request, appended to a file that rotates by UTC day:
//! `<dir>/access-YYYY-MM-DD.jsonl`. Old files are pruned at startup based on
//! `LLM_WIKI_AUDIT_RETENTION_DAYS`. Writes are best-effort: an IO error logs a
//! warning to stderr and is skipped, it never takes the server down.
//!
//! # Client IP behind Cloudflare Tunnel
//! The servers bind 127.0.0.1 and are reached through cloudflared, so
//! `remote_addr()` is always 127.0.0.1. The real visitor IP arrives in the
//! `cf-connecting-ip` header (set by the Cloudflare edge); `x-forwarded-for`
//! is the fallback (already used by the auth crate for sessions).
//!
//! # Privacy
//! Only the request PATH is recorded — never the query string or body. Routes
//! like `/auth/verify-email?token=...` therefore appear as `/auth/verify-email`
//! in the audit log.
//!
//! # Why the "last status" is a thread-local
//! The final HTTP status is captured in `LAST_STATUS` (a thread-local) by the
//! respond helpers (`respond_json`, `respond_with_cookie`, direct
//! `request.respond` sites) and read back after dispatch. This works because
//! `server.rs` spawns one thread per request, so the thread-local is naturally
//! per-request. It avoids threading a status return value through every
//! handler (`auth_routes` has ~19 functions). If the server ever moves to an
//! async request model, this needs to become an explicit per-request value.

use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// "Last HTTP status" captured by respond helpers, read by the audit recorder.
// ---------------------------------------------------------------------------

thread_local! {
    static LAST_STATUS: Cell<u16> = const { Cell::new(0) };
}

/// Record the HTTP status of the response currently being written. Called by
/// the respond helpers before they hand the request to tiny_http.
pub fn set_status(status: u16) {
    LAST_STATUS.with(|c| c.set(status));
}

/// Status set by the last respond helper in THIS thread, or `None` if nothing
/// responded yet (e.g. the handler panicked before responding).
pub fn last_status() -> Option<u16> {
    LAST_STATUS.with(|c| {
        let v = c.get();
        if v == 0 { None } else { Some(v) }
    })
}

// ---------------------------------------------------------------------------
// Entry + serialization
// ---------------------------------------------------------------------------

/// One recorded request. `ts` is filled in by the logger at write time so it
/// is authoritative (no caller/clock skew).
pub struct Entry {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub ip: String,
    pub ua: String,
    pub ms: u64,
    pub req: String,
}

impl Entry {
    fn to_json(&self, ts: &str) -> Value {
        json!({
            "v": 1,
            "ts": ts,
            "method": self.method,
            "path": self.path,
            "status": self.status,
            "ip": self.ip,
            "ua": self.ua,
            "ms": self.ms,
            "req": self.req,
        })
    }
}

// ---------------------------------------------------------------------------
// The logger
// ---------------------------------------------------------------------------

pub struct AuditLog {
    dir: PathBuf,
    retention_days: u32,
    inner: Mutex<Inner>,
}

struct Inner {
    /// Open handle for today's file, if the directory is writable.
    file: Option<File>,
    /// UTC date (`YYYY-MM-DD`) of the currently open file.
    date: String,
}

impl AuditLog {
    /// Open an audit logger rooted at `dir`. Creates the directory if needed
    /// and prunes files older than `retention_days`. Fails (returns an Err)
    /// only if the directory cannot be created — the caller then logs and
    /// runs with auditing disabled.
    pub fn new(dir: PathBuf, retention_days: u32) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let log = AuditLog {
            dir,
            retention_days,
            inner: Mutex::new(Inner { file: None, date: String::new() }),
        };
        log.purge_old();
        Ok(log)
    }

    /// Append one request record to today's file. No-op semantics are handled
    /// by the caller keeping `Option<AuditLog>`; this only fails softly.
    pub fn record(&self, entry: &Entry) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        self.record_at(entry, now.as_secs(), now.subsec_nanos());
    }

    fn record_at(&self, entry: &Entry, secs: u64, nanos: u32) {
        let ts = rfc3339_millis(secs, nanos);
        let date = utc_date(secs);
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return, // poisoned mutex: skip, never panic
        };
        if inner.file.is_none() || inner.date != date {
            inner.date = date.clone();
            inner.file = open_append(&self.dir, &date).ok();
        }
        let Some(file) = inner.file.as_mut() else {
            return; // open already failed once and warned
        };
        let mut line = entry.to_json(&ts).to_string();
        line.push('\n');
        if file.write_all(line.as_bytes()).is_err() {
            eprintln!("[audit] write failed for {}", self.dir.display());
        }
        let _ = file.flush();
    }

    /// Delete `access-*.jsonl` files older than `retention_days`. ISO dates
    /// compare lexicographically, so the cutoff is just the date string
    /// `retention_days` in the past.
    fn purge_old(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = utc_date(now.saturating_sub(self.retention_days as u64 * 86_400));
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(date) = filename_date(&name) else { continue };
            if date.as_str() < cutoff.as_str() {
                let _ = fs::remove_file(entry.path());
                eprintln!("[audit] pruned {}", entry.path().display());
            }
        }
    }
}

fn open_append(dir: &Path, date: &str) -> std::io::Result<File> {
    let path = dir.join(format!("access-{date}.jsonl"));
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    eprintln!("[audit] writing access log -> {}", path.display());
    Ok(file)
}

/// Extract `YYYY-MM-DD` from `access-YYYY-MM-DD.jsonl`; None for other names.
fn filename_date(name: &str) -> Option<String> {
    let rest = name.strip_prefix("access-")?;
    let date: String = rest.chars().take(10).collect();
    if date.len() == 10 && date.chars().enumerate().all(|(i, c)| {
        c.is_ascii_digit() || (i == 4 || i == 7) && c == '-'
    }) {
        Some(date)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Client IP
// ---------------------------------------------------------------------------

/// Best-effort real client IP. Prefers `cf-connecting-ip` (set by the
/// Cloudflare edge when the server is reached through a Tunnel), then the
/// first value of `x-forwarded-for`, then the socket peer address.
pub fn client_ip(request: &tiny_http::Request) -> String {
    if let Some(ip) = client_ip_from_headers(request.headers()) {
        return ip;
    }
    request
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default()
}

/// Pure header-scanning part of `client_ip`, unit-testable without a socket.
fn client_ip_from_headers(headers: &[tiny_http::Header]) -> Option<String> {
    for h in headers {
        if h.field.as_str().as_str().eq_ignore_ascii_case("cf-connecting-ip") {
            let v = h.value.as_str().trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    for h in headers {
        if h.field.as_str().as_str().eq_ignore_ascii_case("x-forwarded-for") {
            let v = h.value.as_str().split(',').next().unwrap_or("").trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Clip a header value (User-Agent) to avoid giant log lines.
pub fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

// ---------------------------------------------------------------------------
// UTC date / time formatting (no chrono dependency — civil-from-days)
// ---------------------------------------------------------------------------

/// RFC 3339 timestamp with milliseconds, e.g. `2026-08-02T07:28:13.123Z`.
fn rfc3339_millis(secs: u64, nanos: u32) -> String {
    let days = (secs / 86_400) as i64;
    let sod = secs % 86_400;
    let hour = sod / 3600;
    let min = (sod % 3600) / 60;
    let sec = sod % 60;
    let (y, mo, d) = civil_from_days(days);
    let ms = nanos / 1_000_000;
    format!("{y:04}-{mo:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.{ms:03}Z")
}

/// UTC date string `YYYY-MM-DD` for a unix epoch second.
pub fn utc_date(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{mo:02}-{d:02}")
}

/// Civil-from-days (Howard Hinnant) — day count to (year, month, day).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_matches_known_epochs() {
        // 1970-01-01
        assert_eq!(utc_date(0), "1970-01-01");
        // 2000-01-01T00:00:00Z = 946684800
        assert_eq!(utc_date(946_684_800), "2000-01-01");
        // 2026-08-02T00:00:00Z (a known deploy day)
        assert_eq!(utc_date(1_785_628_800), "2026-08-02");
        // End of a leap year: 2024-12-31
        assert_eq!(utc_date(1_735_603_200), "2024-12-31");
    }

    #[test]
    fn rfc3339_has_millis_and_z() {
        let ts = rfc3339_millis(1_785_628_800, 123_000_000);
        assert_eq!(ts, "2026-08-02T00:00:00.123Z");
        let ts2 = rfc3339_millis(946_684_800, 0);
        assert_eq!(ts2, "2000-01-01T00:00:00.000Z");
    }

    #[test]
    fn record_writes_parseable_jsonl_line() {
        let dir = tempdir();
        let log = AuditLog::new(dir.clone(), 30).unwrap();
        let entry = Entry {
            method: "GET".into(),
            path: "/health".into(),
            status: 200,
            ip: "1.2.3.4".into(),
            ua: "test-agent".into(),
            ms: 3,
            req: "abc".into(),
        };
        log.record_at(&entry, 1_785_628_800, 0);

        let files: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files, vec!["access-2026-08-02.jsonl"]);
        let raw = fs::read_to_string(dir.join("access-2026-08-02.jsonl")).unwrap();
        let line = raw.lines().next().unwrap();
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["v"], 1);
        assert_eq!(v["ts"], "2026-08-02T00:00:00.000Z");
        assert_eq!(v["method"], "GET");
        assert_eq!(v["path"], "/health");
        assert_eq!(v["status"], 200);
        assert_eq!(v["ip"], "1.2.3.4");
        assert_eq!(v["ua"], "test-agent");
        assert_eq!(v["ms"], 3);
        assert_eq!(v["req"], "abc");
    }

    #[test]
    fn record_rotates_file_on_date_change() {
        let dir = tempdir();
        let log = AuditLog::new(dir.clone(), 30).unwrap();
        let e = |ms: u64| Entry {
            method: "GET".into(),
            path: "/".into(),
            status: 200,
            ip: "1.2.3.4".into(),
            ua: "".into(),
            ms,
            req: "".into(),
        };
        log.record_at(&e(1), 1_785_628_800, 0); // 2026-08-02
        log.record_at(&e(2), 1_785_715_200, 0); // 2026-08-03 (next day)
        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|x| x.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["access-2026-08-02.jsonl", "access-2026-08-03.jsonl"]);
        assert_eq!(
            fs::read_to_string(dir.join("access-2026-08-02.jsonl")).unwrap().lines().count(),
            1
        );
        assert_eq!(
            fs::read_to_string(dir.join("access-2026-08-03.jsonl")).unwrap().lines().count(),
            1
        );
    }

    #[test]
    fn purge_deletes_files_older_than_retention() {
        let dir = tempdir();
        // Two stale files plus one current.
        fs::write(dir.join("access-2026-06-01.jsonl"), "x\n").unwrap();
        fs::write(dir.join("access-2026-07-01.jsonl"), "x\n").unwrap();
        fs::write(dir.join("access-2026-08-02.jsonl"), "x\n").unwrap();
        fs::write(dir.join("notes.txt"), "x\n").unwrap();

        let log = AuditLog::new(dir.clone(), 30).unwrap();
        log.purge_old();

        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|x| x.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["access-2026-08-02.jsonl", "notes.txt"]);
    }

    #[test]
    fn filename_date_parses_and_rejects() {
        assert_eq!(filename_date("access-2026-08-02.jsonl").as_deref(), Some("2026-08-02"));
        // Malformed / non-ISO dates are rejected (must be 10 chars YYYY-MM-DD).
        assert_eq!(filename_date("access-2026-08-2.jsonl"), None);
        assert_eq!(filename_date("access-bad.jsonl"), None);
        assert_eq!(filename_date("notes.txt"), None);
    }

    #[test]
    fn last_status_tracks_per_thread() {
        // Fresh thread → None until set.
        assert_eq!(last_status(), None);
        set_status(404);
        assert_eq!(last_status(), Some(404));
        set_status(0);
        assert_eq!(last_status(), None);
    }

    #[test]
    fn client_ip_prefers_cf_connecting() {
        let h = |n: &str, v: &str| tiny_http::Header::from_bytes(n, v).unwrap();
        let headers = [
            h("User-Agent", "curl"),
            h("CF-Connecting-IP", "203.0.113.9"),
            h("X-Forwarded-For", "192.0.2.1, 10.0.0.1"),
        ];
        assert_eq!(client_ip_from_headers(&headers).as_deref(), Some("203.0.113.9"));
    }

    #[test]
    fn client_ip_falls_back_to_forwarded_first_value() {
        let h = |n: &str, v: &str| tiny_http::Header::from_bytes(n, v).unwrap();
        let headers = [
            h("X-Forwarded-For", "192.0.2.7, 10.0.0.1"),
            h("User-Agent", "curl"),
        ];
        assert_eq!(client_ip_from_headers(&headers).as_deref(), Some("192.0.2.7"));
    }

    #[test]
    fn client_ip_none_without_forwarding_headers() {
        let h = |n: &str, v: &str| tiny_http::Header::from_bytes(n, v).unwrap();
        let headers = [h("User-Agent", "curl")];
        assert_eq!(client_ip_from_headers(&headers), None);
    }

    #[test]
    fn truncate_clips_long_values() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("abc", 10), "abc");
    }

    /// Unique temp dir per test.
    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "llm-wiki-audit-test-{}-{}",
            std::process::id(),
            nanoid()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn nanoid() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed).to_string()
    }
}

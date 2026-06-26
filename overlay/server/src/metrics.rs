//! Lightweight Prometheus text-format metrics.
//!
//! Hand-rolled (no `prometheus` crate) since the metric set is tiny — a few
//! atomic counters/gauges. Exposed at `GET /metrics` as
//! `text/plain; version=0.0.4` for scraping by Prometheus / Grafana Agent.
//!
//! Counters are process-local (single-instance deployment). For multi-instance
//! scrape each instance exposes its own `/metrics`.

use std::sync::atomic::{AtomicU64, Ordering};

static REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);
static CHAT_REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);
static ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);

pub fn inc_requests() {
    REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_chat_requests() {
    CHAT_REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_errors() {
    ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// Render the Prometheus text exposition format for the current metric
/// values. Pure (reads atomics, returns a string) so the format is
/// regression-tested.
pub fn render(in_flight: usize, in_flight_chat: usize, shutting_down: bool) -> String {
    let mut out = String::new();
    out.push_str("# HELP llm_wiki_requests_total Total HTTP requests handled.\n");
    out.push_str("# TYPE llm_wiki_requests_total counter\n");
    out.push_str(&format!(
        "llm_wiki_requests_total {}\n",
        REQUESTS_TOTAL.load(Ordering::Relaxed)
    ));
    out.push_str("# HELP llm_wiki_chat_requests_total Total chat SSE streams started.\n");
    out.push_str("# TYPE llm_wiki_chat_requests_total counter\n");
    out.push_str(&format!(
        "llm_wiki_chat_requests_total {}\n",
        CHAT_REQUESTS_TOTAL.load(Ordering::Relaxed)
    ));
    out.push_str("# HELP llm_wiki_errors_total Total requests that returned a 5xx.\n");
    out.push_str("# TYPE llm_wiki_errors_total counter\n");
    out.push_str(&format!(
        "llm_wiki_errors_total {}\n",
        ERRORS_TOTAL.load(Ordering::Relaxed)
    ));
    out.push_str("# HELP llm_wiki_in_flight_requests Requests currently being handled.\n");
    out.push_str("# TYPE llm_wiki_in_flight_requests gauge\n");
    out.push_str(&format!("llm_wiki_in_flight_requests {in_flight}\n"));
    out.push_str("# HELP llm_wiki_in_flight_chat Chat streams currently active.\n");
    out.push_str("# TYPE llm_wiki_in_flight_chat gauge\n");
    out.push_str(&format!("llm_wiki_in_flight_chat {in_flight_chat}\n"));
    out.push_str("# HELP llm_wiki_shutting_down 1 once shutdown has been requested.\n");
    out.push_str("# TYPE llm_wiki_shutting_down gauge\n");
    out.push_str(&format!(
        "llm_wiki_shutting_down {}\n",
        if shutting_down { 1 } else { 0 }
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_has_help_type_and_value_lines_for_each_metric() {
        let s = render(3, 1, false);
        // Each metric needs a HELP, a TYPE, and a value line.
        for name in [
            "llm_wiki_requests_total",
            "llm_wiki_chat_requests_total",
            "llm_wiki_errors_total",
            "llm_wiki_in_flight_requests",
            "llm_wiki_in_flight_chat",
            "llm_wiki_shutting_down",
        ] {
            assert!(s.contains(&format!("# HELP {name} ")), "missing HELP for {name}");
            assert!(s.contains(&format!("# TYPE {name} ")), "missing TYPE for {name}");
            assert!(s.contains(&format!("{name} ")), "missing value line for {name}");
        }
    }

    #[test]
    fn render_includes_gauge_values() {
        let s = render(7, 2, true);
        assert!(s.contains("llm_wiki_in_flight_requests 7"));
        assert!(s.contains("llm_wiki_in_flight_chat 2"));
        assert!(s.contains("llm_wiki_shutting_down 1"));
    }

    #[test]
    fn counters_increment() {
        // Reset by reading current then adding — tests are isolated per
        // process run, but the statics persist across tests in one run, so
        // assert monotonic increase rather than exact values.
        let before = REQUESTS_TOTAL.load(Ordering::Relaxed);
        inc_requests();
        inc_requests();
        assert_eq!(REQUESTS_TOTAL.load(Ordering::Relaxed), before + 2);
    }
}

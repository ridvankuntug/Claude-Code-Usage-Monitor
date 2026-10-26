//! Fork-only last-resort fallback: Claude Desktop's local usage history.
//!
//! The desktop app records plan usage samples in `plan-usage-history.json`
//! inside its data directory. When every upstream Claude credential path
//! fails (CLI credentials, desktop token cache, refresh, usage endpoint), the
//! newest sample there still shows the current 5-hour and weekly usage.
//! Samples carry no reset times, so both windows are reported without one.
//!
//! See `docs/claude-desktop-history-fallback-maintenance.md`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::diagnose;
use crate::models::{UsageData, UsageSection};
use crate::poller::PollError;

const HISTORY_FILE: &str = "plan-usage-history.json";

#[derive(Deserialize)]
struct PlanUsageHistory {
    samples: Vec<PlanUsageSample>,
}

#[derive(Deserialize)]
struct PlanUsageSample {
    /// Sample time in Unix milliseconds. Optional so one malformed sample does
    /// not discard the whole file; samples without it rank oldest.
    t: Option<i64>,
    u: PlanUsageValues,
}

#[derive(Deserialize)]
struct PlanUsageValues {
    /// Five-hour session utilization, in percent.
    fh: Option<f64>,
    /// Seven-day weekly utilization, in percent.
    sd: Option<f64>,
}

/// Keep `result` when upstream polling succeeded; otherwise try `fallback`
/// and, if it has nothing, return the original upstream error unchanged so
/// the UI keeps upstream's error semantics (sign-in prompts, retries).
pub(in crate::poller) fn or_desktop_history(
    result: Result<UsageData, PollError>,
    fallback: impl FnOnce() -> Option<UsageData>,
) -> Result<UsageData, PollError> {
    result.or_else(|error| {
        diagnose::log(format!(
            "Claude usage poll failed: {error:?}; trying Claude Desktop usage history"
        ));
        fallback().ok_or(error)
    })
}

/// Usage from the first readable desktop history file, if any.
///
/// Like upstream's desktop token lookup, an explicit `CLAUDE_CONFIG_DIR` is
/// an account selection and never falls back to the desktop app's data.
pub(in crate::poller) fn read_usage() -> Option<UsageData> {
    if std::env::var_os("CLAUDE_CONFIG_DIR").is_some_and(|value| !value.is_empty()) {
        diagnose::log("CLAUDE_CONFIG_DIR is set; skipping Claude Desktop usage history");
        return None;
    }
    let paths = history_paths_in(
        crate::known_folders::roaming_app_data_dir().as_deref(),
        crate::known_folders::local_app_data_dir().as_deref(),
    );
    for path in paths {
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                if diagnose::is_enabled() {
                    diagnose::log_error(
                        &format!(
                            "unable to read Claude Desktop usage history at {}",
                            path.display()
                        ),
                        error,
                    );
                }
                continue;
            }
        };

        match usage_from_history(&content) {
            Some(data) => {
                diagnose::log(format!(
                    "using Claude Desktop usage history at {}",
                    path.display()
                ));
                return Some(data);
            }
            None => diagnose::log(format!(
                "unable to parse Claude Desktop usage history at {}",
                path.display()
            )),
        }
    }

    diagnose::log("Claude Desktop usage history not found");
    None
}

/// Candidate history files in upstream's desktop data-directory order
/// (migrated `Claude-Data` first, then Roaming, then MSIX packages), plus the
/// fork's legacy `%LOCALAPPDATA%\Claude` location. Duplicates are dropped
/// while keeping the first occurrence, so upstream's preference order holds.
fn history_paths_in(roaming: Option<&Path>, local: Option<&Path>) -> Vec<PathBuf> {
    let candidates = super::data_directories_in(roaming, local)
        .into_iter()
        .chain(local.map(|local| local.join("Claude")))
        .map(|directory| directory.join(HISTORY_FILE));

    let mut paths: Vec<PathBuf> = Vec::new();
    for path in candidates {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

fn usage_from_history(content: &str) -> Option<UsageData> {
    let history: PlanUsageHistory = serde_json::from_str(content).ok()?;
    let sample = history.samples.into_iter().max_by_key(|sample| sample.t)?;

    Some(UsageData {
        session: section(sample.u.fh?),
        weekly: section(sample.u.sd?),
        ..UsageData::default()
    })
}

fn section(percentage: f64) -> UsageSection {
    UsageSection {
        available: true,
        percentage: percentage.clamp(0.0, 100.0),
        resets_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_newest_sample_by_timestamp() {
        let data = usage_from_history(
            r#"{
                "version": 2,
                "samples": [
                    {"t": 2000, "org": "newer", "u": {"fh": 42, "sd": 31}},
                    {"t": 1000, "org": "older", "u": {"fh": 99, "sd": 88}}
                ]
            }"#,
        )
        .expect("history should parse");

        assert_eq!(data.session.percentage, 42.0);
        assert_eq!(data.weekly.percentage, 31.0);
        assert!(data.session.available && data.weekly.available);
        assert!(data.session.resets_at.is_none());
        assert!(data.weekly.resets_at.is_none());
        assert!(!data.stale);
    }

    #[test]
    fn clamps_percentages() {
        let data = usage_from_history(r#"{"samples": [{"t": 1, "u": {"fh": -5, "sd": 120}}]}"#)
            .expect("history should parse");

        assert_eq!(data.session.percentage, 0.0);
        assert_eq!(data.weekly.percentage, 100.0);
    }

    #[test]
    fn samples_without_a_timestamp_rank_oldest() {
        let data = usage_from_history(
            r#"{"samples": [
                {"u": {"fh": 90, "sd": 90}},
                {"t": 5, "u": {"fh": 10, "sd": 20}}
            ]}"#,
        )
        .expect("history should parse");
        assert_eq!(data.session.percentage, 10.0);

        let data = usage_from_history(r#"{"samples": [{"u": {"fh": 7, "sd": 8}}]}"#)
            .expect("a lone untimed sample is still usable");
        assert_eq!(data.weekly.percentage, 8.0);
    }

    #[test]
    fn rejects_empty_incomplete_or_malformed_history() {
        assert!(usage_from_history(r#"{"samples": []}"#).is_none());
        assert!(usage_from_history(r#"{"samples": [{"t": 1, "u": {"fh": 5}}]}"#).is_none());
        assert!(usage_from_history("not json").is_none());
    }

    #[test]
    fn discovers_history_in_upstream_data_directories_without_duplicates() {
        let temporary = tempfile::tempdir().unwrap();
        let roaming = temporary.path().join("Roaming");
        let local = temporary.path().join("Local");
        let migrated = local.join("Claude-Data").join("claude_pzs8sxrjxfjjc");
        let store = local.join("Packages").join("Claude_pzs8sxrjxfjjc");
        std::fs::create_dir_all(&migrated).unwrap();
        std::fs::create_dir_all(&store).unwrap();

        assert_eq!(
            history_paths_in(Some(&roaming), Some(&local)),
            vec![
                migrated.join(HISTORY_FILE),
                roaming.join("Claude").join(HISTORY_FILE),
                store.join("LocalCache/Roaming/Claude").join(HISTORY_FILE),
                local.join("Claude").join(HISTORY_FILE),
            ]
        );

        // Roaming and Local resolving to the same folder must not repeat a file.
        let same = temporary.path().join("Same");
        assert_eq!(
            history_paths_in(Some(&same), Some(&same)),
            vec![same.join("Claude").join(HISTORY_FILE)]
        );
        assert!(history_paths_in(None, None).is_empty());
    }

    #[test]
    fn fallback_runs_only_after_failure_and_keeps_the_original_error() {
        let usage = || usage_from_history(r#"{"samples": [{"t": 1, "u": {"fh": 3, "sd": 4}}]}"#);

        let upstream = UsageData {
            session: section(50.0),
            ..UsageData::default()
        };
        let kept = or_desktop_history(Ok(upstream.clone()), || {
            panic!("fallback must not run after a successful poll")
        });
        assert_eq!(kept, Ok(upstream));

        let recovered = or_desktop_history(Err(PollError::NoCredentials), usage).unwrap();
        assert_eq!(recovered.session.percentage, 3.0);

        for error in [
            PollError::AuthRequired,
            PollError::TokenExpired,
            PollError::NoCredentials,
            PollError::NetworkError,
            PollError::HttpStatus(429),
        ] {
            assert_eq!(or_desktop_history(Err(error), || None), Err(error));
        }
    }
}

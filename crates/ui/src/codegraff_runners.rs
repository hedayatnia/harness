//! GitHub CI runners for Settings → GitHub CI (the engine's `CodegraffGithubRunners`).
//!
//! Pure reading of the engine's answer and every text the page shows, so the page stays a thin
//! renderer: which panel to show, budgets and spend, repository size choices with their hourly
//! price, and the recent jobs list.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::codegraff_sandboxes::format_rate;

/// The page's answer from the engine. Tolerant of fields added later.
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GithubRunners {
    /// The gateway knows runners at all.
    pub available: bool,
    /// Runners are open to this account (`false`: coming soon).
    pub enabled: bool,
    pub install_url: Option<String>,
    pub installations: Vec<RunnerInstallation>,
    pub recent_jobs: Vec<RunnerJob>,
    pub sizes: Vec<RunnerSize>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerInstallation {
    pub installation_id: String,
    pub account: String,
    pub account_type: Option<String>,
    pub suspended: bool,
    pub monthly_budget_micro_usd: Option<i64>,
    pub untrusted_budget_micro_usd: Option<i64>,
    pub used_this_month_micro_usd: Option<i64>,
    pub max_concurrent: Option<u32>,
    pub repos: Vec<RunnerRepo>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerRepo {
    pub full_name: String,
    pub enabled: bool,
    pub default_size: Option<String>,
    pub private: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerJob {
    pub repo: String,
    pub run_id: Option<i64>,
    pub job_id: Option<i64>,
    pub workflow: Option<String>,
    pub job_name: Option<String>,
    pub tier: Option<String>,
    pub size: Option<String>,
    pub state: String,
    pub queued_at: Option<Value>,
    pub started_at: Option<Value>,
    pub completed_at: Option<Value>,
    pub duration_seconds: Option<f64>,
    pub cost_micro_usd: Option<i64>,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerSize {
    pub key: String,
    pub cpu: Option<f64>,
    #[serde(rename = "memoryGiB")]
    pub memory_gib: Option<f64>,
    #[serde(rename = "diskGiB")]
    pub disk_gib: Option<f64>,
    pub price_per_hour_micro_usd: Option<i64>,
}

/// The most concurrent jobs an installation may run.
pub const MAX_CONCURRENT: u32 = 8;
/// After Connect, how often the page re-reads while the user installs the app in the browser,
/// and for how long.
pub const CONNECT_POLL_SECS: u64 = 10;
pub const CONNECT_WATCH_SECS: u64 = 5 * 60;

/// What a workflow says to run on Codegraff.
pub const YAML_SNIPPET: &str =
    "jobs:\n  build:\n    runs-on: codegraff\n    steps:\n      - uses: actions/checkout@v4\n";

/// The not-connected explanation.
pub const EXPLAINER: &str = "Jobs with runs-on: codegraff run in a fresh Codegraff sandbox per job, billed per second from your Codegraff credit. Pull requests from forks run in a smaller, isolated tier. For public repositories, require approval for workflows from outside contributors.";

/// Which panel the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Signed out of Codegraff.
    SignedOut,
    /// The gateway has no runners yet (404).
    Unavailable,
    /// Runners exist but aren't open to this account yet.
    ComingSoon,
    /// Open, nothing installed: explain and offer Connect.
    NotConnected,
    Connected,
}

/// Pure.
pub fn panel(runners: Option<&GithubRunners>) -> Panel {
    match runners {
        None => Panel::SignedOut,
        Some(r) if !r.available => Panel::Unavailable,
        Some(r) if !r.enabled => Panel::ComingSoon,
        Some(r) if r.installations.is_empty() => Panel::NotConnected,
        Some(_) => Panel::Connected,
    }
}

/// The link Connect opens: only an https one.
pub fn install_url(runners: &GithubRunners) -> Option<&str> {
    runners
        .install_url
        .as_deref()
        .map(str::trim)
        .filter(|url| url.starts_with("https://"))
}

/// "$50", "$12.50", "$0.0042" (sub-cent amounts keep their digits). Pure.
pub fn format_dollars(micro_usd: i64) -> String {
    let sign = if micro_usd < 0 { "-" } else { "" };
    let micro = micro_usd.unsigned_abs();
    if micro == 0 {
        return "$0".into();
    }
    if micro % 1_000_000 == 0 {
        return format!("{sign}${}", micro / 1_000_000);
    }
    let dollars = micro as f64 / 1_000_000.0;
    if micro < 10_000 {
        let text = format!("{dollars:.4}");
        let text = text.trim_end_matches('0');
        return format!("{sign}${text}");
    }
    format!("{sign}${dollars:.2}")
}

/// A budget as typed ("50", "$50", "1,250.50") in micro-USD. Pure.
pub fn parse_budget(text: &str) -> Result<i64, String> {
    let cleaned: String = text
        .trim()
        .trim_start_matches('$')
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        return Err("Enter a budget in dollars.".into());
    }
    let (whole, cents) = match cleaned.split_once('.') {
        Some((whole, cents)) => (whole, cents),
        None => (cleaned.as_str(), ""),
    };
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !digits(whole) || !digits(cents) || (whole.is_empty() && cents.is_empty()) {
        return Err("Enter a dollar amount, like 50 or 12.50.".into());
    }
    if cents.len() > 2 {
        return Err("Use at most two decimal places.".into());
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .map_err(|_| "That budget is too large.".to_string())?
    };
    if whole > 100_000 {
        return Err("Budgets go up to $100,000 a month.".into());
    }
    let cents: i64 = format!("{cents:0<2}").parse().unwrap_or(0);
    Ok(whole * 1_000_000 + cents * 10_000)
}

/// The budget as the input shows it: "50", "12.50". Pure.
pub fn budget_input_text(micro_usd: Option<i64>) -> String {
    match micro_usd {
        None => String::new(),
        Some(micro) => format_dollars(micro).trim_start_matches('$').to_string(),
    }
}

/// How much of the month's budget is used, for the meter (`None` without a budget). Pure.
pub fn budget_fraction(installation: &RunnerInstallation) -> Option<f32> {
    let budget = installation.monthly_budget_micro_usd?;
    let used = installation.used_this_month_micro_usd.unwrap_or(0).max(0);
    Some(if budget <= 0 {
        1.0
    } else {
        (used as f64 / budget as f64).clamp(0.0, 1.0) as f32
    })
}

/// "$12.50 of $50 used this month" / "$12.50 used this month · no monthly budget". Pure.
pub fn spend_line(installation: &RunnerInstallation) -> String {
    let used = format_dollars(installation.used_this_month_micro_usd.unwrap_or(0));
    match installation.monthly_budget_micro_usd {
        Some(budget) => format!("{used} of {} used this month", format_dollars(budget)),
        None => format!("{used} used this month · no monthly budget"),
    }
}

/// "Organization · suspended" and the like, under the account name. Pure.
pub fn installation_meta(installation: &RunnerInstallation) -> String {
    let mut parts = Vec::new();
    if let Some(kind) = installation
        .account_type
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        parts.push(kind.to_string());
    }
    let enabled = installation.repos.iter().filter(|r| r.enabled).count();
    parts.push(match (installation.repos.len(), enabled) {
        (0, _) => "No repositories".to_string(),
        (n, e) => format!(
            "{e} of {n} repositor{} on",
            if n == 1 { "y" } else { "ies" }
        ),
    });
    if installation.suspended {
        parts.push("Suspended on GitHub".into());
    }
    parts.join(" · ")
}

/// The concurrent-job cap, kept within 1…[`MAX_CONCURRENT`]. Pure.
pub fn step_concurrent(current: Option<u32>, delta: i32) -> u32 {
    let current = current.unwrap_or(1).clamp(1, MAX_CONCURRENT) as i32;
    (current + delta).clamp(1, MAX_CONCURRENT as i32) as u32
}

/// "Standard", "Large", or the key capitalised.
pub fn size_name(key: &str) -> String {
    let mut chars = key.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Default".into(),
    }
}

/// "Standard · 2 vCPU · 4 GB · $0.10/h". Pure.
pub fn size_option_label(size: &RunnerSize) -> String {
    let mut parts = vec![size_name(&size.key)];
    parts.extend(crate::codegraff_sandboxes::machine_spec(
        size.cpu,
        size.memory_gib,
    ));
    parts.extend(size.price_per_hour_micro_usd.map(format_rate));
    parts.join(" · ")
}

/// The repository's size as its dropdown shows it: the named size, else the first offered. Pure.
pub fn repo_size_label(repo: &RunnerRepo, sizes: &[RunnerSize]) -> String {
    let key = repo
        .default_size
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    let size = key
        .and_then(|key| sizes.iter().find(|s| s.key == key))
        .or_else(|| if key.is_none() { sizes.first() } else { None });
    match (size, key) {
        (Some(size), _) => {
            let mut text = size_name(&size.key);
            if let Some(price) = size.price_per_hour_micro_usd {
                text = format!("{text} · {}", format_rate(price));
            }
            text
        }
        (None, Some(key)) => size_name(key),
        (None, None) => "Default size".into(),
    }
}

/// "CI · test", falling back to whichever is known. Pure.
pub fn job_title(job: &RunnerJob) -> String {
    let parts: Vec<&str> = [job.workflow.as_deref(), job.job_name.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        "Job".into()
    } else {
        parts.join(" · ")
    }
}

/// How a job's state reads, and whether it is good, bad or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobTone {
    Working,
    Good,
    Bad,
    Quiet,
}

/// "Queued", "Running", "Succeeded", "Failed", "Cancelled", … Pure.
pub fn job_status(job: &RunnerJob) -> (String, JobTone) {
    let conclusion = job.conclusion.as_deref().map(str::trim).unwrap_or("");
    match (job.state.trim(), conclusion) {
        ("queued" | "waiting" | "pending", _) => ("Queued".into(), JobTone::Working),
        ("in_progress" | "running" | "started", _) => ("Running".into(), JobTone::Working),
        (_, "success") => ("Succeeded".into(), JobTone::Good),
        (_, "failure" | "timed_out" | "startup_failure") => ("Failed".into(), JobTone::Bad),
        (_, "cancelled") => ("Cancelled".into(), JobTone::Quiet),
        (_, "skipped" | "neutral") => ("Skipped".into(), JobTone::Quiet),
        ("", "") => ("Unknown".into(), JobTone::Quiet),
        (state, "") => (size_name(&state.replace('_', " ")), JobTone::Quiet),
        (_, other) => (size_name(&other.replace('_', " ")), JobTone::Quiet),
    }
}

/// "45s", "3m 5s", "1h 2m". Pure.
pub fn format_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {}s", s / 60, s % 60),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

fn moment(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.with_timezone(&Utc)),
        Value::Number(n) => {
            let n = n.as_i64()?;
            // Milliseconds or seconds.
            if n > 100_000_000_000 {
                DateTime::from_timestamp_millis(n)
            } else {
                DateTime::from_timestamp(n, 0)
            }
        }
        _ => None,
    }
}

/// "just now", "5 min ago", "3h ago", "2d ago". Pure.
fn ago(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (now - at).num_minutes();
    match minutes {
        m if m < 1 => "just now".into(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 60 * 24 => format!("{}h ago", m / 60),
        m => format!("{}d ago", m / (60 * 24)),
    }
}

/// "Large · 3m 5s · $0.0050 · 5 min ago · fork". Pure.
pub fn job_meta(job: &RunnerJob, now: DateTime<Utc>) -> String {
    let mut parts = Vec::new();
    if let Some(size) = job.size.as_deref().filter(|s| !s.trim().is_empty()) {
        parts.push(size_name(size));
    }
    let duration = job.duration_seconds.or_else(|| {
        let start = moment(job.started_at.as_ref()?)?;
        let end = moment(job.completed_at.as_ref()?)?;
        Some((end - start).num_seconds() as f64)
    });
    if let Some(duration) = duration {
        parts.push(format_duration(duration));
    }
    if let Some(cost) = job.cost_micro_usd {
        parts.push(format_dollars(cost));
    }
    if let Some(at) = [&job.completed_at, &job.started_at, &job.queued_at]
        .into_iter()
        .find_map(|at| at.as_ref().and_then(moment))
    {
        parts.push(ago(at, now));
    }
    if job.tier.as_deref() == Some("untrusted") {
        parts.push("Fork, isolated".into());
    }
    parts.join(" · ")
}

/// The job's page on GitHub, when it is an https link.
pub fn job_url(job: &RunnerJob) -> Option<&str> {
    job.html_url
        .as_deref()
        .map(str::trim)
        .filter(|url| url.starts_with("https://"))
}

/// Anything still queued or running moves on its own, so the page re-reads while one is. Pure.
pub fn jobs_working(runners: &GithubRunners) -> bool {
    runners
        .recent_jobs
        .iter()
        .any(|job| job_status(job).1 == JobTone::Working)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn listed() -> GithubRunners {
        serde_json::from_value(json!({
            "available": true, "enabled": true,
            "installUrl": "https://github.com/apps/codegraff/installations/new",
            "installations": [{
                "installationId": "4242", "account": "octo-org", "accountType": "Organization",
                "suspended": false, "monthlyBudgetMicroUsd": 50000000,
                "usedThisMonthMicroUsd": 12500000, "maxConcurrent": 3,
                "repos": [
                    {"fullName": "octo-org/app", "enabled": true, "defaultSize": "large", "private": true},
                    {"fullName": "octo-org/docs", "enabled": false}
                ]
            }],
            "recentJobs": [],
            "sizes": [
                {"key": "standard", "cpu": 2, "memoryGiB": 4, "diskGiB": 40, "pricePerHourMicroUsd": 100000},
                {"key": "large", "cpu": 4, "memoryGiB": 8, "diskGiB": 40, "pricePerHourMicroUsd": 200000}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn the_panel_follows_sign_in_availability_and_installs() {
        assert_eq!(panel(None), Panel::SignedOut);
        assert_eq!(panel(Some(&GithubRunners::default())), Panel::Unavailable);
        let soon = GithubRunners {
            available: true,
            ..Default::default()
        };
        assert_eq!(panel(Some(&soon)), Panel::ComingSoon);
        let open = GithubRunners {
            available: true,
            enabled: true,
            ..Default::default()
        };
        assert_eq!(panel(Some(&open)), Panel::NotConnected);
        assert_eq!(panel(Some(&listed())), Panel::Connected);
        let mut insecure = listed();
        insecure.install_url = Some("http://example.invalid".into());
        assert_eq!(install_url(&insecure), None);
        assert!(install_url(&listed()).is_some());
    }

    #[test]
    fn dollars_read_plainly() {
        assert_eq!(format_dollars(50_000_000), "$50");
        assert_eq!(format_dollars(12_500_000), "$12.50");
        assert_eq!(format_dollars(5_000), "$0.005");
        assert_eq!(format_dollars(4_200), "$0.0042");
        assert_eq!(format_dollars(0), "$0");
        assert_eq!(budget_input_text(Some(12_500_000)), "12.50");
        assert_eq!(budget_input_text(None), "");
    }

    #[test]
    fn budgets_parse_and_refuse_nonsense() {
        assert_eq!(parse_budget("50"), Ok(50_000_000));
        assert_eq!(parse_budget(" $1,250.5 "), Ok(1_250_500_000));
        assert_eq!(parse_budget("0.25"), Ok(250_000));
        assert_eq!(parse_budget(".5"), Ok(500_000));
        assert_eq!(parse_budget("0"), Ok(0));
        assert!(parse_budget("").is_err());
        assert!(parse_budget("-5").is_err());
        assert!(parse_budget("abc").is_err());
        assert!(parse_budget("1.234").is_err());
        assert!(parse_budget("1.2.3").is_err());
        assert!(parse_budget(".").is_err());
        assert!(parse_budget("100001").is_err());
        assert!(parse_budget("99999999999999999999").is_err());
    }

    #[test]
    fn spend_and_the_meter_follow_the_budget() {
        let install = &listed().installations[0];
        assert_eq!(spend_line(install), "$12.50 of $50 used this month");
        assert_eq!(budget_fraction(install), Some(0.25));
        assert_eq!(
            installation_meta(install),
            "Organization · 1 of 2 repositories on"
        );
        let mut open_ended = install.clone();
        open_ended.monthly_budget_micro_usd = None;
        open_ended.suspended = true;
        open_ended.repos.truncate(1);
        assert_eq!(budget_fraction(&open_ended), None);
        assert_eq!(
            spend_line(&open_ended),
            "$12.50 used this month · no monthly budget"
        );
        assert_eq!(
            installation_meta(&open_ended),
            "Organization · 1 of 1 repository on · Suspended on GitHub"
        );
        let mut zero = install.clone();
        zero.monthly_budget_micro_usd = Some(0);
        assert_eq!(budget_fraction(&zero), Some(1.0));
    }

    #[test]
    fn concurrency_stays_between_one_and_eight() {
        assert_eq!(step_concurrent(Some(3), 1), 4);
        assert_eq!(step_concurrent(Some(8), 1), 8);
        assert_eq!(step_concurrent(Some(1), -1), 1);
        assert_eq!(step_concurrent(None, 1), 2);
        assert_eq!(step_concurrent(Some(40), -1), 7);
    }

    #[test]
    fn sizes_show_their_machine_and_rate() {
        let runners = listed();
        assert_eq!(
            size_option_label(&runners.sizes[0]),
            "Standard · 2 vCPU · 4 GB · $0.10/h"
        );
        let repos = &runners.installations[0].repos;
        assert_eq!(
            repo_size_label(&repos[0], &runners.sizes),
            "Large · $0.20/h"
        );
        assert_eq!(
            repo_size_label(&repos[1], &runners.sizes),
            "Standard · $0.10/h",
            "no size named: the first"
        );
        let gone = RunnerRepo {
            default_size: Some("xl".into()),
            ..Default::default()
        };
        assert_eq!(repo_size_label(&gone, &runners.sizes), "Xl");
        assert_eq!(repo_size_label(&RunnerRepo::default(), &[]), "Default size");
    }

    #[test]
    fn jobs_say_what_ran_how_it_went_and_what_it_cost() {
        let now = Utc.with_ymd_and_hms(2026, 10, 22, 10, 8, 5).unwrap();
        let job: RunnerJob = serde_json::from_value(json!({
            "repo": "octo-org/app", "workflow": "CI", "jobName": "test", "size": "standard",
            "state": "completed", "conclusion": "success",
            "startedAt": "2026-10-22T10:00:05Z", "completedAt": "2026-10-22T10:03:05Z",
            "costMicroUsd": 5000, "tier": "untrusted",
            "htmlUrl": "https://github.com/octo-org/app/actions/runs/1/job/2"
        }))
        .unwrap();
        assert_eq!(job_title(&job), "CI · test");
        assert_eq!(job_status(&job), ("Succeeded".into(), JobTone::Good));
        assert_eq!(
            job_meta(&job, now),
            "Standard · 3m 0s · $0.005 · 5 min ago · Fork, isolated"
        );
        assert!(job_url(&job).is_some());

        let queued = RunnerJob {
            state: "queued".into(),
            ..Default::default()
        };
        assert_eq!(job_status(&queued).0, "Queued");
        assert_eq!(job_title(&queued), "Job");
        assert_eq!(job_meta(&queued, now), "");
        let failed = RunnerJob {
            state: "completed".into(),
            conclusion: Some("failure".into()),
            duration_seconds: Some(3725.0),
            html_url: Some("javascript:alert(1)".into()),
            ..Default::default()
        };
        assert_eq!(job_status(&failed), ("Failed".into(), JobTone::Bad));
        assert_eq!(job_meta(&failed, now), "1h 2m");
        assert_eq!(job_url(&failed), None);
        let mut runners = listed();
        assert!(!jobs_working(&runners));
        runners.recent_jobs.push(queued);
        assert!(jobs_working(&runners));
        assert_eq!(format_duration(45.4), "45s");
    }
}

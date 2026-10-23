//! Codegraff GitHub CI runners (the gateway's `/v1/github/runners`) for Settings → GitHub CI.
//!
//! Jobs with `runs-on: codegraff` run in a fresh Codegraff sandbox per job, billed per second
//! from the account's credit. The account connects GitHub by installing the Codegraff app in the
//! browser (`installUrl`); after that each installation has budgets, a cap on concurrent jobs and
//! a per-repository switch and default size, all changed here with the same sign-in the sandbox
//! calls use.
//!
//! Gateways without runners answer 404: the list then says `available: false` and the page
//! explains instead of failing. Every field is read leniently (missing or mistyped values take
//! their defaults), so a gateway that adds or reshapes a field never breaks the page.

use std::time::Duration;

use reqwest::Method;
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{CodegraffAuth, urlencode};

const LIST: &str = "/v1/github/runners";
const QUICK: Duration = Duration::from_secs(20);
/// The most concurrent jobs one installation may ask for.
pub const MAX_CONCURRENT: u32 = 8;

/// What the page shows. `available: false` (and nothing else) from a gateway without runners.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GithubRunners {
    /// The gateway knows runners at all (false on a 404).
    pub available: bool,
    /// Runners are open to this account; `false` reads as "coming soon".
    #[serde(deserialize_with = "lenient")]
    pub enabled: bool,
    #[serde(deserialize_with = "lenient")]
    pub install_url: Option<String>,
    #[serde(deserialize_with = "lenient_vec")]
    pub installations: Vec<RunnerInstallation>,
    #[serde(deserialize_with = "lenient_vec")]
    pub recent_jobs: Vec<RunnerJob>,
    #[serde(deserialize_with = "lenient_vec")]
    pub sizes: Vec<RunnerSize>,
}

/// One GitHub account (user or organization) the Codegraff app is installed on.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerInstallation {
    #[serde(deserialize_with = "id_text")]
    pub installation_id: String,
    #[serde(deserialize_with = "lenient")]
    pub account: String,
    /// `User` or `Organization`.
    #[serde(deserialize_with = "lenient")]
    pub account_type: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub suspended: bool,
    #[serde(deserialize_with = "micro")]
    pub monthly_budget_micro_usd: Option<i64>,
    /// The budget for jobs from outside contributors (fork pull requests).
    #[serde(deserialize_with = "micro")]
    pub untrusted_budget_micro_usd: Option<i64>,
    #[serde(deserialize_with = "micro")]
    pub used_this_month_micro_usd: Option<i64>,
    #[serde(deserialize_with = "lenient")]
    pub max_concurrent: Option<u32>,
    #[serde(deserialize_with = "lenient_vec")]
    pub repos: Vec<RunnerRepo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerRepo {
    #[serde(deserialize_with = "lenient")]
    pub full_name: String,
    #[serde(deserialize_with = "lenient")]
    pub enabled: bool,
    /// A size `key` from the list's `sizes`.
    #[serde(deserialize_with = "lenient")]
    pub default_size: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub private: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerJob {
    #[serde(deserialize_with = "lenient")]
    pub repo: String,
    #[serde(deserialize_with = "id_number")]
    pub run_id: Option<i64>,
    #[serde(deserialize_with = "id_number")]
    pub job_id: Option<i64>,
    #[serde(deserialize_with = "lenient")]
    pub workflow: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub job_name: Option<String>,
    /// `trusted` or `untrusted` (fork pull requests run in a smaller, isolated tier).
    #[serde(deserialize_with = "lenient")]
    pub tier: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub size: Option<String>,
    /// `queued`, `in_progress`, `completed`, …
    #[serde(deserialize_with = "lenient")]
    pub state: String,
    #[serde(deserialize_with = "lenient")]
    pub queued_at: Option<Value>,
    #[serde(deserialize_with = "lenient")]
    pub started_at: Option<Value>,
    #[serde(deserialize_with = "lenient")]
    pub completed_at: Option<Value>,
    #[serde(deserialize_with = "lenient")]
    pub duration_seconds: Option<f64>,
    #[serde(deserialize_with = "micro")]
    pub cost_micro_usd: Option<i64>,
    /// `success`, `failure`, `cancelled`, …
    #[serde(deserialize_with = "lenient")]
    pub conclusion: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunnerSize {
    #[serde(deserialize_with = "lenient")]
    pub key: String,
    #[serde(deserialize_with = "lenient")]
    pub cpu: Option<f64>,
    #[serde(rename = "memoryGiB", deserialize_with = "lenient")]
    pub memory_gib: Option<f64>,
    #[serde(rename = "diskGiB", deserialize_with = "lenient")]
    pub disk_gib: Option<f64>,
    #[serde(deserialize_with = "micro")]
    pub price_per_hour_micro_usd: Option<i64>,
}

/// One change to an installation, as the UI sends it over RPC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum RunnerAction {
    /// Budgets and the concurrent-job cap; absent fields are left as they are.
    #[serde(rename_all = "camelCase")]
    Installation {
        #[serde(deserialize_with = "id_text")]
        installation_id: String,
        #[serde(default)]
        monthly_budget_micro_usd: Option<i64>,
        #[serde(default)]
        untrusted_budget_micro_usd: Option<i64>,
        #[serde(default)]
        max_concurrent: Option<u32>,
    },
    /// Switch one repository on or off, or change its default size.
    #[serde(rename_all = "camelCase")]
    Repo {
        #[serde(deserialize_with = "id_text")]
        installation_id: String,
        full_name: String,
        #[serde(default)]
        enabled: Option<bool>,
        #[serde(default)]
        default_size: Option<String>,
    },
    /// Remove the installation from the account.
    #[serde(rename_all = "camelCase")]
    Disconnect {
        #[serde(deserialize_with = "id_text")]
        installation_id: String,
    },
}

/// `(method, path, body)` for an action, or why it can't be sent. Pure.
fn request_for(action: &RunnerAction) -> Result<(Method, String, Option<Value>), String> {
    let installation = |id: &str| {
        if id.trim().is_empty() {
            Err("Pick a GitHub installation first.".to_string())
        } else {
            Ok(format!("{LIST}/installations/{}", urlencode(id)))
        }
    };
    match action {
        RunnerAction::Installation {
            installation_id,
            monthly_budget_micro_usd,
            untrusted_budget_micro_usd,
            max_concurrent,
        } => {
            let mut body = Map::new();
            for (key, amount) in [
                ("monthlyBudgetMicroUsd", monthly_budget_micro_usd),
                ("untrustedBudgetMicroUsd", untrusted_budget_micro_usd),
            ] {
                if let Some(amount) = amount {
                    if *amount < 0 {
                        return Err("A budget can't be negative.".into());
                    }
                    body.insert(key.into(), Value::from(*amount));
                }
            }
            if let Some(max) = max_concurrent {
                if !(1..=MAX_CONCURRENT).contains(max) {
                    return Err(format!(
                        "Concurrent jobs must be between 1 and {MAX_CONCURRENT}."
                    ));
                }
                body.insert("maxConcurrent".into(), Value::from(*max));
            }
            if body.is_empty() {
                return Err("Nothing to change.".into());
            }
            Ok((
                Method::PATCH,
                installation(installation_id)?,
                Some(Value::Object(body)),
            ))
        }
        RunnerAction::Repo {
            installation_id,
            full_name,
            enabled,
            default_size,
        } => {
            let full_name = full_name.trim();
            if full_name.is_empty() {
                return Err("Pick a repository first.".into());
            }
            let mut body = Map::new();
            body.insert("fullName".into(), Value::from(full_name));
            if let Some(enabled) = enabled {
                body.insert("enabled".into(), Value::from(*enabled));
            }
            if let Some(size) = default_size.as_deref().map(str::trim) {
                if size.is_empty() {
                    return Err("Pick a size.".into());
                }
                body.insert("defaultSize".into(), Value::from(size));
            }
            if body.len() == 1 {
                return Err("Nothing to change.".into());
            }
            Ok((
                Method::PATCH,
                format!("{}/repos", installation(installation_id)?),
                Some(Value::Object(body)),
            ))
        }
        RunnerAction::Disconnect { installation_id } => {
            Ok((Method::DELETE, installation(installation_id)?, None))
        }
    }
}

/// A gateway without the runner routes.
fn missing(status: u16) -> bool {
    matches!(status, 404 | 501)
}

/// A 404 that says no more than "not found" (an unknown route), as opposed to one naming what
/// is missing (an installation that is gone), whose words are worth showing.
fn generic_message(body: Option<&Value>) -> bool {
    let message = body
        .and_then(|body| {
            body.pointer("/error/message")
                .or_else(|| body.get("error"))
                .or_else(|| body.get("message"))
        })
        .and_then(Value::as_str)
        .map(|m| m.trim().trim_end_matches('.').to_ascii_lowercase());
    match message.as_deref() {
        None | Some("") => true,
        Some(message) => matches!(message, "not found" | "not_found" | "404 not found"),
    }
}

impl CodegraffAuth {
    /// The account's runners (`None` when signed out). A gateway without them answers
    /// `available: false` instead of an error.
    pub async fn github_runners(&self) -> Result<Option<GithubRunners>, String> {
        if self.current_key().is_none() {
            return Ok(None);
        }
        match self.sandbox_call(Method::GET, LIST, None, QUICK).await {
            Ok(value) => {
                let mut runners: GithubRunners = serde_json::from_value(value).unwrap_or_default();
                runners.available = true;
                runners.install_url = runners
                    .install_url
                    .filter(|url| url.starts_with("https://"));
                Ok(Some(runners))
            }
            Err(failure) if missing(failure.status) => Ok(Some(GithubRunners::default())),
            Err(failure) => Err(failure.sentence("GitHub CI is unavailable right now")),
        }
    }

    pub async fn github_runner_action(&self, action: RunnerAction) -> Result<(), String> {
        let (method, path, body) = request_for(&action)?;
        let fallback = match action {
            RunnerAction::Installation { .. } => "CodeGraff couldn't save the GitHub CI settings",
            RunnerAction::Repo { .. } => "CodeGraff couldn't change the repository",
            RunnerAction::Disconnect { .. } => "CodeGraff couldn't disconnect GitHub",
        };
        self.sandbox_call(method, &path, body, QUICK)
            .await
            .map(drop)
            .map_err(|failure| {
                if missing(failure.status) && generic_message(failure.body.as_ref()) {
                    "GitHub CI isn't available on Codegraff yet.".to_string()
                } else {
                    failure.sentence(fallback)
                }
            })
    }
}

/// Any JSON; a value of the wrong shape reads as the default.
fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// A list whose unreadable items are dropped rather than costing the whole list.
fn lenient_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    })
}

/// An id GitHub may send as a number or a string, kept as text.
fn id_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    })
}

/// A number, or numeric text, as a whole number.
fn whole(value: &Value) -> Option<i64> {
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    (number.is_finite() && number.abs() < 9.0e15).then(|| number.round() as i64)
}

fn id_number<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(whole(&Value::deserialize(deserializer)?))
}

/// Micro-USD amounts: a number (rounded) or numeric text; anything else is unknown.
fn micro<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(whole(&Value::deserialize(deserializer)?))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::sandboxes::tests::{fake, recorded, signed_in};
    use super::*;

    const LISTED: &str = r#"{
        "enabled": true,
        "installUrl": "https://github.com/apps/codegraff/installations/new",
        "installations": [{
            "installationId": 4242, "account": "octo-org", "accountType": "Organization",
            "suspended": false, "monthlyBudgetMicroUsd": 50000000,
            "untrustedBudgetMicroUsd": 5000000, "usedThisMonthMicroUsd": 1250000.4,
            "maxConcurrent": 3,
            "repos": [
                {"fullName": "octo-org/app", "enabled": true, "defaultSize": "standard", "private": true},
                {"fullName": "octo-org/docs", "enabled": false, "defaultSize": null, "private": false},
                "not a repo"
            ],
            "future": {"field": 1}
        }],
        "recentJobs": [{
            "repo": "octo-org/app", "runId": 77, "jobId": "88", "workflow": "CI", "jobName": "test",
            "tier": "trusted", "size": "standard", "state": "completed",
            "queuedAt": "2026-10-22T10:00:00Z", "startedAt": "2026-10-22T10:00:05Z",
            "completedAt": "2026-10-22T10:03:05Z", "durationSeconds": 180,
            "costMicroUsd": 5000, "conclusion": "success",
            "htmlUrl": "https://github.com/octo-org/app/actions/runs/77/job/88"
        }],
        "sizes": [
            {"key": "standard", "cpu": 2, "memoryGiB": 4, "diskGiB": 40, "pricePerHourMicroUsd": 100000},
            {"key": "large", "cpu": 4, "memoryGiB": 8, "diskGiB": 40, "pricePerHourMicroUsd": 200000}
        ]
    }"#;

    #[tokio::test]
    async fn the_list_reads_installations_repos_jobs_and_sizes() {
        let (gateway, seen) = fake(vec![("GET /v1/github/runners ", 200, LISTED)]).await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let runners = auth.github_runners().await.unwrap().unwrap();
        assert!(runners.available && runners.enabled);
        assert_eq!(
            runners.install_url.as_deref(),
            Some("https://github.com/apps/codegraff/installations/new")
        );
        let install = &runners.installations[0];
        assert_eq!(install.installation_id, "4242");
        assert_eq!(install.account, "octo-org");
        assert_eq!(install.monthly_budget_micro_usd, Some(50_000_000));
        assert_eq!(install.used_this_month_micro_usd, Some(1_250_000));
        assert_eq!(install.max_concurrent, Some(3));
        assert_eq!(install.repos.len(), 2, "an unreadable repo is dropped");
        assert_eq!(install.repos[1].default_size, None);
        let job = &runners.recent_jobs[0];
        assert_eq!((job.run_id, job.job_id), (Some(77), Some(88)));
        assert_eq!(job.cost_micro_usd, Some(5000));
        assert_eq!(job.duration_seconds, Some(180.0));
        assert_eq!(runners.sizes[1].key, "large");
        assert_eq!(runners.sizes[1].price_per_hour_micro_usd, Some(200_000));
        assert!(
            seen.lock().unwrap()[0].contains("Bearer cg_sk_test"),
            "the sandbox sign-in"
        );
    }

    #[test]
    fn missing_and_mistyped_fields_take_their_defaults() {
        let runners: GithubRunners = serde_json::from_value(json!({
            "enabled": "yes",
            "installations": [{"installationId": "abc", "maxConcurrent": "lots", "repos": null}],
            "recentJobs": {"not": "a list"},
        }))
        .unwrap();
        assert!(!runners.enabled);
        assert_eq!(runners.installations[0].installation_id, "abc");
        assert_eq!(runners.installations[0].max_concurrent, None);
        assert!(runners.installations[0].repos.is_empty());
        assert!(runners.recent_jobs.is_empty());
        assert!(runners.sizes.is_empty());
        let empty: GithubRunners = serde_json::from_value(json!({})).unwrap();
        assert_eq!(empty, GithubRunners::default());
    }

    #[tokio::test]
    async fn a_gateway_without_runners_reads_as_unavailable() {
        let (gateway, _) = fake(vec![]).await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let runners = auth.github_runners().await.unwrap().unwrap();
        assert!(!runners.available);
        assert_eq!(runners, GithubRunners::default());
    }

    #[tokio::test]
    async fn other_failures_say_so_and_an_insecure_install_link_is_dropped() {
        let (gateway, _) = fake(vec![(
            "GET /v1/github/runners ",
            500,
            r#"{"error":{"message":"boom"}}"#,
        )])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        assert_eq!(auth.github_runners().await.unwrap_err(), "Boom.");

        let (gateway, _) = fake(vec![(
            "GET /v1/github/runners ",
            200,
            r#"{"enabled":true,"installUrl":"http://example.invalid/install"}"#,
        )])
        .await;
        let auth = signed_in(dir.path(), gateway);
        let runners = auth.github_runners().await.unwrap().unwrap();
        assert_eq!(runners.install_url, None);
    }

    #[tokio::test]
    async fn signed_out_lists_nothing_and_never_calls_the_gateway() {
        let (gateway, seen) = fake(vec![]).await;
        let dir = tempfile::tempdir().unwrap();
        let auth = std::sync::Arc::new(CodegraffAuth::new(
            dir.path(),
            Some(dir.path().join("none")),
            gateway,
        ));
        assert_eq!(auth.github_runners().await.unwrap(), None);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn actions_patch_and_delete_exactly_what_changed() {
        let (gateway, seen) = fake(vec![
            ("PATCH /v1/github/runners/installations/4242 ", 200, "{}"),
            (
                "PATCH /v1/github/runners/installations/4242/repos ",
                200,
                "{}",
            ),
            ("DELETE /v1/github/runners/installations/4242 ", 200, "{}"),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        for params in [
            json!({"action": "installation", "installationId": 4242,
                   "monthlyBudgetMicroUsd": 25000000, "maxConcurrent": 4}),
            json!({"action": "repo", "installationId": "4242", "fullName": "octo-org/app",
                   "enabled": false}),
            json!({"action": "repo", "installationId": "4242", "fullName": "octo-org/app",
                   "defaultSize": "large"}),
            json!({"action": "disconnect", "installationId": 4242}),
        ] {
            let action: RunnerAction = serde_json::from_value(params).unwrap();
            auth.github_runner_action(action).await.unwrap();
        }
        assert_eq!(
            recorded(&seen),
            [
                (
                    "PATCH /v1/github/runners/installations/4242".to_string(),
                    Some(json!({"monthlyBudgetMicroUsd": 25000000, "maxConcurrent": 4}))
                ),
                (
                    "PATCH /v1/github/runners/installations/4242/repos".to_string(),
                    Some(json!({"fullName": "octo-org/app", "enabled": false}))
                ),
                (
                    "PATCH /v1/github/runners/installations/4242/repos".to_string(),
                    Some(json!({"fullName": "octo-org/app", "defaultSize": "large"}))
                ),
                (
                    "DELETE /v1/github/runners/installations/4242".to_string(),
                    None
                ),
            ]
        );
    }

    #[tokio::test]
    async fn an_action_on_a_gateway_without_runners_says_it_is_unavailable() {
        let (gateway, _) = fake(vec![]).await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let error = auth
            .github_runner_action(RunnerAction::Disconnect {
                installation_id: "1".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error, "GitHub CI isn't available on Codegraff yet.");
    }

    #[tokio::test]
    async fn a_404_that_names_what_is_missing_is_shown() {
        let (gateway, _) = fake(vec![(
            "DELETE /v1/github/runners/installations/1 ",
            404,
            r#"{"error":{"type":"installation_not_found","message":"installation not found"}}"#,
        )])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let error = auth
            .github_runner_action(RunnerAction::Disconnect {
                installation_id: "1".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error, "Installation not found.");
        assert!(generic_message(Some(&json!({"error": "Not Found"}))));
        assert!(generic_message(Some(
            &json!({"error": {"message": "not found."}})
        )));
        assert!(generic_message(None));
    }

    #[test]
    fn bad_actions_are_refused_before_anything_is_sent() {
        let installation = |budget: Option<i64>, max: Option<u32>| RunnerAction::Installation {
            installation_id: "1".into(),
            monthly_budget_micro_usd: budget,
            untrusted_budget_micro_usd: None,
            max_concurrent: max,
        };
        assert!(request_for(&installation(Some(-1), None)).is_err());
        assert!(request_for(&installation(None, Some(0))).is_err());
        assert!(request_for(&installation(None, Some(9))).is_err());
        assert!(
            request_for(&installation(None, None)).is_err(),
            "nothing to change"
        );
        assert!(request_for(&installation(Some(0), Some(8))).is_ok());
        let repo = RunnerAction::Repo {
            installation_id: "1".into(),
            full_name: " ".into(),
            enabled: Some(true),
            default_size: None,
        };
        assert!(request_for(&repo).is_err());
        let blank = RunnerAction::Disconnect {
            installation_id: "".into(),
        };
        assert!(request_for(&blank).is_err());
        // Ids stay one path segment.
        let odd = RunnerAction::Disconnect {
            installation_id: "../x".into(),
        };
        assert_eq!(
            request_for(&odd).unwrap().1,
            "/v1/github/runners/installations/..%2Fx"
        );
    }
}

//! Codegraff cloud sandboxes (the gateway's `/v1/sandboxes`) for Settings → Accounts.
//!
//! A Harness cloud sandbox is a persistent sandbox on the gateway's own `fleet` provider. After
//! it exists, `POST /v1/sandboxes/:id/harness {deviceName}` does the rest: it attaches a fresh
//! lease-bound sign-in, installs the pinned Linux build once, starts `harness headless`, and
//! waits for it to sign in, after which the sandbox shows up in the owner's device list under
//! `deviceName`. That call is idempotent and is also the second half of every wake (pausing
//! retires the token), so start = `/start` (if not running) then `/harness`.
//!
//! Fleet rows carry no name and no link to their device except the device's name, so each
//! sandbox gets a unique `deviceName`, remembered in `cloud-sandboxes.json` in the data
//! directory and joined onto the list.
//!
//! Sandboxes created or woken here keep running: the gateway renews their window
//! (`autoStopMinutes` long) on its own, billed per second from the account's credit, until the
//! user turns them off (`/stop`, a pause) or a renewal fails (`renewError`), after which the
//! sandbox pauses at the end of its current window with its state kept.
//!
//! Every build offers them to an account signed in with Codegraff; the gateway decides what the
//! account may do (credit, reservations) and its errors are shown as they come.
//! [`cloud_sandboxes_enabled`] can still turn the feature off.
//!
//! Sizes: `GET /v1/sandboxes/personal-sizes` lists what a personal sandbox can be (vCPU, memory,
//! disk, hourly price). The list carries them so every view can offer a size and show a rate, and
//! a create asks for the chosen size's `cpu`/`memory`/`disk`. Gateways without sizes answer 404:
//! the list then has none and a create sends the sizeless body it always did.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{CodegraffAuth, http, urlencode};

const FLEET: &str = "fleet";
const SANDBOX_NAME: &str = "Harness cloud";
/// The length of each running window. A keep-running sandbox has its window renewed before it
/// ends; any other sandbox pauses when it ends.
const AUTO_STOP_MINUTES: u32 = 30;
/// The window a wake asks for (the same length as a fresh sandbox's).
const WAKE_LEASE_SECONDS: u32 = AUTO_STOP_MINUTES * 60;
const QUICK: Duration = Duration::from_secs(20);
/// Creating or waking a sandbox boots a machine.
const BOOT: Duration = Duration::from_secs(90);
/// `/harness` waits up to ~30 s for the device to sign in, plus a first-time install.
const HARNESS_STEP: Duration = Duration::from_secs(120);
const DEVICES_FILE: &str = "cloud-sandboxes.json";

/// One sandbox on the account. Tolerant: unknown fields are ignored and missing ones default.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CodegraffSandbox {
    pub id: String,
    /// `fleet` (ours) or `daytona`.
    pub provider: String,
    pub name: Option<String>,
    /// Normalised: `running` | `starting` | `paused` | `stopping` | `error` | `unknown`
    /// (the fleet says `started`; `destroyed` rows are dropped).
    pub state: String,
    pub tier: Option<String>,
    pub expires_at: Option<Value>,
    /// Harness was set up in it, so it can appear as a device.
    pub harness: bool,
    /// The name its device registers under (remembered at create time).
    pub device_name: Option<String>,
    /// The gateway renews its window until it is turned off. `None` from gateways that don't
    /// know the setting (treated as off).
    pub keep_running: Option<bool>,
    /// Why the last renewal failed (`insufficient_credit`, `key_budget`, `fleet_unavailable`,
    /// or another word); the sandbox pauses when its current window ends.
    pub renew_error: Option<String>,
    /// Its machine, when the gateway says (older gateways don't).
    pub cpu: Option<f64>,
    #[serde(rename = "memoryGiB")]
    pub memory_gib: Option<f64>,
    #[serde(rename = "diskGiB")]
    pub disk_gib: Option<f64>,
    /// What it costs per hour while running, in micro-USD. From the view, else from the
    /// personal size with the same vCPU and memory.
    pub price_per_hour_micro_usd: Option<i64>,
}

/// One size a personal sandbox can be (`GET /v1/sandboxes/personal-sizes`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonalSize {
    /// `standard`, `large`, …
    pub id: String,
    pub cpu: u32,
    #[serde(rename = "memoryGiB")]
    pub memory_gib: u32,
    #[serde(rename = "diskGiB")]
    pub disk_gib: u32,
    pub price_per_hour_micro_usd: Option<i64>,
    /// The size a create uses unless another is picked.
    pub default: bool,
}

/// `enabled: false` means this build doesn't offer cloud sandboxes (the UI hides the card).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CodegraffSandboxes {
    pub enabled: bool,
    pub sandboxes: Vec<CodegraffSandbox>,
    /// The sizes a new sandbox can be; empty when the gateway doesn't offer sizes.
    pub sizes: Vec<PersonalSize>,
}

/// On unless `HARNESS_CLOUD_SANDBOXES=0` (or `false`) turns cloud sandboxes off.
pub fn cloud_sandboxes_enabled() -> bool {
    enabled_for(std::env::var("HARNESS_CLOUD_SANDBOXES").ok().as_deref())
}

fn enabled_for(flag: Option<&str>) -> bool {
    !matches!(flag.map(str::trim), Some("0" | "false"))
}

fn parse_sandbox(value: &Value) -> CodegraffSandbox {
    let text = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    CodegraffSandbox {
        id: text("/id").unwrap_or_default(),
        provider: text("/provider").unwrap_or_default(),
        name: text("/name"),
        state: normalise_state(text("/state").as_deref()),
        // Daytona has no tier in its list response; retain the temporary intent via its labels.
        tier: text("/tier").or_else(|| {
            (value
                .pointer("/labels/harness-purpose")
                .and_then(Value::as_str)
                == Some("temporary"))
            .then(|| "ephemeral".to_string())
        }),
        expires_at: value
            .get("expiresAt")
            .or_else(|| value.get("expires_at"))
            .filter(|v| !v.is_null())
            .cloned(),
        harness: value
            .pointer("/codegraff/harness")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        device_name: None,
        keep_running: value.get("keepRunning").and_then(Value::as_bool),
        renew_error: text("/renewError").filter(|e| !e.trim().is_empty()),
        cpu: number(value, "cpu").filter(|n| *n > 0.0),
        memory_gib: number(value, "memoryGiB")
            .or_else(|| number(value, "memory"))
            .filter(|n| *n > 0.0),
        disk_gib: number(value, "diskGiB")
            .or_else(|| number(value, "disk"))
            .filter(|n| *n > 0.0),
        price_per_hour_micro_usd: number(value, "pricePerHourMicroUsd")
            .filter(|n| *n >= 0.0)
            .map(|n| n.round() as i64),
    }
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
}

/// The gateway's `{sizes: [...]}`. Rows without an id or a machine are dropped; when none is
/// marked default, the first is.
fn parse_sizes(value: &Value) -> Vec<PersonalSize> {
    let whole = |row: &Value, key: &str| {
        number(row, key)
            .filter(|n| *n > 0.0 && *n < f64::from(u32::MAX))
            .map(|n| n.round() as u32)
    };
    let mut sizes: Vec<PersonalSize> = value
        .get("sizes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let id = row.get("id")?.as_str()?.trim();
            if id.is_empty() {
                return None;
            }
            Some(PersonalSize {
                id: id.to_owned(),
                cpu: whole(row, "cpu")?,
                memory_gib: whole(row, "memoryGiB")?,
                disk_gib: whole(row, "diskGiB").unwrap_or(0),
                price_per_hour_micro_usd: number(row, "pricePerHourMicroUsd")
                    .filter(|n| *n >= 0.0)
                    .map(|n| n.round() as i64),
                default: row.get("default").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect();
    if !sizes.is_empty() && !sizes.iter().any(|s| s.default) {
        sizes[0].default = true;
    }
    sizes
}

/// The size a create asks for: the one named, else the default.
fn chosen_size<'a>(sizes: &'a [PersonalSize], wanted: Option<&str>) -> Option<&'a PersonalSize> {
    wanted
        .and_then(|id| sizes.iter().find(|s| s.id == id))
        .or_else(|| sizes.iter().find(|s| s.default))
}

/// A view without a price borrows the price of the size with the same vCPU and memory.
fn fill_price(sandbox: &mut CodegraffSandbox, sizes: &[PersonalSize]) {
    if sandbox.price_per_hour_micro_usd.is_some()
        || sandbox.provider == "daytona"
        || sandbox.tier.as_deref() == Some("ephemeral")
    {
        return;
    }
    let (Some(cpu), Some(memory)) = (sandbox.cpu, sandbox.memory_gib) else {
        return;
    };
    sandbox.price_per_hour_micro_usd = sizes
        .iter()
        .find(|s| f64::from(s.cpu) == cpu && f64::from(s.memory_gib) == memory)
        .and_then(|s| s.price_per_hour_micro_usd);
}

/// The create body: today's, plus the chosen size's machine when the gateway offers sizes.
fn create_body(size: Option<&PersonalSize>) -> Value {
    let mut body = json!({
        "provider": FLEET,
        "persistent": true,
        "name": SANDBOX_NAME,
        "autoStopMinutes": AUTO_STOP_MINUTES,
        "keepRunning": true,
    });
    if let Some(size) = size {
        body["cpu"] = json!(size.cpu);
        body["memory"] = json!(size.memory_gib);
        if size.disk_gib > 0 {
            body["disk"] = json!(size.disk_gib);
        }
    }
    body
}

/// The fleet's own words (`started`, `destroyed`) in the words the rest of the app uses.
fn normalise_state(state: Option<&str>) -> String {
    match state {
        Some("started") => "running".into(),
        Some("destroyed") => "terminated".into(),
        Some(other) => other.into(),
        None => String::new(),
    }
}

/// A device name the gateway accepts (1-40 of `[A-Za-z0-9 ._-]`, first alphanumeric) that is
/// unique per sandbox, so the device list can tell two cloud sandboxes apart.
fn device_name_for(sandbox_id: &str) -> String {
    let short: String = sandbox_id
        .strip_prefix("cnd_")
        .unwrap_or(sandbox_id)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(4)
        .collect();
    if short.is_empty() {
        SANDBOX_NAME.to_string()
    } else {
        format!("{SANDBOX_NAME} {short}")
    }
}

/// The list may be a bare array or wrapped; terminated sandboxes are gone for good.
fn parse_list(value: &Value) -> Vec<CodegraffSandbox> {
    let items = match value {
        Value::Array(items) => Some(items),
        other => ["sandboxes", "data", "items"]
            .iter()
            .find_map(|key| other.get(key).and_then(Value::as_array)),
    };
    items
        .into_iter()
        .flatten()
        .map(parse_sandbox)
        .filter(|sandbox| !sandbox.id.is_empty() && sandbox.state != "terminated")
        .collect()
}

/// A gateway call that did not succeed.
pub(super) struct Failure {
    /// 0 when nothing was sent (not signed in, bad gateway URL, no network).
    pub(super) status: u16,
    pub(super) body: Option<Value>,
    pub(super) message: Option<String>,
}

impl Failure {
    fn local(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            body: None,
            message: Some(message.into()),
        }
    }

    pub(super) fn error_type(&self) -> Option<&str> {
        self.body.as_ref()?.pointer("/error/type")?.as_str()
    }

    /// The gateway's own `error.message` as a sentence, else `fallback`. A few error types
    /// get app wording because the gateway's text is written for developers.
    pub(super) fn sentence(&self, fallback: &str) -> String {
        let known = match self.error_type() {
            Some("insufficient_credits") => {
                Some("Your Codegraff credit is used up. Add credit and try again.")
            }
            Some("sandbox_not_running") => Some("The sandbox isn't running. Start it first."),
            Some("insufficient_scope") => {
                Some("This Codegraff sign-in can't manage sandboxes. Sign in again.")
            }
            Some("harness_cloud_unavailable") => {
                Some("Harness cloud isn't available yet. Try again later.")
            }
            _ => None,
        };
        if let Some(known) = known {
            return known.to_string();
        }
        let message = self
            .message
            .as_deref()
            .or_else(|| self.body.as_ref()?.pointer("/error/message")?.as_str())
            .map(str::trim)
            .filter(|m| !m.is_empty());
        match message {
            Some(message) => {
                let mut chars = message.chars();
                let first = chars.next().map(|c| c.to_uppercase().to_string());
                format!(
                    "{}{}.",
                    first.unwrap_or_default(),
                    chars.as_str().trim_end_matches('.')
                )
            }
            None if self.status == 0 => format!("{fallback}."),
            None => format!("{fallback} ({}).", self.status),
        }
    }
}

impl CodegraffAuth {
    pub(super) async fn sandbox_call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, Failure> {
        let key = self
            .current_key()
            .ok_or_else(|| Failure::local("Sign in with Codegraff first"))?;
        crate::auth::validate_secure_url("CodeGraff gateway", &self.gateway)
            .map_err(Failure::local)?;
        let mut request = http()
            .request(method, format!("{}{path}", self.gateway))
            .bearer_auth(key)
            .timeout(timeout);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| Failure::local(format!("couldn't reach CodeGraff: {e}")))?;
        let status = response.status();
        let body = response.json::<Value>().await.ok();
        if status.is_success() {
            Ok(body.unwrap_or(Value::Null))
        } else {
            Err(Failure {
                status: status.as_u16(),
                body,
                message: None,
            })
        }
    }

    fn sandbox_path(id: &str, tail: &str) -> String {
        format!("/v1/sandboxes/{}{tail}", urlencode(id))
    }

    fn devices_path(&self) -> Option<PathBuf> {
        self.identity.parent().map(|dir| dir.join(DEVICES_FILE))
    }

    /// `sandbox id → device name`, as remembered at create time.
    fn device_names(&self) -> BTreeMap<String, String> {
        self.devices_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save_device_names(&self, names: &BTreeMap<String, String>) {
        let Some(path) = self.devices_path() else {
            return;
        };
        let staged = path.with_extension("json.tmp");
        let written = serde_json::to_vec_pretty(names)
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(&staged, bytes))
            .and_then(|()| std::fs::rename(&staged, &path));
        if let Err(error) = written {
            tracing::warn!(%error, "could not save the cloud sandbox device names");
        }
    }

    /// The account's sandboxes (`None` when signed out). Builds that don't offer cloud
    /// sandboxes answer `enabled: false` without calling the gateway.
    pub async fn sandboxes(&self) -> Result<Option<CodegraffSandboxes>, String> {
        if self.current_key().is_none() {
            return Ok(None);
        }
        if !cloud_sandboxes_enabled() {
            return Ok(Some(CodegraffSandboxes::default()));
        }
        let (value, sizes) = tokio::join!(
            self.sandbox_call(Method::GET, "/v1/sandboxes", None, QUICK),
            self.personal_sizes()
        );
        let value = value.map_err(|f| f.sentence("CodeGraff sandboxes are unavailable"))?;
        let names = self.device_names();
        let mut sandboxes = parse_list(&value);
        for sandbox in &mut sandboxes {
            sandbox.device_name = names.get(&sandbox.id).cloned();
            sandbox.harness = sandbox.harness || sandbox.device_name.is_some();
            fill_price(sandbox, &sizes);
        }
        Ok(Some(CodegraffSandboxes {
            enabled: true,
            sandboxes,
            sizes,
        }))
    }

    /// The sizes a personal sandbox can be. Empty when the gateway doesn't offer sizes (404) or
    /// can't say right now: callers then behave as before sizes existed.
    async fn personal_sizes(&self) -> Vec<PersonalSize> {
        match self
            .sandbox_call(Method::GET, "/v1/sandboxes/personal-sizes", None, QUICK)
            .await
        {
            Ok(value) => parse_sizes(&value),
            Err(failure) => {
                if failure.status != 404 {
                    tracing::debug!(status = failure.status, "cloud sandbox sizes unavailable");
                }
                Vec::new()
            }
        }
    }

    /// Set Harness up in a running sandbox (idempotent; see the module docs). One retry,
    /// because the gateway says a failed install or start is safe to retry once.
    async fn attach_harness(&self, id: &str, device_name: &str) -> Result<(), Failure> {
        let path = Self::sandbox_path(id, "/harness");
        let body = json!({ "deviceName": device_name });
        match self
            .sandbox_call(Method::POST, &path, Some(body.clone()), HARNESS_STEP)
            .await
        {
            Err(f) if f.error_type() == Some("harness_start_failed") => self
                .sandbox_call(Method::POST, &path, Some(body), HARNESS_STEP)
                .await
                .map(drop),
            other => other.map(drop),
        }
    }

    /// A short-lived API/tool sandbox. Let the gateway choose the fleet or Daytona fallback;
    /// unlike a personal cloud machine, this must not attach Harness or renew its lease.
    pub async fn create_temporary_sandbox(&self) -> Result<CodegraffSandbox, String> {
        if !cloud_sandboxes_enabled() {
            return Err("Cloud sandboxes are not available in this build.".into());
        }
        let body = json!({
            "provider": "auto",
            "ephemeral": true,
            "name": "Temporary sandbox",
            "autoStopMinutes": AUTO_STOP_MINUTES,
            "labels": { "harness-purpose": "temporary" },
        });
        let created = self
            .sandbox_call(Method::POST, "/v1/sandboxes", Some(body), BOOT)
            .await
            .map_err(|f| f.sentence("CodeGraff couldn't create the temporary sandbox"))?;
        let sandbox = parse_sandbox(&created);
        if sandbox.id.is_empty() {
            return Err("CodeGraff created a sandbox but didn't say which one.".into());
        }
        Ok(sandbox)
    }

    /// Create the account's Harness cloud sandbox, `size` (a personal size id) or the default
    /// one, and set Harness up in it. If the machine exists but Harness could not start, it stays
    /// in the list: Start retries the setup.
    pub async fn create_sandbox(&self, size: Option<&str>) -> Result<CodegraffSandbox, String> {
        if !cloud_sandboxes_enabled() {
            return Err("Cloud sandboxes are not available in this build.".into());
        }
        let sizes = self.personal_sizes().await;
        let chosen = chosen_size(&sizes, size);
        let body = create_body(chosen);
        let created = self
            .sandbox_call(Method::POST, "/v1/sandboxes", Some(body), BOOT)
            .await
            .map_err(|f| f.sentence("CodeGraff couldn't create the sandbox"))?;
        let mut sandbox = parse_sandbox(&created);
        if sandbox.id.is_empty() {
            return Err("CodeGraff created a sandbox but didn't say which one.".into());
        }
        if let Some(chosen) = chosen {
            sandbox.cpu.get_or_insert(f64::from(chosen.cpu));
            sandbox
                .memory_gib
                .get_or_insert(f64::from(chosen.memory_gib));
            if chosen.disk_gib > 0 {
                sandbox.disk_gib.get_or_insert(f64::from(chosen.disk_gib));
            }
            if sandbox.price_per_hour_micro_usd.is_none() {
                sandbox.price_per_hour_micro_usd = chosen.price_per_hour_micro_usd;
            }
        }
        // Remembered before the setup call, so a failed setup retries under the same name.
        let device_name = device_name_for(&sandbox.id);
        let mut names = self.device_names();
        names.insert(sandbox.id.clone(), device_name.clone());
        self.save_device_names(&names);
        if let Err(failure) = self.attach_harness(&sandbox.id, &device_name).await {
            return Err(format!(
                "The sandbox was created, but Harness couldn't be set up in it ({}) Use Start on it to try again.",
                failure.sentence("setup failed")
            ));
        }
        sandbox.state = "running".into();
        sandbox.harness = true;
        sandbox.device_name = Some(device_name);
        Ok(sandbox)
    }

    /// Start a sandbox that isn't running (or set Harness up again in a running one): `/start`
    /// when needed, kept running like a fresh one, then `/harness` under the name remembered
    /// at create time.
    pub async fn start_sandbox(&self, id: &str) -> Result<(), String> {
        let current = self
            .sandbox_call(Method::GET, &Self::sandbox_path(id, ""), None, QUICK)
            .await
            .map_err(|f| f.sentence("CodeGraff couldn't read the sandbox"))?;
        if parse_sandbox(&current).state != "running" {
            let body = json!({ "leaseSeconds": WAKE_LEASE_SECONDS, "keepRunning": true });
            self.sandbox_call(
                Method::POST,
                &Self::sandbox_path(id, "/start"),
                Some(body),
                BOOT,
            )
            .await
            .map_err(|f| f.sentence("CodeGraff couldn't start the sandbox"))?;
        }
        let mut names = self.device_names();
        let device_name = names
            .get(id)
            .cloned()
            .unwrap_or_else(|| device_name_for(id));
        self.attach_harness(id, &device_name).await.map_err(|f| {
            f.sentence(
                "The sandbox is running but Harness couldn't be set up in it; try Start again",
            )
        })?;
        if names.insert(id.to_owned(), device_name.clone()).is_none() {
            self.save_device_names(&names);
        }
        Ok(())
    }

    /// Pause (fleet: memory and processes are kept). Billing for the machine stops.
    pub async fn stop_sandbox(&self, id: &str) -> Result<(), String> {
        self.sandbox_call(Method::POST, &Self::sandbox_path(id, "/stop"), None, QUICK)
            .await
            .map(drop)
            .map_err(|f| f.sentence("CodeGraff couldn't stop the sandbox"))
    }

    /// Turn window renewal on or off for a running sandbox. Off, it pauses when its current
    /// window ends. Answers the sandbox as the gateway now sees it.
    pub async fn set_sandbox_keep_running(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<CodegraffSandbox, String> {
        let value = self
            .sandbox_call(
                Method::POST,
                &Self::sandbox_path(id, "/keep-running"),
                Some(json!({ "enabled": enabled })),
                QUICK,
            )
            .await
            .map_err(|f| {
                f.sentence("CodeGraff couldn't change whether the sandbox keeps running")
            })?;
        // The gateway answers the bare view (like `/start` and `/stop`); a `{sandbox}`
        // wrapper is read too.
        let view = match value.get("sandbox") {
            Some(inner) if inner.is_object() && value.get("id").is_none() => inner,
            _ => &value,
        };
        let mut sandbox = parse_sandbox(view);
        if sandbox.id.is_empty() {
            sandbox.id = id.to_owned();
        }
        sandbox.device_name = self.device_names().get(id).cloned();
        sandbox.harness = sandbox.harness || sandbox.device_name.is_some();
        Ok(sandbox)
    }

    pub async fn delete_sandbox(&self, id: &str) -> Result<(), String> {
        self.sandbox_call(Method::DELETE, &Self::sandbox_path(id, ""), None, QUICK)
            .await
            .map_err(|f| f.sentence("CodeGraff couldn't delete the sandbox"))?;
        let mut names = self.device_names();
        if names.remove(id).is_some() {
            self.save_device_names(&names);
        }
        Ok(())
    }

    /// Usage for one sandbox, as the gateway sends it.
    pub async fn sandbox_meter(&self, id: &str) -> Result<Value, String> {
        self.sandbox_call(Method::GET, &Self::sandbox_path(id, "/meter"), None, QUICK)
            .await
            .map_err(|f| f.sentence("CodeGraff couldn't read the sandbox's usage"))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::super::{KEY_FILE, write_private};
    use super::*;

    #[test]
    fn every_build_offers_sandboxes_unless_the_flag_turns_them_off() {
        assert!(enabled_for(None), "on in every build");
        assert!(enabled_for(Some("1")));
        assert!(enabled_for(Some("")));
        assert!(!enabled_for(Some("0")));
        assert!(!enabled_for(Some(" false ")));
    }

    #[test]
    fn lists_use_the_apps_state_words_and_drop_destroyed_rows() {
        let wrapped = json!({"sandboxes": [
            {"id": "cnd_1", "provider": "fleet", "state": "started", "expiresAt": 1790000000,
             "future": 1},
            {"id": "cnd_2", "provider": "fleet", "state": "destroyed"},
            {"id": "sb_3", "provider": "daytona", "state": "terminated"},
            {"id": "cnd_4", "state": "paused"},
            {"id": "cnd_5", "state": "starting"},
        ]});
        let parsed = parse_list(&wrapped);
        let states: Vec<_> = parsed
            .iter()
            .map(|s| (s.id.as_str(), s.state.as_str()))
            .collect();
        assert_eq!(
            states,
            [
                ("cnd_1", "running"),
                ("cnd_4", "paused"),
                ("cnd_5", "starting")
            ]
        );
        assert!(parsed[0].expires_at.is_some());
        assert_eq!(
            parse_list(&json!([{"id": "a", "state": "started"}])).len(),
            1
        );
        assert!(parse_list(&json!({"error": "nope"})).is_empty());
    }

    #[test]
    fn device_names_are_valid_and_unique_per_sandbox() {
        let a = device_name_for("cnd_ab12cd34ef56");
        let b = device_name_for("cnd_ff00cd34ef56");
        assert_eq!(a, "Harness cloud ab12");
        assert_ne!(a, b);
        for name in [&a, &b, &device_name_for("odd id!!")] {
            assert!((1..=40).contains(&name.len()));
            assert!(name.starts_with(|c: char| c.is_ascii_alphanumeric()));
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || " ._-".contains(c))
            );
        }
        assert_eq!(device_name_for(""), "Harness cloud");
    }

    #[test]
    fn failures_speak_in_app_words_or_the_gateways() {
        let credits = Failure {
            status: 402,
            body: Some(json!({"error": {"type": "insufficient_credits", "message": "x"}})),
            message: None,
        };
        assert!(
            credits
                .sentence("x")
                .starts_with("Your Codegraff credit is used up")
        );
        let limit = Failure {
            status: 429,
            body: Some(
                json!({"error": {"type": "fleet_limit", "message": "the fleet sandbox limit is reached"}}),
            ),
            message: None,
        };
        assert_eq!(limit.sentence("x"), "The fleet sandbox limit is reached.");
        let bare = Failure {
            status: 500,
            body: None,
            message: None,
        };
        assert_eq!(
            bare.sentence("CodeGraff couldn't stop the sandbox"),
            "CodeGraff couldn't stop the sandbox (500)."
        );
    }

    /// A fake gateway. Each request takes the first matching route not yet used; when every
    /// match is used, the last one repeats. Every request is recorded in full.
    pub(in super::super) async fn fake(
        routes: Vec<(&'static str, u16, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let used = Arc::new(Mutex::new(vec![false; routes.len()]));
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 16384];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = request.lines().next().unwrap_or_default().to_owned();
                log.lock().unwrap().push(request);
                let matching: Vec<usize> = (0..routes.len())
                    .filter(|&i| line.starts_with(routes[i].0))
                    .collect();
                let (status, body) = {
                    let mut used = used.lock().unwrap();
                    let pick = matching
                        .iter()
                        .copied()
                        .find(|&i| !used[i])
                        .or_else(|| matching.last().copied());
                    match pick {
                        Some(i) => {
                            used[i] = true;
                            (routes[i].1, routes[i].2)
                        }
                        None => (404, "{}"),
                    }
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (base, seen)
    }

    pub(in super::super) fn signed_in(dir: &Path, gateway: String) -> Arc<CodegraffAuth> {
        let key_file = dir.join("home").join(KEY_FILE);
        write_private(&key_file, br#"{"api_key":"cg_sk_test"}"#).unwrap();
        Arc::new(CodegraffAuth::new(dir, Some(key_file), gateway))
    }

    fn lines(seen: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        seen.lock()
            .unwrap()
            .iter()
            .map(|r| {
                r.lines()
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches(" HTTP/1.1")
                    .to_owned()
            })
            .collect()
    }

    fn enable() {
        // SAFETY: every test that needs the flag sets the same value.
        unsafe { std::env::set_var("HARNESS_CLOUD_SANDBOXES", "1") };
    }

    #[tokio::test]
    async fn temporary_create_allows_fallback_without_sizes_or_harness_setup() {
        for (provider, reply) in [
            (
                "fleet",
                r#"{"id":"temp_1","provider":"fleet","state":"started","tier":"ephemeral","cpu":2,"memoryGiB":4}"#,
            ),
            (
                "daytona",
                r#"{"id":"temp_1","provider":"daytona","state":"started","cpu":2,"memory":4,"disk":10,"labels":{"harness-purpose":"temporary"}}"#,
            ),
        ] {
            let (gateway, seen) = fake(vec![("POST /v1/sandboxes ", 201, reply)]).await;
            let dir = tempfile::tempdir().unwrap();
            let auth = signed_in(dir.path(), gateway);
            enable();
            let sandbox = auth.create_temporary_sandbox().await.unwrap();
            assert_eq!(sandbox.provider, provider);
            assert_eq!(sandbox.tier.as_deref(), Some("ephemeral"));
            assert_eq!(sandbox.memory_gib, Some(4.0));
            assert!(!sandbox.harness);
            assert!(auth.device_names().is_empty());
            assert_eq!(
                recorded(&seen),
                expand(&vector()["plans"]["createTemporary"], "", "")
            );
        }
    }

    #[test]
    fn temporary_lists_keep_the_provider_and_never_borrow_personal_prices() {
        let list = parse_list(&json!([
            {"id":"f","provider":"fleet","state":"started","tier":"ephemeral","cpu":2,"memoryGiB":4},
            {"id":"d","provider":"daytona","state":"started","cpu":2,"memory":4,
             "labels":{"harness-purpose":"temporary"}}
        ]));
        let sizes = parse_sizes(&serde_json::from_str(SIZES).unwrap());
        for mut sandbox in list {
            assert_eq!(sandbox.tier.as_deref(), Some("ephemeral"));
            fill_price(&mut sandbox, &sizes);
            assert_eq!(sandbox.price_per_hour_micro_usd, None);
        }
    }

    #[tokio::test]
    async fn create_makes_a_fleet_sandbox_then_sets_harness_up_under_a_unique_name() {
        let (gateway, seen) = fake(vec![
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_ab12cd","provider":"fleet","state":"started","tier":"persistent","expiresAt":1790000000}"#,
            ),
            (
                "POST /v1/sandboxes/cnd_ab12cd/harness ",
                200,
                r#"{"id":"cnd_ab12cd","harness":{"running":true,"signedIn":true}}"#,
            ),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        let made = auth.create_sandbox(None).await.unwrap();
        assert_eq!(made.state, "running");
        assert_eq!(made.device_name.as_deref(), Some("Harness cloud ab12"));
        assert!(made.harness);
        let mut sent = seen.lock().unwrap().clone();
        assert!(
            sent.remove(0)
                .starts_with("GET /v1/sandboxes/personal-sizes "),
            "the sizes are read first"
        );
        assert!(sent[0].contains("Bearer cg_sk_test"));
        for needle in [
            r#""provider":"fleet""#,
            r#""persistent":true"#,
            r#""autoStopMinutes":30"#,
            r#""keepRunning":true"#,
        ] {
            assert!(sent[0].contains(needle), "{needle} in {}", sent[0]);
        }
        assert!(
            !sent[0].contains(r#""harness""#),
            "the create call must not ask for the old flag"
        );
        assert!(
            !sent[0].contains(r#""cpu""#) && !sent[0].contains(r#""memory""#),
            "a gateway without sizes gets today's body"
        );
        assert!(sent[1].contains(r#""deviceName":"Harness cloud ab12""#));
        assert!(
            lines(&seen).iter().all(|l| !l.contains("/codegraff")),
            "{:?}",
            lines(&seen)
        );
        // The name is remembered, and the list joins it back on.
        assert_eq!(
            auth.device_names().get("cnd_ab12cd").map(String::as_str),
            Some("Harness cloud ab12")
        );
    }

    #[tokio::test]
    async fn a_failed_setup_keeps_the_sandbox_and_says_to_use_start() {
        let (gateway, seen) = fake(vec![
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_aa","provider":"fleet","state":"started"}"#,
            ),
            (
                "POST /v1/sandboxes/cnd_aa/harness ",
                503,
                r#"{"error":{"type":"harness_cloud_unavailable","message":"no pinned build"}}"#,
            ),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        let error = auth.create_sandbox(None).await.unwrap_err();
        assert!(
            error.contains("was created") && error.contains("Use Start"),
            "{error}"
        );
        assert!(
            lines(&seen).iter().all(|l| !l.starts_with("DELETE")),
            "nothing is deleted"
        );
        assert!(
            auth.device_names().contains_key("cnd_aa"),
            "so Start reuses the name"
        );
    }

    #[tokio::test]
    async fn a_failed_install_is_retried_once() {
        let (gateway, seen) = fake(vec![
            (
                "POST /v1/sandboxes/cnd_1/harness ",
                502,
                r#"{"error":{"type":"harness_start_failed","message":"Harness could not be installed/started in the sandbox."}}"#,
            ),
            ("POST /v1/sandboxes/cnd_1/harness ", 200, r#"{"id":"cnd_1"}"#),
            ("GET /v1/sandboxes/cnd_1 ", 200, r#"{"id":"cnd_1","state":"started"}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        auth.start_sandbox("cnd_1").await.unwrap();
        let harness_calls = lines(&seen)
            .iter()
            .filter(|l| l.contains("/harness"))
            .count();
        assert_eq!(harness_calls, 2);
    }

    #[tokio::test]
    async fn waking_starts_a_paused_sandbox_then_sets_harness_up_never_the_old_attach() {
        let (gateway, seen) = fake(vec![
            (
                "GET /v1/sandboxes/cnd_1 ",
                200,
                r#"{"id":"cnd_1","state":"paused"}"#,
            ),
            (
                "POST /v1/sandboxes/cnd_1/start ",
                200,
                r#"{"state":"started"}"#,
            ),
            (
                "POST /v1/sandboxes/cnd_1/harness ",
                200,
                r#"{"id":"cnd_1"}"#,
            ),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let mut names = BTreeMap::new();
        names.insert("cnd_1".to_string(), "Harness cloud 0001".to_string());
        auth.save_device_names(&names);
        auth.start_sandbox("cnd_1").await.unwrap();
        assert_eq!(
            lines(&seen),
            [
                "GET /v1/sandboxes/cnd_1",
                "POST /v1/sandboxes/cnd_1/start",
                "POST /v1/sandboxes/cnd_1/harness"
            ]
        );
        assert!(seen.lock().unwrap()[2].contains(r#""deviceName":"Harness cloud 0001""#));
        let start = &recorded(&seen)[1];
        assert_eq!(
            start.1,
            Some(json!({"leaseSeconds": 1800, "keepRunning": true})),
            "a wake keeps the sandbox running like a fresh one"
        );
    }

    #[test]
    fn views_carry_keep_running_and_renew_errors_and_older_gateways_omit_them() {
        let kept = parse_sandbox(&json!({
            "id": "cnd_1", "state": "started", "keepRunning": true, "renewError": null
        }));
        assert_eq!(kept.keep_running, Some(true));
        assert_eq!(kept.renew_error, None);
        let failing = parse_sandbox(&json!({
            "id": "cnd_1", "state": "started", "keepRunning": false,
            "renewError": "insufficient_credit"
        }));
        assert_eq!(failing.keep_running, Some(false));
        assert_eq!(failing.renew_error.as_deref(), Some("insufficient_credit"));
        let old = parse_sandbox(&json!({"id": "cnd_1", "state": "started"}));
        assert_eq!((old.keep_running, old.renew_error), (None, None));
        let blank = parse_sandbox(&json!({"id": "cnd_1", "renewError": " "}));
        assert_eq!(blank.renew_error, None);
    }

    #[tokio::test]
    async fn keep_running_posts_the_flag_and_answers_the_sandbox() {
        let (gateway, seen) = fake(vec![(
            "POST /v1/sandboxes/cnd_1/keep-running ",
            200,
            r#"{"id":"cnd_1","provider":"fleet","state":"started","keepRunning":false,"renewError":null}"#,
        )])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let mut names = BTreeMap::new();
        names.insert("cnd_1".to_string(), "Harness cloud 0001".to_string());
        auth.save_device_names(&names);
        let sandbox = auth.set_sandbox_keep_running("cnd_1", false).await.unwrap();
        assert_eq!(sandbox.keep_running, Some(false));
        assert_eq!(sandbox.state, "running");
        assert_eq!(sandbox.device_name.as_deref(), Some("Harness cloud 0001"));
        assert_eq!(
            recorded(&seen),
            [(
                "POST /v1/sandboxes/cnd_1/keep-running".to_string(),
                Some(json!({"enabled": false}))
            )]
        );
    }

    #[tokio::test]
    async fn keep_running_also_reads_a_wrapped_view() {
        let (gateway, _) = fake(vec![(
            "POST /v1/sandboxes/cnd_1/keep-running ",
            200,
            r#"{"sandbox":{"id":"cnd_1","state":"started","keepRunning":true}}"#,
        )])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let sandbox = auth.set_sandbox_keep_running("cnd_1", true).await.unwrap();
        assert_eq!(sandbox.keep_running, Some(true));
        assert_eq!(sandbox.state, "running");
    }

    #[tokio::test]
    async fn a_refused_keep_running_says_why() {
        let (gateway, _) = fake(vec![(
            "POST /v1/sandboxes/cnd_1/keep-running ",
            402,
            r#"{"error":{"type":"insufficient_credits","message":"x"}}"#,
        )])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let error = auth
            .set_sandbox_keep_running("cnd_1", true)
            .await
            .unwrap_err();
        assert!(
            error.starts_with("Your Codegraff credit is used up"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_running_sandbox_skips_start() {
        let (gateway, seen) = fake(vec![
            (
                "GET /v1/sandboxes/cnd_1 ",
                200,
                r#"{"id":"cnd_1","state":"started"}"#,
            ),
            (
                "POST /v1/sandboxes/cnd_1/harness ",
                200,
                r#"{"id":"cnd_1"}"#,
            ),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        auth.start_sandbox("cnd_1").await.unwrap();
        assert!(lines(&seen).iter().all(|l| !l.contains("/start")));
    }

    #[tokio::test]
    async fn a_failed_setup_after_start_says_so_instead_of_claiming_success() {
        let (gateway, _) = fake(vec![
            ("GET /v1/sandboxes/cnd_1 ", 200, r#"{"state":"paused"}"#),
            ("POST /v1/sandboxes/cnd_1/start ", 200, r#"{}"#),
            ("POST /v1/sandboxes/cnd_1/harness ", 500, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        let error = auth.start_sandbox("cnd_1").await.unwrap_err();
        assert!(error.contains("couldn't be set up"), "{error}");
    }

    #[tokio::test]
    async fn the_list_joins_the_remembered_device_name_and_delete_forgets_it() {
        let (gateway, _) = fake(vec![
            (
                "GET /v1/sandboxes ",
                200,
                r#"{"sandboxes":[{"id":"cnd_1","provider":"fleet","state":"started"},{"id":"cnd_2","provider":"fleet","state":"paused"}]}"#,
            ),
            ("DELETE /v1/sandboxes/cnd_1 ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        let mut names = BTreeMap::new();
        names.insert("cnd_1".to_string(), "Harness cloud 0001".to_string());
        auth.save_device_names(&names);
        let list = auth.sandboxes().await.unwrap().unwrap();
        assert_eq!(
            list.sandboxes[0].device_name.as_deref(),
            Some("Harness cloud 0001")
        );
        assert!(list.sandboxes[0].harness);
        assert_eq!(list.sandboxes[1].device_name, None);
        assert!(!list.sandboxes[1].harness);
        auth.delete_sandbox("cnd_1").await.unwrap();
        assert!(auth.device_names().is_empty());
    }

    #[tokio::test]
    async fn signed_out_lists_nothing_and_never_calls_the_gateway() {
        let (gateway, seen) = fake(vec![]).await;
        let dir = tempfile::tempdir().unwrap();
        let auth = Arc::new(CodegraffAuth::new(
            dir.path(),
            Some(dir.path().join("none")),
            gateway,
        ));
        enable();
        assert_eq!(auth.sandboxes().await.unwrap(), None);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stop_delete_and_ids_stay_one_path_segment() {
        let (gateway, seen) = fake(vec![
            ("POST /v1/sandboxes/cnd_1/stop ", 200, r#"{}"#),
            ("DELETE /v1/sandboxes/..%2Fx ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        auth.stop_sandbox("cnd_1").await.unwrap();
        auth.delete_sandbox("../x").await.unwrap();
        assert!(
            lines(&seen)
                .iter()
                .any(|l| l == "DELETE /v1/sandboxes/..%2Fx")
        );
    }

    const SIZES: &str = r#"{"sizes":[
        {"id":"standard","cpu":2,"memoryGiB":4,"diskGiB":40,"pricePerHourMicroUsd":100000,"default":true},
        {"id":"large","cpu":4,"memoryGiB":8,"diskGiB":40,"pricePerHourMicroUsd":200000}
    ]}"#;

    #[test]
    fn sizes_parse_tolerantly_and_always_have_a_default() {
        let sizes = parse_sizes(&serde_json::from_str(SIZES).unwrap());
        assert_eq!(sizes.len(), 2);
        assert_eq!(
            sizes[0],
            PersonalSize {
                id: "standard".into(),
                cpu: 2,
                memory_gib: 4,
                disk_gib: 40,
                price_per_hour_micro_usd: Some(100_000),
                default: true,
            }
        );
        assert!(!sizes[1].default);
        let loose = parse_sizes(&json!({"sizes": [
            {"id": "", "cpu": 2, "memoryGiB": 4},
            {"id": "tiny", "cpu": "two", "memoryGiB": 1},
            {"id": "big", "cpu": 8.0, "memoryGiB": 16, "pricePerHourMicroUsd": 400000.4},
            {"id": "huge", "cpu": 16, "memoryGiB": 32}
        ]}));
        let ids: Vec<_> = loose.iter().map(|s| (s.id.as_str(), s.default)).collect();
        assert_eq!(
            ids,
            [("big", true), ("huge", false)],
            "first is the default"
        );
        assert_eq!(loose[0].price_per_hour_micro_usd, Some(400_000));
        assert_eq!(loose[1].price_per_hour_micro_usd, None);
        assert!(parse_sizes(&json!({"error": "nope"})).is_empty());
        assert!(parse_sizes(&json!([])).is_empty());
    }

    #[test]
    fn the_create_body_adds_the_chosen_machine_and_keeps_todays_fields() {
        let sizes = parse_sizes(&serde_json::from_str(SIZES).unwrap());
        assert_eq!(chosen_size(&sizes, None).unwrap().id, "standard");
        assert_eq!(chosen_size(&sizes, Some("large")).unwrap().id, "large");
        assert_eq!(
            chosen_size(&sizes, Some("gone")).unwrap().id,
            "standard",
            "an unknown size falls back to the default"
        );
        assert_eq!(chosen_size(&[], Some("large")), None);
        assert_eq!(
            create_body(chosen_size(&sizes, Some("large"))),
            json!({
                "provider": "fleet", "persistent": true, "name": "Harness cloud",
                "autoStopMinutes": 30, "keepRunning": true,
                "cpu": 4, "memory": 8, "disk": 40
            })
        );
        assert_eq!(
            create_body(None),
            json!({
                "provider": "fleet", "persistent": true, "name": "Harness cloud",
                "autoStopMinutes": 30, "keepRunning": true
            })
        );
    }

    #[test]
    fn views_carry_the_machine_and_price_and_older_gateways_omit_them() {
        let sized = parse_sandbox(&json!({
            "id": "cnd_1", "state": "started", "cpu": 2, "memoryGiB": 4, "diskGiB": 40,
            "pricePerHourMicroUsd": 100000
        }));
        assert_eq!(sized.cpu, Some(2.0));
        assert_eq!(sized.memory_gib, Some(4.0));
        assert_eq!(sized.disk_gib, Some(40.0));
        assert_eq!(sized.price_per_hour_micro_usd, Some(100_000));
        let old = parse_sandbox(&json!({"id": "cnd_1", "state": "started"}));
        assert_eq!(
            (
                old.cpu,
                old.memory_gib,
                old.disk_gib,
                old.price_per_hour_micro_usd
            ),
            (None, None, None, None)
        );
        let odd = parse_sandbox(&json!({"id": "cnd_1", "cpu": "2", "pricePerHourMicroUsd": -1}));
        assert_eq!((odd.cpu, odd.price_per_hour_micro_usd), (None, None));

        // A view without a price borrows its size's.
        let sizes = parse_sizes(&serde_json::from_str(SIZES).unwrap());
        let mut unpriced = parse_sandbox(&json!({"id": "x", "cpu": 4, "memoryGiB": 8}));
        fill_price(&mut unpriced, &sizes);
        assert_eq!(unpriced.price_per_hour_micro_usd, Some(200_000));
        let mut odd_size = parse_sandbox(&json!({"id": "x", "cpu": 3, "memoryGiB": 8}));
        fill_price(&mut odd_size, &sizes);
        assert_eq!(odd_size.price_per_hour_micro_usd, None);
    }

    #[tokio::test]
    async fn create_with_a_size_sends_its_machine() {
        let (gateway, seen) = fake(vec![
            ("GET /v1/sandboxes/personal-sizes ", 200, SIZES),
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_ab12cd","provider":"fleet","state":"started"}"#,
            ),
            ("POST /v1/sandboxes/cnd_ab12cd/harness ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        let made = auth.create_sandbox(Some("large")).await.unwrap();
        assert_eq!(made.cpu, Some(4.0));
        assert_eq!(made.memory_gib, Some(8.0));
        assert_eq!(
            made.price_per_hour_micro_usd,
            Some(200_000),
            "the size's price until the list says otherwise"
        );
        let calls = recorded(&seen);
        assert_eq!(calls[0].0, "GET /v1/sandboxes/personal-sizes");
        assert_eq!(calls[1].0, "POST /v1/sandboxes");
        let body = calls[1].1.as_ref().unwrap();
        assert_eq!(
            (&body["cpu"], &body["memory"], &body["disk"]),
            (&json!(4), &json!(8), &json!(40))
        );
        assert_eq!(body["keepRunning"], json!(true));
    }

    #[tokio::test]
    async fn create_without_a_size_takes_the_default() {
        let (gateway, seen) = fake(vec![
            ("GET /v1/sandboxes/personal-sizes ", 200, SIZES),
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_ab12cd","state":"started"}"#,
            ),
            ("POST /v1/sandboxes/cnd_ab12cd/harness ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        auth.create_sandbox(None).await.unwrap();
        let body = recorded(&seen)[1].1.clone().unwrap();
        assert_eq!((&body["cpu"], &body["memory"]), (&json!(2), &json!(4)));
    }

    #[tokio::test]
    async fn create_falls_back_to_todays_body_when_sizes_404() {
        let (gateway, seen) = fake(vec![
            (
                "GET /v1/sandboxes/personal-sizes ",
                404,
                r#"{"error":{"type":"not_found"}}"#,
            ),
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_ab12cd","state":"started"}"#,
            ),
            ("POST /v1/sandboxes/cnd_ab12cd/harness ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        let made = auth.create_sandbox(Some("large")).await.unwrap();
        assert_eq!(made.cpu, None);
        assert_eq!(made.price_per_hour_micro_usd, None);
        assert_eq!(
            recorded(&seen)[1].1,
            Some(create_body(None)),
            "no machine fields for a gateway that doesn't know sizes"
        );
    }

    #[tokio::test]
    async fn the_list_carries_the_sizes_and_prices_rows_or_has_none_on_old_gateways() {
        let (gateway, _) = fake(vec![
            (
                "GET /v1/sandboxes ",
                200,
                r#"{"sandboxes":[{"id":"cnd_1","provider":"fleet","state":"started","cpu":2,"memoryGiB":4}]}"#,
            ),
            ("GET /v1/sandboxes/personal-sizes ", 200, SIZES),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        let list = auth.sandboxes().await.unwrap().unwrap();
        assert_eq!(list.sizes.len(), 2);
        assert_eq!(list.sandboxes[0].price_per_hour_micro_usd, Some(100_000));

        let (gateway, _) = fake(vec![(
            "GET /v1/sandboxes ",
            200,
            r#"{"sandboxes":[{"id":"cnd_1","provider":"fleet","state":"started"}]}"#,
        )])
        .await;
        let auth = signed_in(dir.path(), gateway);
        let list = auth.sandboxes().await.unwrap().unwrap();
        assert!(list.enabled);
        assert!(list.sizes.is_empty(), "personal-sizes 404s");
        assert_eq!(list.sandboxes.len(), 1);
        let wire = serde_json::to_value(&list).unwrap();
        assert_eq!(wire["sizes"], json!([]));
    }

    // ---- the shared contract: apps/parity/vectors/cloud-sandbox.json -------------------------

    fn vector() -> Value {
        serde_json::from_str(include_str!(
            "../../../../apps/parity/vectors/cloud-sandbox.json"
        ))
        .unwrap()
    }

    #[test]
    fn the_shared_vector_state_words_and_hidden_rows() {
        let vector = vector();
        for (gateway, app) in vector["stateWords"].as_object().unwrap() {
            assert_eq!(
                normalise_state(Some(gateway)),
                app.as_str().unwrap(),
                "{gateway}"
            );
        }
        for hidden in vector["hiddenStates"].as_array().unwrap() {
            let list = json!([{"id": "x", "state": hidden}]);
            assert!(parse_list(&list).is_empty(), "{hidden} rows are dropped");
        }
    }

    #[test]
    fn the_shared_vector_device_names() {
        for case in vector()["deviceNames"].as_array().unwrap() {
            assert_eq!(
                device_name_for(case["id"].as_str().unwrap()),
                case["name"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn the_shared_vector_error_words() {
        for case in vector()["errors"].as_array().unwrap() {
            let status = case["status"].as_u64().unwrap() as u16;
            let message = case["message"].as_str();
            let failure = if status == 0 {
                Failure {
                    status,
                    body: None,
                    message: message.map(str::to_owned),
                }
            } else {
                Failure {
                    status,
                    body: (case["type"].is_string() || message.is_some()).then(
                        || json!({"error": {"type": case["type"], "message": case["message"]}}),
                    ),
                    message: None,
                }
            };
            assert_eq!(
                failure.sentence(case["fallback"].as_str().unwrap()),
                case["sentence"].as_str().unwrap(),
                "{case}"
            );
        }
    }

    /// `METHOD path` and the JSON body of every request the fake gateway saw.
    pub(in super::super) fn recorded(
        seen: &Arc<Mutex<Vec<String>>>,
    ) -> Vec<(String, Option<Value>)> {
        seen.lock()
            .unwrap()
            .iter()
            .map(|request| {
                let line = request.lines().next().unwrap_or_default();
                let call = line.trim_end_matches(" HTTP/1.1").to_owned();
                let body = request
                    .split_once("\r\n\r\n")
                    .and_then(|(_, body)| serde_json::from_str(body.trim()).ok());
                (call, body)
            })
            .collect()
    }

    /// A plan from the vector with `{id}` and `{deviceName}` filled in.
    fn expand(plan: &Value, id: &str, device_name: &str) -> Vec<(String, Option<Value>)> {
        fn fill(value: &Value, id: &str, name: &str) -> Value {
            match value {
                Value::String(text) => {
                    Value::String(text.replace("{id}", id).replace("{deviceName}", name))
                }
                Value::Object(map) => Value::Object(
                    map.iter()
                        .map(|(k, v)| (k.clone(), fill(v, id, name)))
                        .collect(),
                ),
                other => other.clone(),
            }
        }
        plan.as_array()
            .unwrap()
            .iter()
            .map(|call| {
                (
                    format!(
                        "{} {}",
                        call["method"].as_str().unwrap(),
                        fill(&call["path"], id, device_name).as_str().unwrap()
                    ),
                    call.get("body").map(|body| fill(body, id, device_name)),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn the_engine_makes_exactly_the_calls_the_shared_plans_name() {
        let vector = vector();
        let plans = &vector["plans"];
        let id = "cnd_ab12cd34ef56";
        let name = "Harness cloud ab12";

        let (gateway, seen) = fake(vec![
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_ab12cd34ef56","provider":"fleet","state":"started"}"#,
            ),
            ("POST /v1/sandboxes/cnd_ab12cd34ef56/harness ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        enable();
        auth.create_sandbox(None).await.unwrap();
        assert_eq!(
            recorded(&seen),
            expand(&plans["create"], id, name),
            "create"
        );

        let (gateway, seen) = fake(vec![
            (
                "GET /v1/sandboxes/personal-sizes ",
                200,
                Box::leak(vector["personalSizes"].to_string().into_boxed_str()),
            ),
            (
                "POST /v1/sandboxes ",
                201,
                r#"{"id":"cnd_ab12cd34ef56","provider":"fleet","state":"started"}"#,
            ),
            ("POST /v1/sandboxes/cnd_ab12cd34ef56/harness ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        auth.create_sandbox(Some("large")).await.unwrap();
        assert_eq!(
            recorded(&seen),
            expand(&plans["createLarge"], id, name),
            "create with a size"
        );

        for (label, state) in [
            ("startWhenPaused", "paused"),
            ("startWhenRunning", "started"),
        ] {
            let get = format!(r#"{{"id":"{id}","state":"{state}"}}"#);
            let get: &'static str = Box::leak(get.into_boxed_str());
            let (gateway, seen) = fake(vec![
                ("GET /v1/sandboxes/cnd_ab12cd34ef56 ", 200, get),
                ("POST /v1/sandboxes/cnd_ab12cd34ef56/start ", 200, r#"{}"#),
                ("POST /v1/sandboxes/cnd_ab12cd34ef56/harness ", 200, r#"{}"#),
            ])
            .await;
            let dir = tempfile::tempdir().unwrap();
            let auth = signed_in(dir.path(), gateway);
            let mut names = BTreeMap::new();
            names.insert(id.to_string(), name.to_string());
            auth.save_device_names(&names);
            auth.start_sandbox(id).await.unwrap();
            assert_eq!(recorded(&seen), expand(&plans[label], id, name), "{label}");
        }

        let (gateway, seen) = fake(vec![
            ("POST /v1/sandboxes/cnd_ab12cd34ef56/stop ", 200, r#"{}"#),
            ("DELETE /v1/sandboxes/cnd_ab12cd34ef56 ", 200, r#"{}"#),
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        auth.stop_sandbox(id).await.unwrap();
        auth.delete_sandbox(id).await.unwrap();
        let mut expected = expand(&plans["stop"], id, name);
        expected.extend(expand(&plans["delete"], id, name));
        assert_eq!(recorded(&seen), expected, "stop then delete");

        let (gateway, seen) = fake(vec![(
            "POST /v1/sandboxes/cnd_ab12cd34ef56/keep-running ",
            200,
            r#"{"id":"cnd_ab12cd34ef56","state":"started"}"#,
        )])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let auth = signed_in(dir.path(), gateway);
        auth.set_sandbox_keep_running(id, true).await.unwrap();
        auth.set_sandbox_keep_running(id, false).await.unwrap();
        let mut expected = expand(&plans["keepRunningOn"], id, name);
        expected.extend(expand(&plans["keepRunningOff"], id, name));
        assert_eq!(recorded(&seen), expected, "keep running on then off");

        for never in plans["neverCalled"].as_array().unwrap() {
            let never = never.as_str().unwrap();
            assert!(
                recorded(&seen)
                    .iter()
                    .all(|(call, _)| !call.contains(never)),
                "{never} must never be called"
            );
        }
    }
}

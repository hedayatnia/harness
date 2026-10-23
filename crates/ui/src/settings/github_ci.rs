//! Settings → GitHub CI: run GitHub Actions jobs (`runs-on: codegraff`) on Codegraff sandboxes.
//!
//! Not connected, the page explains what happens and what it costs, offers Connect GitHub (the
//! Codegraff app's install page, in the browser) and a workflow snippet to copy. After Connect the
//! page re-reads every [`CONNECT_POLL_SECS`] for up to [`CONNECT_WATCH_SECS`], and whenever the
//! window comes back to the front, so the new installation appears once GitHub sends the user
//! back. Connected, each installation has its monthly budgets, a cap on concurrent jobs and its
//! repositories (on/off and default size), with the recent jobs below.
//!
//! Everything reads the engine's `CodegraffGithubRunners` (pure text in
//! [`crate::codegraff_runners`]); a gateway without runners shows the explanation with Connect
//! disabled instead of an error.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::Utc;
use gpui::{
    AnyElement, ClipboardItem, Context, Entity, SharedString, Subscription, Task, Window, div,
    prelude::*, px,
};

use harness_rpc::methods;

use crate::codegraff_runners::{
    CONNECT_POLL_SECS, CONNECT_WATCH_SECS, EXPLAINER, GithubRunners, JobTone, MAX_CONCURRENT,
    Panel, RunnerInstallation, RunnerRepo, YAML_SNIPPET, budget_fraction, budget_input_text,
    install_url, installation_meta, job_meta, job_status, job_title, job_url, jobs_working, panel,
    parse_budget, repo_size_label, size_option_label, spend_line, step_concurrent,
};
use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::popover::{self, Loadable};
use crate::settings::accounts::{usage_color, usage_level};
use crate::settings::widgets;
use crate::state::AppState;
use crate::theme::Theme;

/// While a job is queued or running, the page re-reads this often.
const JOBS_POLL: Duration = Duration::from_secs(20);
/// Coming back to the window re-reads at most this often.
const FOCUS_RELOAD_GAP: Duration = Duration::from_secs(5);
/// How many recent jobs the page lists.
const JOBS_SHOWN: usize = 12;

/// Which budget an input edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Budget {
    Monthly,
    /// Jobs from outside contributors (fork pull requests).
    Untrusted,
}

impl Budget {
    fn key(self) -> &'static str {
        match self {
            Budget::Monthly => "monthlyBudgetMicroUsd",
            Budget::Untrusted => "untrustedBudgetMicroUsd",
        }
    }

    fn current(self, installation: &RunnerInstallation) -> Option<i64> {
        match self {
            Budget::Monthly => installation.monthly_budget_micro_usd,
            Budget::Untrusted => installation.untrusted_budget_micro_usd,
        }
    }
}

pub struct GithubCiPage {
    state: Entity<AppState>,
    scroll: widgets::PageScroll,
    runners: Loadable<Option<GithubRunners>>,
    error: Option<SharedString>,
    /// The action in flight (`"<installation>:<what>"`); one at a time.
    busy: Option<String>,
    /// The installation whose Disconnect asks "for good?".
    confirm_disconnect: Option<String>,
    budget_inputs: HashMap<(String, Budget), Entity<ComposerInput>>,
    budget_errors: HashMap<(String, Budget), String>,
    /// The repository whose size menu is open: `(installation id, full name)`.
    size_menu: popover::Popup<(String, String)>,
    /// After Connect: keep re-reading until this moment (or until an installation appears).
    connect_until: Option<Instant>,
    snippet_copied: bool,
    last_load: Option<Instant>,
    load_task: Option<Task<()>>,
    poll_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
    copy_task: Option<Task<()>>,
    activation: Option<Subscription>,
    _inputs: Vec<Subscription>,
}

impl GithubCiPage {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            state,
            scroll: widgets::PageScroll::default(),
            runners: Loadable::Idle,
            error: None,
            busy: None,
            confirm_disconnect: None,
            budget_inputs: HashMap::new(),
            budget_errors: HashMap::new(),
            size_menu: popover::Popup::default(),
            connect_until: None,
            snippet_copied: false,
            last_load: None,
            load_task: None,
            poll_task: None,
            action_task: None,
            copy_task: None,
            activation: None,
            _inputs: Vec::new(),
        };
        page.load(cx);
        page
    }

    /// Show an answer without asking the engine (tests and fixtures).
    #[cfg(test)]
    pub(crate) fn set_runners(&mut self, runners: Option<GithubRunners>, cx: &mut Context<Self>) {
        self.load_task = None;
        self.apply(Ok(runners), cx);
    }

    /// Read the runners. Rows already on screen stay through a reload.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            self.runners = Loadable::Error("Engine not connected".into());
            return;
        };
        if !matches!(self.runners, Loadable::Ready(_)) {
            self.runners = Loadable::Loading;
        }
        self.last_load = Some(Instant::now());
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(methods::CODEGRAFF_GITHUB_RUNNERS, serde_json::json!({}))
                .await
                .map_err(|e| e.to_string())
                .and_then(|value| {
                    serde_json::from_value::<Option<GithubRunners>>(value)
                        .map_err(|e| format!("Unexpected reply: {e}"))
                });
            this.update(cx, |page, cx| page.apply(result, cx)).ok();
        }));
        cx.notify();
    }

    fn apply(&mut self, result: Result<Option<GithubRunners>, String>, cx: &mut Context<Self>) {
        match result {
            Ok(runners) => {
                if let Some(runners) = &runners {
                    self.ensure_budget_inputs(runners, cx);
                    // The installation the user went to the browser for has arrived.
                    if !runners.installations.is_empty() {
                        self.connect_until = None;
                    }
                }
                self.runners = Loadable::Ready(runners);
            }
            Err(error) => {
                if matches!(self.runners, Loadable::Ready(_)) {
                    self.error = Some(error.into());
                } else {
                    self.runners = Loadable::Error(error);
                }
            }
        }
        self.schedule_poll(cx);
        cx.notify();
    }

    /// Re-read on a timer while waiting for a Connect to land or while a job is working.
    fn schedule_poll(&mut self, cx: &mut Context<Self>) {
        let watching = self
            .connect_until
            .is_some_and(|until| Instant::now() < until);
        if !watching {
            self.connect_until = None;
        }
        let working = matches!(&self.runners, Loadable::Ready(Some(r)) if jobs_working(r));
        let every = match (watching, working) {
            (true, _) => Duration::from_secs(CONNECT_POLL_SECS),
            (false, true) => JOBS_POLL,
            (false, false) => {
                self.poll_task = None;
                return;
            }
        };
        self.poll_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(every).await;
            this.update(cx, |page, cx| page.load(cx)).ok();
        }));
    }

    /// One input per installation budget, created on first sight with the saved amount.
    fn ensure_budget_inputs(&mut self, runners: &GithubRunners, cx: &mut Context<Self>) {
        for installation in &runners.installations {
            for budget in [Budget::Monthly, Budget::Untrusted] {
                let key = (installation.installation_id.clone(), budget);
                if self.budget_inputs.contains_key(&key) {
                    continue;
                }
                let text = budget_input_text(budget.current(installation));
                let input = cx.new(|cx| {
                    let mut input = ComposerInput::new("Amount", cx);
                    input.set_text(text, cx);
                    input
                });
                let id = installation.installation_id.clone();
                self._inputs
                    .push(
                        cx.subscribe(&input, move |page: &mut Self, _, event, cx| match event {
                            ComposerInputEvent::Submitted => {
                                page.save_budget(id.clone(), budget, cx)
                            }
                            ComposerInputEvent::Edited => {
                                if page.budget_errors.remove(&(id.clone(), budget)).is_some() {
                                    cx.notify();
                                }
                            }
                            _ => {}
                        }),
                    );
                self.budget_inputs.insert(key, input);
            }
        }
    }

    /// Run one `CodegraffGithubRunnerAction`, then re-read. `busy` names it for its button.
    fn act(&mut self, busy: String, params: serde_json::Value, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        self.busy = Some(busy);
        self.error = None;
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(methods::CODEGRAFF_GITHUB_RUNNER_ACTION, params)
                .await;
            this.update(cx, |page, cx| {
                page.busy = None;
                if let Err(error) = result {
                    page.error = Some(error.to_string().into());
                }
                page.load(cx);
            })
            .ok();
        }));
        cx.notify();
    }

    fn save_budget(&mut self, installation_id: String, budget: Budget, cx: &mut Context<Self>) {
        let key = (installation_id.clone(), budget);
        let Some(input) = self.budget_inputs.get(&key) else {
            return;
        };
        let text = input.read(cx).text().to_string();
        let amount = match parse_budget(&text) {
            Ok(amount) => amount,
            Err(problem) => {
                self.budget_errors.insert(key, problem);
                cx.notify();
                return;
            }
        };
        self.budget_errors.remove(&key);
        input.update(cx, |input, cx| {
            input.set_text(budget_input_text(Some(amount)), cx)
        });
        let mut params = serde_json::json!({
            "action": "installation",
            "installationId": installation_id,
        });
        params[budget.key()] = serde_json::json!(amount);
        self.act(format!("{installation_id}:{}", budget.key()), params, cx);
    }

    fn set_concurrent(&mut self, installation_id: String, max: u32, cx: &mut Context<Self>) {
        self.act(
            format!("{installation_id}:concurrent"),
            serde_json::json!({
                "action": "installation",
                "installationId": installation_id,
                "maxConcurrent": max,
            }),
            cx,
        );
    }

    fn set_repo(
        &mut self,
        installation_id: String,
        full_name: String,
        change: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let mut params = serde_json::json!({
            "action": "repo",
            "installationId": installation_id,
            "fullName": full_name,
        });
        if let (Some(params), Some(change)) = (params.as_object_mut(), change.as_object()) {
            params.extend(change.clone());
        }
        self.act(format!("{installation_id}:repo:{full_name}"), params, cx);
    }

    fn disconnect(&mut self, installation_id: String, cx: &mut Context<Self>) {
        if self.confirm_disconnect.as_deref() != Some(installation_id.as_str()) {
            self.confirm_disconnect = Some(installation_id);
            cx.notify();
            return;
        }
        self.confirm_disconnect = None;
        self.act(
            format!("{installation_id}:disconnect"),
            serde_json::json!({ "action": "disconnect", "installationId": installation_id }),
            cx,
        );
    }

    /// Open the Codegraff app's install page and watch for the installation to arrive.
    fn connect(&mut self, url: String, cx: &mut Context<Self>) {
        cx.open_url(&url);
        self.connect_until = Some(Instant::now() + Duration::from_secs(CONNECT_WATCH_SECS));
        self.schedule_poll(cx);
        cx.notify();
    }

    fn copy_snippet(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(YAML_SNIPPET.to_string()));
        self.snippet_copied = true;
        self.copy_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            this.update(cx, |page, cx| {
                page.snippet_copied = false;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn close_size_menu(&mut self, cx: &mut Context<Self>) {
        if self.size_menu.begin_close() {
            popover::reap_popup(cx, |page: &mut Self| &mut page.size_menu);
            cx.notify();
        }
    }

    fn on_scroll_hovered(&mut self, hovered: &bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.scroll.set_list_hovered(*hovered) {
            cx.notify();
        }
    }

    fn is_busy(&self, what: &str) -> bool {
        self.busy.as_deref() == Some(what)
    }

    // ---- rendering --------------------------------------------------------------------------

    fn quiet_text(theme: &Theme, text: impl Into<SharedString>) -> gpui::Div {
        div()
            .text_size(crate::typography::ui_rems(widgets::ROW_DESCRIPTION_SIZE))
            .text_color(theme.text_muted)
            .child(text.into())
    }

    /// The explanation, Connect and the snippet: everything before an installation exists.
    fn render_intro(
        &self,
        theme: &Theme,
        runners: Option<&GithubRunners>,
        which: Panel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = runners.and_then(install_url).map(str::to_owned);
        let open = which == Panel::NotConnected && url.is_some();
        let label = match which {
            Panel::ComingSoon | Panel::Unavailable => "Coming soon",
            _ if self.connect_until.is_some() => "Waiting for GitHub…",
            _ => "Connect GitHub",
        };
        let note = match which {
            Panel::Unavailable => {
                Some("GitHub CI isn't available on Codegraff yet. This page fills in once it is.")
            }
            Panel::ComingSoon => Some("GitHub CI is coming soon to your Codegraff account."),
            _ if self.connect_until.is_some() => Some(
                "Finish installing the Codegraff app on GitHub in your browser. This page updates when you come back.",
            ),
            _ if url.is_none() => Some("Codegraff didn't send an install link. Try Refresh."),
            _ => None,
        };
        let button = div()
            .id("github-ci-connect")
            .debug_selector(|| "github-ci-connect".into())
            .flex_none()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .text_size(crate::typography::ui_rems(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .when(open, |el| {
                el.bg(theme.solid)
                    .text_color(theme.on_solid)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
            })
            .when(!open, |el| {
                el.bg(crate::theme::ink(0.06))
                    .text_color(theme.text_muted.opacity(0.7))
            })
            .when_some(url.filter(|_| open), |el, url| {
                el.on_click(cx.listener(move |page, _, _, cx| page.connect(url.clone(), cx)))
            })
            .child(SharedString::from(label));
        widgets::section_card(theme)
            .mt(px(16.0))
            .debug_selector(move || format!("github-ci-intro-{which:?}"))
            .child(
                widgets::card_row(theme, true)
                    .items_start()
                    .child(widgets::row_tile(theme, crate::icons::PULL_REQUEST))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(widgets::row_title(theme, "Run GitHub Actions on Codegraff"))
                            .child(Self::quiet_text(theme, EXPLAINER))
                            .when_some(note, |el, note| {
                                el.child(
                                    div()
                                        .mt(px(4.0))
                                        .text_size(crate::typography::ui_rems(12.0))
                                        .text_color(theme.text_muted.opacity(0.8))
                                        .child(SharedString::from(note)),
                                )
                            }),
                    )
                    .child(button),
            )
            .child(self.render_snippet(theme, true, cx))
            .into_any_element()
    }

    /// The `runs-on: codegraff` snippet with Copy.
    fn render_snippet(&self, theme: &Theme, inside: bool, cx: &mut Context<Self>) -> AnyElement {
        let copied = self.snippet_copied;
        div()
            .when(inside, |el| el.border_t_1().border_color(theme.border))
            .px(px(20.0))
            .py(px(12.0))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(Self::quiet_text(
                        theme,
                        "In a workflow, run a job on Codegraff:",
                    ))
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(8.0))
                            .bg(crate::theme::ink(0.04))
                            .border_1()
                            .border_color(theme.border)
                            .font_family(theme.font_mono.clone())
                            .text_size(crate::typography::ui_rems(12.0))
                            .text_color(theme.text)
                            .children(
                                YAML_SNIPPET
                                    .lines()
                                    .map(|line| div().child(SharedString::from(line.to_string()))),
                            ),
                    ),
            )
            .child(
                widgets::ghost_action(theme)
                    .id("github-ci-copy-snippet")
                    .flex_none()
                    .border_1()
                    .border_color(theme.border)
                    .hover(|s| widgets::ghost_hover(theme, s))
                    .on_click(cx.listener(|page, _, _, cx| page.copy_snippet(cx)))
                    .child(
                        crate::icons::icon(crate::icons::COPY)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
                    .child(if copied { "Copied" } else { "Copy" }),
            )
            .into_any_element()
    }

    fn render_budget_row(
        &self,
        theme: &Theme,
        installation: &RunnerInstallation,
        budget: Budget,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = installation.installation_id.clone();
        let key = (id.clone(), budget);
        let busy = self.is_busy(&format!("{id}:{}", budget.key()));
        let (title, description) = match budget {
            Budget::Monthly => ("Monthly budget", spend_line(installation)),
            Budget::Untrusted => (
                "Fork pull request budget",
                "Jobs from outside contributors stop at this amount each month.".to_string(),
            ),
        };
        let meter = (budget == Budget::Monthly)
            .then(|| budget_fraction(installation))
            .flatten()
            .map(|fraction| {
                let fill = usage_color(usage_level(fraction), theme);
                div()
                    .mt(px(6.0))
                    .w_full()
                    .max_w(px(260.0))
                    .h(px(5.0))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(crate::theme::ink(0.07))
                    .when(fraction > 0.0, |el| {
                        el.child(
                            div()
                                .h_full()
                                .w(gpui::relative(fraction.max(0.015)))
                                .rounded_full()
                                .bg(fill.opacity(0.85)),
                        )
                    })
            });
        let problem = self.budget_errors.get(&key).cloned();
        let field = self.budget_inputs.get(&key).cloned().map(|input| {
            div()
                .w(px(112.0))
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if problem.is_some() {
                    theme.danger.opacity(0.5)
                } else {
                    theme.border
                })
                .bg(crate::theme::ink(0.03))
                .text_size(crate::typography::ui_rems(12.5))
                .child(div().text_color(theme.text_muted).child("$"))
                .child(div().flex_1().min_w_0().child(input))
        });
        let budget_id = id.clone();
        widgets::card_row(theme, false)
            .id((
                "github-ci-budget",
                ix * 2 + usize::from(budget == Budget::Untrusted),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(widgets::row_title(theme, title))
                    .child(Self::quiet_text(theme, description))
                    .children(meter)
                    .when_some(problem, |el, problem| {
                        el.child(
                            div()
                                .mt(px(4.0))
                                .text_size(crate::typography::ui_rems(12.0))
                                .text_color(theme.danger_muted.opacity(0.9))
                                .child(SharedString::from(problem)),
                        )
                    }),
            )
            .children(field)
            .child(
                widgets::ghost_action(theme)
                    .id((
                        "github-ci-budget-save",
                        ix * 2 + usize::from(budget == Budget::Untrusted),
                    ))
                    .flex_none()
                    .border_1()
                    .border_color(theme.border)
                    .hover(|s| widgets::ghost_hover(theme, s))
                    .when(busy, |el| el.opacity(0.5))
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.save_budget(budget_id.clone(), budget, cx)
                    }))
                    .child(if busy { "Saving…" } else { "Save" }),
            )
            .into_any_element()
    }

    fn render_concurrency_row(
        &self,
        theme: &Theme,
        installation: &RunnerInstallation,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = installation.installation_id.clone();
        let current = installation
            .max_concurrent
            .unwrap_or(1)
            .clamp(1, MAX_CONCURRENT);
        let busy = self.is_busy(&format!("{id}:concurrent"));
        let step = |label: &'static str, delta: i32, slot: usize, cx: &mut Context<Self>| {
            let next = step_concurrent(Some(current), delta);
            let enabled = next != current && !busy;
            let id = id.clone();
            div()
                .id(("github-ci-concurrent", ix * 2 + slot))
                .flex_none()
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(theme.border)
                .text_color(if enabled {
                    theme.text
                } else {
                    theme.text_muted.opacity(0.4)
                })
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.bg(crate::theme::ink(0.06)))
                        .on_click(cx.listener(move |page, _, _, cx| {
                            page.set_concurrent(id.clone(), next, cx)
                        }))
                })
                .child(SharedString::from(label))
        };
        widgets::card_row(theme, false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(widgets::row_title(theme, "Concurrent jobs"))
                    .child(Self::quiet_text(
                        theme,
                        format!("How many jobs run at once, 1 to {MAX_CONCURRENT}. More wait in the queue."),
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(crate::typography::ui_rems(13.0))
                    .child(step("−", -1, 0, cx))
                    .child(
                        div()
                            .w(px(16.0))
                            .text_center()
                            .text_color(theme.text)
                            .when(busy, |el| el.opacity(0.5))
                            .child(SharedString::from(current.to_string())),
                    )
                    .child(step("+", 1, 1, cx)),
            )
            .into_any_element()
    }

    fn render_repo_row(
        &self,
        theme: &Theme,
        installation: &RunnerInstallation,
        repo: &RunnerRepo,
        sizes: &[crate::codegraff_runners::RunnerSize],
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = installation.installation_id.clone();
        let name = repo.full_name.clone();
        let busy = self.is_busy(&format!("{id}:repo:{name}"));
        let enabled = repo.enabled;
        let accent = theme.accent;
        let menu_key = (id.clone(), name.clone());
        let menu_open = self.size_menu.get() == Some(&menu_key);
        let size_trigger = (!sizes.is_empty()).then(|| {
            let key = menu_key.clone();
            let key_for_press = menu_key.clone();
            let mut trigger = div()
                .id(("github-ci-repo-size", ix))
                .relative()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(theme.border)
                .text_size(crate::typography::ui_rems(12.0))
                .text_color(theme.text_muted)
                .cursor_pointer()
                .hover(|s| s.bg(crate::theme::ink(0.05)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |page, _, _, _| {
                        let key = key_for_press.clone();
                        page.size_menu
                            .note_trigger_press_matching(|open| *open == key)
                    }),
                )
                .on_click(cx.listener(move |page, _, _, cx| {
                    if page.size_menu.take_press_was_open() {
                        page.close_size_menu(cx);
                    } else {
                        page.size_menu.open(key.clone());
                    }
                    cx.notify();
                }))
                .child(SharedString::from(repo_size_label(repo, sizes)))
                .child(
                    crate::icons::icon(crate::icons::SORT_VERTICAL)
                        .size(px(12.0))
                        .text_color(theme.text_muted.opacity(0.6)),
                );
            if menu_open {
                let closing = self.size_menu.closing_since();
                let current = repo.default_size.clone();
                let menu = popover::popover_card(theme)
                    .w(px(260.0))
                    .on_mouse_down_out(cx.listener(|page, _, _, cx| page.close_size_menu(cx)))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(popover::menu_heading(theme, "Default size"))
                    .children(sizes.iter().enumerate().map(|(size_ix, size)| {
                        let active = current.as_deref() == Some(size.key.as_str());
                        let key = size.key.clone();
                        let id = id.clone();
                        let name = name.clone();
                        popover::menu_row(theme, active, format!("github-ci-size-{size_ix}"))
                            .id(("github-ci-size-row", size_ix))
                            .on_click(cx.listener(move |page, _, _, cx| {
                                page.close_size_menu(cx);
                                page.set_repo(
                                    id.clone(),
                                    name.clone(),
                                    serde_json::json!({ "defaultSize": key }),
                                    cx,
                                );
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(SharedString::from(size_option_label(size))),
                            )
                    }))
                    .into_any_element();
                trigger = trigger.child(popover::anchored_menu_below_end(
                    "github-ci-size-menu",
                    menu,
                    closing,
                ));
            }
            trigger
        });
        let toggle_id = id.clone();
        let toggle_name = name.clone();
        widgets::card_row(theme, false)
            .id(("github-ci-repo", ix))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(widgets::row_title(theme, repo.full_name.clone()))
                    .when(repo.private, |el| {
                        el.child(widgets::badge(theme, "Private"))
                    }),
            )
            .children(size_trigger)
            .child(
                div()
                    .id(("github-ci-repo-switch", ix))
                    .flex_none()
                    .size(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .role(gpui::Role::Switch)
                    .aria_label(SharedString::from(format!(
                        "Run {} jobs on Codegraff",
                        repo.full_name
                    )))
                    .aria_toggled(if enabled {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .tab_index(0)
                    .focus_visible(move |style| style.border_2().border_color(accent))
                    .when(busy, |el| el.opacity(0.5))
                    .cursor_pointer()
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.set_repo(
                            toggle_id.clone(),
                            toggle_name.clone(),
                            serde_json::json!({ "enabled": !enabled }),
                            cx,
                        )
                    }))
                    .child(widgets::toggle_switch(theme, enabled)),
            )
            .into_any_element()
    }

    fn render_installation(
        &self,
        theme: &Theme,
        runners: &GithubRunners,
        installation: &RunnerInstallation,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = installation.installation_id.clone();
        let confirming = self.confirm_disconnect.as_deref() == Some(id.as_str());
        let disconnecting = self.is_busy(&format!("{id}:disconnect"));
        let head = widgets::card_row(theme, true)
            .child(widgets::row_tile(theme, crate::icons::PULL_REQUEST))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(widgets::row_title(theme, installation.account.clone()))
                    .child(Self::quiet_text(theme, installation_meta(installation))),
            )
            .child(
                widgets::ghost_action(theme)
                    .id(("github-ci-disconnect", ix))
                    .flex_none()
                    .hover(|s| widgets::ghost_hover(theme, s))
                    .when(confirming, |el| el.text_color(theme.danger_muted))
                    .when(disconnecting, |el| el.opacity(0.5))
                    .on_click(cx.listener(move |page, _, _, cx| page.disconnect(id.clone(), cx)))
                    .child(match (disconnecting, confirming) {
                        (true, _) => "Disconnecting…",
                        (_, true) => "Disconnect for good?",
                        _ => "Disconnect",
                    }),
            );
        let repos: Vec<AnyElement> = installation
            .repos
            .iter()
            .enumerate()
            .map(|(repo_ix, repo)| {
                self.render_repo_row(
                    theme,
                    installation,
                    repo,
                    &runners.sizes,
                    ix * 1000 + repo_ix,
                    cx,
                )
            })
            .collect();
        widgets::section_card(theme)
            .mt(px(16.0))
            .debug_selector(move || format!("github-ci-installation-{ix}"))
            .child(head)
            .child(self.render_budget_row(theme, installation, Budget::Monthly, ix, cx))
            .child(self.render_budget_row(theme, installation, Budget::Untrusted, ix, cx))
            .child(self.render_concurrency_row(theme, installation, ix, cx))
            .when(repos.is_empty(), |el| {
                el.child(widgets::card_row(theme, false).child(Self::quiet_text(
                    theme,
                    "No repositories yet. Choose them in the Codegraff app's settings on GitHub.",
                )))
            })
            .children(repos)
            .into_any_element()
    }

    fn render_jobs(
        &self,
        theme: &Theme,
        runners: &GithubRunners,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = Utc::now();
        let rows: Vec<AnyElement> = runners
            .recent_jobs
            .iter()
            .take(JOBS_SHOWN)
            .enumerate()
            .map(|(ix, job)| {
                let (status, tone) = job_status(job);
                let color = match tone {
                    JobTone::Good => theme.success,
                    JobTone::Bad => theme.danger,
                    JobTone::Working => theme.accent,
                    JobTone::Quiet => theme.text_muted.opacity(0.5),
                };
                let meta = job_meta(job, now);
                let link = job_url(job).map(str::to_owned);
                widgets::card_row(theme, ix == 0)
                    .id(("github-ci-job", ix))
                    .child(div().size(px(7.0)).flex_none().rounded_full().bg(color))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(
                                theme,
                                format!("{} · {}", job.repo, job_title(job)),
                            ))
                            .child(Self::quiet_text(
                                theme,
                                if meta.is_empty() {
                                    status.clone()
                                } else {
                                    format!("{status} · {meta}")
                                },
                            )),
                    )
                    .when_some(link, |el, link| {
                        el.child(
                            widgets::ghost_action(theme)
                                .id(("github-ci-job-open", ix))
                                .flex_none()
                                .hover(|s| widgets::ghost_hover(theme, s))
                                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&link)))
                                .child("Open on GitHub"),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        div()
            .mt(px(24.0))
            .flex()
            .flex_col()
            .debug_selector(|| "github-ci-jobs".into())
            .child(
                div()
                    .text_size(crate::typography::ui_rems(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .child("Recent jobs"),
            )
            .child(widgets::section_card(theme).mt(px(8.0)).map(|card| {
                if rows.is_empty() {
                    card.child(widgets::card_row(theme, true).child(Self::quiet_text(
                        theme,
                        "No jobs yet. Push a workflow with runs-on: codegraff to start one.",
                    )))
                } else {
                    card.children(rows)
                }
            }))
            .into_any_element()
    }

    fn render_body(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        match &self.runners {
            Loadable::Idle | Loadable::Loading => div()
                .mt(px(24.0))
                .text_size(crate::typography::ui_rems(12.5))
                .text_color(theme.text_muted.opacity(0.6))
                .child("Loading GitHub CI…")
                .into_any_element(),
            Loadable::Error(message) => div()
                .child(widgets::error_strip(theme, message.clone()))
                .child(
                    widgets::ghost_action(theme)
                        .id("github-ci-retry")
                        .mt(px(8.0))
                        .hover(|s| s.bg(crate::theme::ink(0.06)))
                        .on_click(cx.listener(|page, _, _, cx| page.load(cx)))
                        .child("Retry"),
                )
                .into_any_element(),
            Loadable::Ready(runners) => {
                let runners = runners.clone();
                match panel(runners.as_ref()) {
                    Panel::SignedOut => widgets::section_card(theme)
                        .mt(px(16.0))
                        .debug_selector(|| "github-ci-signed-out".into())
                        .child(widgets::card_row(theme, true).child(Self::quiet_text(
                            theme,
                            "Sign in with Codegraff from the account menu to run GitHub Actions on Codegraff.",
                        )))
                        .into_any_element(),
                    which @ (Panel::Unavailable | Panel::ComingSoon | Panel::NotConnected) => {
                        self.render_intro(theme, runners.as_ref(), which, cx)
                    }
                    Panel::Connected => {
                        let runners = runners.unwrap_or_default();
                        let url = install_url(&runners).map(str::to_owned);
                        let cards: Vec<AnyElement> = runners
                            .installations
                            .iter()
                            .enumerate()
                            .map(|(ix, installation)| {
                                self.render_installation(theme, &runners, installation, ix, cx)
                            })
                            .collect();
                        div()
                            .flex()
                            .flex_col()
                            .children(cards)
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .when_some(url, |el, url| {
                                        el.child(
                                            widgets::ghost_action(theme)
                                                .id("github-ci-add-account")
                                                .hover(|s| widgets::ghost_hover(theme, s))
                                                .on_click(cx.listener(move |page, _, _, cx| {
                                                    page.connect(url.clone(), cx)
                                                }))
                                                .child(if self.connect_until.is_some() {
                                                    "Waiting for GitHub…"
                                                } else {
                                                    "Connect another account"
                                                }),
                                        )
                                    }),
                            )
                            .child(self.render_jobs(theme, &runners, cx))
                            .child(
                                widgets::section_card(theme)
                                    .mt(px(16.0))
                                    .child(self.render_snippet(theme, false, cx)),
                            )
                            .into_any_element()
                    }
                }
            }
        }
    }
}

impl popover::ScrollRailHost for GithubCiPage {
    fn rail_bar(&mut self) -> &mut popover::MenuScrollbarState {
        self.scroll.rail_bar()
    }

    fn rail_scroll(&self) -> Option<gpui::ScrollHandle> {
        self.scroll.rail_scroll()
    }
}

impl Render for GithubCiPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Coming back from the browser (after installing the app) re-reads the page.
        if self.activation.is_none() {
            self.activation = Some(cx.observe_window_activation(
                window,
                |page: &mut Self, window, cx| {
                    let stale = page
                        .last_load
                        .is_none_or(|at| at.elapsed() >= FOCUS_RELOAD_GAP);
                    if window.is_window_active() && stale {
                        page.load(cx);
                    }
                },
            ));
        }
        let theme = Theme::of(cx).clone();
        let refreshing = self.load_task.is_some() && self.runners.is_loading();
        let refresh = widgets::ghost_action(&theme)
            .id("github-ci-refresh")
            .border_1()
            .border_color(theme.border)
            .when(refreshing, |el| el.opacity(0.5))
            .hover(|s| widgets::ghost_hover(&theme, s))
            .on_click(cx.listener(|page, _, _, cx| page.load(cx)))
            .child(
                crate::icons::icon(crate::icons::REFRESH)
                    .size(px(14.0))
                    .text_color(theme.text_muted),
            )
            .child("Refresh");
        let body = self.render_body(&theme, cx);
        let scrollbar = popover::rail(self, "github-ci-page-scrollbar", &theme, cx);
        div()
            .id("github-ci-page-host")
            .relative()
            .size_full()
            .on_hover(cx.listener(Self::on_scroll_hovered))
            .child(
                div()
                    .id("github-ci-page")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll.scroll)
                    .child(
                        widgets::page_column()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(widgets::page_header(&theme, "GitHub CI", None))
                                    .child(refresh),
                            )
                            .child(widgets::page_subtitle(
                                &theme,
                                "Run GitHub Actions jobs on Codegraff sandboxes, billed per second from your Codegraff credit.",
                            ))
                            .when_some(self.error.clone(), |el, message| {
                                el.child(
                                    widgets::error_strip(&theme, message)
                                        .id("github-ci-error")
                                        .cursor_pointer()
                                        .on_click(cx.listener(|page, _, _, cx| {
                                            page.error = None;
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(body),
                    ),
            )
            .children(scrollbar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runners(value: serde_json::Value) -> Option<GithubRunners> {
        Some(serde_json::from_value(value).unwrap())
    }

    fn open_page(
        cx: &mut gpui::TestAppContext,
    ) -> (Entity<GithubCiPage>, &mut gpui::VisualTestContext) {
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
        });
        let state = cx.new(|_| AppState::new());
        cx.add_window_view(|_, cx| GithubCiPage::new(state, cx))
    }

    fn redraw(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
    }

    fn shows(cx: &mut gpui::VisualTestContext, selector: &'static str) -> bool {
        cx.debug_bounds(selector).is_some()
    }

    #[gpui::test]
    fn each_state_shows_its_panel(cx: &mut gpui::TestAppContext) {
        let (page, cx) = open_page(cx);
        redraw(cx);
        assert!(!shows(cx, "github-ci-intro-NotConnected"), "no engine yet");

        page.update(cx, |page, cx| page.set_runners(None, cx));
        redraw(cx);
        assert!(shows(cx, "github-ci-signed-out"));

        page.update(cx, |page, cx| {
            page.set_runners(Some(GithubRunners::default()), cx)
        });
        redraw(cx);
        assert!(shows(cx, "github-ci-intro-Unavailable"));
        assert!(shows(cx, "github-ci-connect"), "shown, disabled");

        page.update(cx, |page, cx| {
            page.set_runners(
                runners(serde_json::json!({"available": true, "enabled": false})),
                cx,
            )
        });
        redraw(cx);
        assert!(shows(cx, "github-ci-intro-ComingSoon"));

        page.update(cx, |page, cx| {
            page.set_runners(
                runners(serde_json::json!({
                    "available": true, "enabled": true,
                    "installUrl": "https://github.com/apps/codegraff/installations/new"
                })),
                cx,
            )
        });
        redraw(cx);
        assert!(shows(cx, "github-ci-intro-NotConnected"));
        assert!(shows(cx, "github-ci-connect"));
        assert!(!shows(cx, "github-ci-installation-0"));

        page.update(cx, |page, cx| {
            page.set_runners(
                runners(serde_json::json!({
                    "available": true, "enabled": true,
                    "installUrl": "https://github.com/apps/codegraff/installations/new",
                    "installations": [{
                        "installationId": "4242", "account": "octo-org",
                        "monthlyBudgetMicroUsd": 50000000, "usedThisMonthMicroUsd": 12500000,
                        "maxConcurrent": 3,
                        "repos": [{"fullName": "octo-org/app", "enabled": true, "defaultSize": "standard"}]
                    }],
                    "recentJobs": [{
                        "repo": "octo-org/app", "workflow": "CI", "jobName": "test",
                        "state": "completed", "conclusion": "success", "durationSeconds": 180,
                        "costMicroUsd": 5000,
                        "htmlUrl": "https://github.com/octo-org/app/actions/runs/1/job/2"
                    }],
                    "sizes": [{"key": "standard", "cpu": 2, "memoryGiB": 4, "pricePerHourMicroUsd": 100000}]
                })),
                cx,
            )
        });
        redraw(cx);
        assert!(shows(cx, "github-ci-installation-0"));
        assert!(shows(cx, "github-ci-jobs"));
        assert!(!shows(cx, "github-ci-intro-NotConnected"));
        page.read_with(cx, |page, cx| {
            let input = page
                .budget_inputs
                .get(&("4242".to_string(), Budget::Monthly))
                .expect("a budget input per installation");
            assert_eq!(input.read(cx).text(), "50");
        });
    }

    #[gpui::test]
    fn a_bad_budget_is_refused_in_place(cx: &mut gpui::TestAppContext) {
        let (page, cx) = open_page(cx);
        page.update(cx, |page, cx| {
            page.set_runners(
                runners(serde_json::json!({
                    "available": true, "enabled": true,
                    "installations": [{"installationId": "1", "account": "me"}]
                })),
                cx,
            )
        });
        page.update(cx, |page, cx| {
            let input = page
                .budget_inputs
                .get(&("1".to_string(), Budget::Monthly))
                .unwrap()
                .clone();
            input.update(cx, |input, cx| input.set_text("twelve", cx));
            page.save_budget("1".into(), Budget::Monthly, cx);
            assert!(
                page.budget_errors
                    .contains_key(&("1".to_string(), Budget::Monthly))
            );
            assert_eq!(page.busy, None, "nothing was sent");
        });
    }

    #[gpui::test]
    fn connect_watches_until_an_installation_arrives(cx: &mut gpui::TestAppContext) {
        let (page, cx) = open_page(cx);
        page.update(cx, |page, cx| {
            page.set_runners(
                runners(serde_json::json!({
                    "available": true, "enabled": true,
                    "installUrl": "https://github.com/apps/codegraff/installations/new"
                })),
                cx,
            );
            page.connect_until = Some(Instant::now() + Duration::from_secs(60));
            page.schedule_poll(cx);
            assert!(page.poll_task.is_some(), "re-reads while waiting");
            page.set_runners(
                runners(serde_json::json!({
                    "available": true, "enabled": true,
                    "installations": [{"installationId": "1", "account": "me"}]
                })),
                cx,
            );
            assert_eq!(page.connect_until, None, "done waiting");
            assert!(page.poll_task.is_none());
        });
    }
}

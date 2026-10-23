//! In-memory checkout change-request state for desktop views.
//!
//! The workspace document remains the source of truth for chats and projects;
//! pull-request metadata is host-local, short-lived capability state and is
//! deliberately never written back into a synced document.

use std::collections::{HashMap, HashSet};

use gpui::{AnyElement, Context, Render, SharedString, Window, div, prelude::*, px};
use harness_proto::{
    ChangeRequestSummary, Chat, CheckoutChangeRequestStatus, ConversationSourceContext, Space,
};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeRequestBadgeTone {
    Open,
    Merged,
    Closed,
}

impl ChangeRequestBadgeTone {
    pub fn color(self, theme: &Theme) -> gpui::Hsla {
        match self {
            Self::Open => theme.success,
            Self::Merged => theme.code_text,
            Self::Closed => theme.danger,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeRequestBadgeModel {
    pub number: SharedString,
    pub state_label: &'static str,
    pub title: SharedString,
    pub tone: ChangeRequestBadgeTone,
}

impl ChangeRequestBadgeModel {
    pub fn from_summary(summary: &ChangeRequestSummary) -> Self {
        use harness_proto::ChangeRequestState;

        let (state_label, tone) = match summary.state {
            ChangeRequestState::Open => ("Open", ChangeRequestBadgeTone::Open),
            ChangeRequestState::Merged => ("Merged", ChangeRequestBadgeTone::Merged),
            ChangeRequestState::Closed => ("Closed", ChangeRequestBadgeTone::Closed),
        };
        Self {
            number: format!("#{}", summary.number).into(),
            state_label,
            title: summary.title.replace(['\r', '\n'], " ").into(),
            tone,
        }
    }
}

/// How a branch's PR relates to a chat (#357). A PR is found from the
/// chat's branch, not recorded against the chat, so it is the chat's own
/// only when nobody else works on that branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeRequestProvenance {
    /// The chat works in its own checkout: not the project's main folder,
    /// and no other chat uses it. Its branch's PR is the chat's.
    OwnBranch,
    /// Found from the branch of a checkout that is the project's main
    /// folder or that `other_chats` other chats share: the branch's PR,
    /// which this chat did not necessarily open, update or merge.
    SharedBranch { other_chats: usize },
}

impl ChangeRequestProvenance {
    /// A merged or closed PR found from a shared branch is the branch's
    /// history, not this chat's status, so it doesn't take a state color.
    fn mutes(self, summary: &ChangeRequestSummary) -> bool {
        matches!(self, Self::SharedBranch { .. })
            && summary.state != harness_proto::ChangeRequestState::Open
    }

    fn note(self, summary: &ChangeRequestSummary) -> Option<String> {
        let Self::SharedBranch { other_chats } = self else {
            return None;
        };
        let mut note = format!("Found from branch {}", summary.head_ref);
        match other_chats {
            0 => {}
            1 => note.push_str(", also used by 1 other chat"),
            n => note.push_str(&format!(", also used by {n} other chats")),
        }
        note.push_str(
            if summary.state == harness_proto::ChangeRequestState::Open {
                ". Not necessarily this chat's."
            } else {
                ". Not necessarily from this chat."
            },
        );
        Some(note)
    }
}

/// See [`ChangeRequestProvenance`]. A chat without a current source context
/// or a project to compare against counts as shared: ownership has to be
/// shown, not assumed.
pub(crate) fn change_request_provenance(
    chat: &Chat,
    chats: &[Chat],
    spaces: &[Space],
) -> ChangeRequestProvenance {
    let checkout = current_source_context(chat).map(|source| source.checkout_id.as_str());
    let other_chats = checkout.map_or(0, |checkout| {
        chats
            .iter()
            .filter(|other| other.id != chat.id && !other.archived)
            .filter(|other| other.device_id == chat.device_id)
            .filter(|other| {
                current_source_context(other).is_some_and(|source| source.checkout_id == checkout)
            })
            .count()
    });
    let project_checkout = chat
        .space_id
        .as_deref()
        .and_then(|id| spaces.iter().find(|space| space.id == id))
        .map(|space| space.checkout_id.as_deref());
    match (checkout, project_checkout) {
        (Some(checkout), Some(main)) if other_chats == 0 && main != Some(checkout) => {
            ChangeRequestProvenance::OwnBranch
        }
        _ => ChangeRequestProvenance::SharedBranch { other_chats },
    }
}

pub(crate) struct ChangeRequestTooltip {
    model: ChangeRequestBadgeModel,
    note: Option<String>,
}

impl ChangeRequestTooltip {
    pub fn new(summary: &ChangeRequestSummary, provenance: ChangeRequestProvenance) -> Self {
        Self {
            model: ChangeRequestBadgeModel::from_summary(summary),
            note: provenance.note(summary),
        }
    }
}

impl Render for ChangeRequestTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        let card = div()
            .max_w(px(320.0))
            .px(px(9.0))
            .py(px(7.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(crate::popover::surface_bg(theme))
            .shadow_md()
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(self.model.tone.color(theme))
                    .child(SharedString::from(format!(
                        "PR {} · {}",
                        self.model.number, self.model.state_label
                    ))),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .whitespace_nowrap()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(self.model.title.clone()),
            )
            .when_some(self.note.clone(), |card, note| {
                card.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted.opacity(0.8))
                        .child(SharedString::from(note)),
                )
            });
        crate::frost::frosted(6.0, crate::frost::MENU_BLUR, card)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeRequestBadgeSurface {
    Sidebar,
    Composer,
}

pub(crate) fn pull_request_badge(
    id: SharedString,
    summary: ChangeRequestSummary,
    provenance: ChangeRequestProvenance,
    surface: ChangeRequestBadgeSurface,
    theme: &Theme,
) -> AnyElement {
    pull_request_badge_with_query(id, summary, provenance, surface, None, theme)
}

pub(crate) fn pull_request_badge_with_query(
    id: SharedString,
    summary: ChangeRequestSummary,
    provenance: ChangeRequestProvenance,
    surface: ChangeRequestBadgeSurface,
    query: Option<&str>,
    theme: &Theme,
) -> AnyElement {
    render_pull_request_badge(id, summary, provenance, surface, query, true, theme)
}

/// The same badge geometry without hover, tooltip, or click behavior in drag previews.
pub(crate) fn pull_request_badge_preview(
    id: SharedString,
    summary: ChangeRequestSummary,
    provenance: ChangeRequestProvenance,
    surface: ChangeRequestBadgeSurface,
    theme: &Theme,
) -> AnyElement {
    render_pull_request_badge(id, summary, provenance, surface, None, false, theme)
}

fn render_pull_request_badge(
    id: SharedString,
    summary: ChangeRequestSummary,
    provenance: ChangeRequestProvenance,
    surface: ChangeRequestBadgeSurface,
    query: Option<&str>,
    interactive: bool,
    theme: &Theme,
) -> AnyElement {
    let model = ChangeRequestBadgeModel::from_summary(&summary);
    let color = if provenance.mutes(&summary) {
        theme.text_muted
    } else {
        model.tone.color(theme)
    };
    let url = summary.url.clone();
    let tooltip_summary = summary;
    let composer = surface == ChangeRequestBadgeSurface::Composer;

    div()
        .id(id)
        .h(px(if composer { 20.0 } else { 16.0 }))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(if composer { 5.0 } else { 0.0 }))
        .px(px(if composer { 7.0 } else { 4.0 }))
        .rounded(px(if composer { 6.0 } else { 4.0 }))
        .bg(color.opacity(0.08))
        .text_size(px(if composer { 11.0 } else { 10.0 }))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(color.opacity(0.85))
        .when(interactive, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(color.opacity(0.16)).text_color(color))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&url);
                })
                .tooltip(move |_, cx| {
                    cx.new(|_| ChangeRequestTooltip::new(&tooltip_summary, provenance))
                        .into()
                })
                .tooltip_show_delay(std::time::Duration::from_millis(350))
        })
        .when(composer, |element| {
            element.child(
                crate::icons::icon(crate::icons::PULL_REQUEST)
                    .size(px(11.0))
                    .flex_none()
                    .text_color(color.opacity(0.85)),
            )
        })
        // Monospace digits give the badge a stable tabular width as PR numbers change.
        .child(
            div()
                .font_family(theme.font_mono.clone())
                .child(crate::popover::search_highlight(model.number, query, theme)),
        )
        .into_any_element()
}

/// One host-side checkout watch. Multiple chats can share this target.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ChangeRequestWatchKey {
    pub device_id: String,
    pub cwd: String,
    pub branch: String,
    pub checkout_id: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ChangeRequestClientState {
    snapshots: HashMap<ChangeRequestWatchKey, CheckoutChangeRequestStatus>,
    /// The engine version that rejected this versioned capability. A device
    /// update publishes its running version, so a host upgrade invalidates this
    /// negative cache without polling older hosts.
    unsupported_devices: HashMap<String, Option<String>>,
}

impl ChangeRequestClientState {
    pub fn is_supported(&self, device_id: &str) -> bool {
        !self.unsupported_devices.contains_key(device_id)
    }

    pub fn mark_unsupported(&mut self, device_id: String, engine_version: Option<String>) {
        self.unsupported_devices
            .insert(device_id.clone(), engine_version);
        self.snapshots.retain(|key, _| key.device_id != device_id);
    }

    /// Forget a version-skew rejection after the host advertises a different
    /// engine version. `UnknownMethod` describes one running engine, not the
    /// device permanently.
    pub fn clear_unsupported_on_version_change(
        &mut self,
        device_id: &str,
        engine_version: Option<&str>,
    ) -> bool {
        let Some(unsupported_version) = self.unsupported_devices.get(device_id) else {
            return false;
        };
        if unsupported_version.as_deref() == engine_version {
            return false;
        }
        self.unsupported_devices.remove(device_id);
        true
    }

    pub fn store(&mut self, key: ChangeRequestWatchKey, snapshot: CheckoutChangeRequestStatus) {
        // A new frame replaces the old branch/checkout context for this path.
        // Rendering validates the complete identity again before exposing it.
        self.snapshots.insert(key, snapshot);
    }

    pub fn retain_targets(&mut self, targets: &HashSet<ChangeRequestWatchKey>) {
        self.snapshots.retain(|key, _| targets.contains(key));
    }

    pub fn change_request_for_chat<'a>(
        &'a self,
        chat: &Chat,
        spaces: &[Space],
    ) -> Option<&'a ChangeRequestSummary> {
        change_request_for_chat(chat, spaces, self.snapshots.values())
    }
}

/// Active, fully identified checkouts that need host-side PR resolution.
pub(crate) fn desired_watch_targets(
    chats: &[Chat],
    _spaces: &[Space],
    is_unsupported: impl Fn(&str) -> bool,
) -> HashSet<ChangeRequestWatchKey> {
    chats
        .iter()
        .filter(|chat| !chat.archived)
        .filter(|chat| !is_unsupported(&chat.device_id))
        .filter_map(|chat| {
            let source = current_source_context(chat)?;
            let (cwd, branch, checkout_id) = (
                source.repo_root.as_str(),
                source.branch.as_str(),
                Some(source.checkout_id.clone()),
            );
            if branch.trim().is_empty() {
                return None;
            }
            Some(ChangeRequestWatchKey {
                device_id: chat.device_id.clone(),
                cwd: cwd.to_owned(),
                branch: branch.to_owned(),
                checkout_id,
            })
        })
        .collect()
}

pub(crate) fn watch_params(
    target: &ChangeRequestWatchKey,
    local_device_id: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::json!({
        "cwd": target.cwd,
        "branch": target.branch,
    });
    if local_device_id != Some(target.device_id.as_str())
        && let Some(object) = params.as_object_mut()
    {
        object.insert(
            "targetDeviceId".into(),
            serde_json::Value::String(target.device_id.clone()),
        );
    }
    params
}

/// `MergeChangeRequest` params: the checkout and branch this chat's badge
/// watches, routed to the chat's device. The host re-resolves the branch and
/// merges only while `number` is still its open pull request.
pub(crate) fn merge_params(
    chat: &Chat,
    number: u64,
    local_device_id: Option<&str>,
) -> Option<serde_json::Value> {
    let source = chat.source_context.as_ref()?;
    if source.branch.trim().is_empty() {
        return None;
    }
    let target = ChangeRequestWatchKey {
        device_id: chat.device_id.clone(),
        cwd: source.repo_root.clone(),
        branch: source.branch.clone(),
        checkout_id: None,
    };
    let mut params = watch_params(&target, local_device_id);
    params["number"] = number.into();
    Some(params)
}

/// Resolve a snapshot for a row without trusting the cache key alone.
///
/// Only conversation-owned source context is trusted. Legacy scalar metadata
/// cannot prove that a worktree has not switched branches since it was written.
pub fn change_request_for_chat<'a>(
    chat: &Chat,
    spaces: &[Space],
    snapshots: impl IntoIterator<Item = &'a CheckoutChangeRequestStatus>,
) -> Option<&'a ChangeRequestSummary> {
    let branch = conversation_branch(chat, spaces)?.trim();
    if branch.is_empty() {
        return None;
    }
    let source = current_source_context(chat)?;
    let cwd = source.repo_root.as_str();
    let checkout_id = Some(source.checkout_id.as_str());

    snapshots
        .into_iter()
        .find(|snapshot| {
            snapshot.device_id == chat.device_id
                && snapshot.cwd == cwd
                && snapshot.branch == branch
                && checkout_id.is_none_or(|checkout_id| {
                    !snapshot.checkout_id.is_empty() && snapshot.checkout_id == checkout_id
                })
        })
        .and_then(|snapshot| snapshot.change_request.as_ref())
}

/// Branch metadata safe to render for this conversation. Pre-source-context
/// rows are intentionally rejected: their scalar branch may describe an old
/// state of either a shared checkout or a worktree.
pub(crate) fn conversation_branch<'a>(chat: &'a Chat, _spaces: &'a [Space]) -> Option<&'a str> {
    current_source_context(chat)
        .map(|source| source.branch.as_str())
        .filter(|branch| !branch.trim().is_empty())
}

/// The chat's source context when it describes where the chat runs now
/// (#356). Not when the latest accepted run could not observe its checkout,
/// and not after the chat was moved to another folder that no run has
/// observed yet: the last known context then belongs to the old place.
pub(crate) fn current_source_context(chat: &Chat) -> Option<&ConversationSourceContext> {
    let source = chat.source_context.as_ref()?;
    if chat.source_context_unavailable_at.is_some() {
        return None;
    }
    let same_folder =
        |cwd: &str| cwd.trim_end_matches(['/', '\\']) == source.cwd.trim_end_matches(['/', '\\']);
    if chat.cwd.as_deref().is_some_and(|cwd| !same_folder(cwd)) {
        return None;
    }
    Some(source)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};
    use harness_proto::ChangeRequestState;

    use super::*;

    fn chat(id: &str, device: &str, cwd: Option<&str>, checkout: Option<&str>) -> Chat {
        Chat {
            last_prompt_at: None,
            id: id.into(),
            device_id: device.into(),
            title: None,
            archived: false,
            cwd: cwd.map(str::to_owned),
            branch: Some("feature/pr".into()),
            checkout_id: checkout.map(str::to_owned),
            source_context: None,
            config: None,
            last_message_preview: None,
            last_message_at: None,
            created_at: Utc.timestamp_opt(0, 0).unwrap(),
            harness_session_id: None,
            harness_session_cwd: None,
            parent_chat_id: None,
            space_id: Some("space".into()),
            last_seen_at: None,
            room_gen: None,
            source_context_unavailable_at: None,
        }
    }

    fn space(device: &str) -> Space {
        Space {
            id: "space".into(),
            device_id: device.into(),
            path: "/project".into(),
            name: None,
            git_detected: true,
            git_checked_at: None,
            checkout_id: Some("checkout".into()),
            created_at: Utc.timestamp_opt(0, 0).unwrap(),
        }
    }

    fn snapshot(device: &str, cwd: &str, checkout: &str) -> CheckoutChangeRequestStatus {
        CheckoutChangeRequestStatus {
            checkout_id: checkout.into(),
            device_id: device.into(),
            cwd: cwd.into(),
            branch: "feature/pr".into(),
            change_request: Some(ChangeRequestSummary {
                provider: "github".into(),
                number: 90,
                title: "Add pull request badges".into(),
                url: "https://github.com/acme/harness/pull/90".into(),
                state: ChangeRequestState::Open,
                base_ref: "main".into(),
                head_ref: "feature/pr".into(),
            }),
            updated_at: Utc.timestamp_opt(1, 0).unwrap(),
        }
    }

    fn with_source(mut chat: Chat, branch: &str) -> Chat {
        chat.source_context = Some(harness_proto::ConversationSourceContext {
            checkout_id: "checkout".into(),
            repo_root: "/repo".into(),
            cwd: "/repo".into(),
            branch: branch.into(),
            head_sha: Some("abc123".into()),
            observed_at: Utc.timestamp_opt(2, 0).unwrap(),
        });
        chat
    }

    /// #357: a branch's PR is a chat's own only when the chat works in its
    /// own checkout; otherwise it is labelled as found from the branch, and
    /// merged or closed it is shown without a state color.
    #[test]
    fn a_branch_pull_request_is_a_chats_own_only_in_its_own_checkout() {
        use ChangeRequestProvenance::{OwnBranch, SharedBranch};
        let spaces = [space("local")];
        let in_worktree = |id: &str, checkout: &str| {
            let mut chat = with_source(chat(id, "local", Some("/repo"), None), "feature/pr");
            chat.source_context.as_mut().unwrap().checkout_id = checkout.into();
            chat
        };
        let own = in_worktree("own", "worktree-1");
        assert_eq!(
            change_request_provenance(&own, &[own.clone()], &spaces),
            OwnBranch
        );

        let peer = in_worktree("peer", "worktree-1");
        let chats = [own.clone(), peer];
        assert_eq!(
            change_request_provenance(&own, &chats, &spaces),
            SharedBranch { other_chats: 1 },
            "two chats in one checkout share its branch"
        );

        let main = in_worktree("main", "checkout");
        assert_eq!(
            change_request_provenance(&main, &[main.clone()], &spaces),
            SharedBranch { other_chats: 0 },
            "the project's own folder is shared by whoever checks a branch out there"
        );
        assert_eq!(
            change_request_provenance(&own, &[own.clone()], &[]),
            SharedBranch { other_chats: 0 },
            "no project to compare against: not assumed to be the chat's"
        );

        let mut merged = snapshot("local", "/repo", "checkout")
            .change_request
            .unwrap();
        merged.state = ChangeRequestState::Merged;
        let open = snapshot("local", "/repo", "checkout")
            .change_request
            .unwrap();
        let shared = SharedBranch { other_chats: 2 };
        assert!(shared.mutes(&merged));
        assert!(!shared.mutes(&open));
        assert!(!OwnBranch.mutes(&merged));
        assert_eq!(OwnBranch.note(&merged), None);
        assert_eq!(
            shared.note(&merged).as_deref(),
            Some(
                "Found from branch feature/pr, also used by 2 other chats. Not necessarily from this chat."
            )
        );
        assert_eq!(
            SharedBranch { other_chats: 0 }.note(&open).as_deref(),
            Some("Found from branch feature/pr. Not necessarily this chat's.")
        );
    }

    /// #356: a context the latest run could not confirm, or one left behind
    /// by a move to another folder, is history, not where the chat runs.
    #[test]
    fn only_a_current_source_context_finds_a_pull_request() {
        let current = with_source(chat("chat", "local", Some("/repo"), None), "feature/pr");
        let snapshot = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&current, &[], [&snapshot]).is_some());

        let mut unavailable = current.clone();
        unavailable.source_context_unavailable_at = Some(Utc.timestamp_opt(3, 0).unwrap());
        assert!(change_request_for_chat(&unavailable, &[], [&snapshot]).is_none());
        assert_eq!(conversation_branch(&unavailable, &[]), None);
        assert!(desired_watch_targets(&[unavailable], &[], |_| false).is_empty());

        let mut moved = current.clone();
        moved.cwd = Some("/elsewhere".into());
        assert!(change_request_for_chat(&moved, &[], [&snapshot]).is_none());
        assert!(desired_watch_targets(&[moved], &[], |_| false).is_empty());

        let mut trailing = current;
        trailing.cwd = Some("/repo/".into());
        assert!(
            change_request_for_chat(&trailing, &[], [&snapshot]).is_some(),
            "the same folder spelled with a trailing slash"
        );
    }

    #[test]
    fn shared_checkout_keeps_conversation_branches_independent() {
        let first = with_source(chat("first", "local", Some("/repo"), None), "feature/one");
        let second = with_source(chat("second", "local", Some("/repo"), None), "feature/two");
        let targets = desired_watch_targets(&[first.clone(), second.clone()], &[], |_| false);
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|target| target.branch == "feature/one"));
        assert!(targets.iter().any(|target| target.branch == "feature/two"));

        let mut first_snapshot = snapshot("local", "/repo", "checkout");
        first_snapshot.branch = "feature/one".into();
        assert_eq!(
            change_request_for_chat(&first, &[], [&first_snapshot]).map(|pr| pr.number),
            Some(90)
        );
        assert!(change_request_for_chat(&second, &[], [&first_snapshot]).is_none());
    }

    #[test]
    fn merge_targets_the_badge_checkout_and_routes_to_the_chat_device() {
        let local = with_source(chat("chat", "local", Some("/repo/wt"), None), "feature/pr");
        assert_eq!(
            merge_params(&local, 90, Some("local")),
            Some(serde_json::json!({ "cwd": "/repo", "branch": "feature/pr", "number": 90 }))
        );
        let remote = with_source(chat("chat", "remote", Some("/repo"), None), "feature/pr");
        assert_eq!(
            merge_params(&remote, 90, Some("local")).unwrap()["targetDeviceId"],
            "remote"
        );
        assert!(
            merge_params(
                &chat("chat", "local", Some("/repo"), None),
                90,
                Some("local")
            )
            .is_none()
        );
    }

    #[test]
    fn source_context_chat_finds_local_snapshot() {
        let chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let status = snapshot("local", "/repo", "checkout");
        assert_eq!(
            change_request_for_chat(&chat, &[], [&status]).map(|pr| pr.number),
            Some(90)
        );
    }

    #[test]
    fn remote_chat_requires_the_same_device() {
        let chat = with_source(
            chat("chat", "remote", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let status = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
    }

    #[test]
    fn mismatched_checkout_rejects_cwd_match() {
        let mut chat = with_source(
            chat("chat", "local", Some("/repo"), Some("new-checkout")),
            "feature/pr",
        );
        chat.source_context.as_mut().unwrap().checkout_id = "new-checkout".into();
        let status = snapshot("local", "/repo", "old-checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
    }

    #[test]
    fn source_less_worktree_hides_stale_legacy_metadata() {
        let chat = chat("chat", "local", Some("/repo"), None);
        let status = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
        assert!(conversation_branch(&chat, &[]).is_none());
        assert!(desired_watch_targets(&[chat], &[], |_| false).is_empty());
    }

    #[test]
    fn source_less_project_root_hides_legacy_branch_metadata() {
        let chat = chat("chat", "local", None, None);
        let status = snapshot("local", "/project", "checkout");
        assert!(change_request_for_chat(&chat, &[space("local")], [&status]).is_none());
        assert!(desired_watch_targets(&[chat], &[space("local")], |_| false).is_empty());
    }

    #[test]
    fn different_branch_hides_snapshot() {
        let mut chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/other",
        );
        chat.branch = Some("feature/pr".into());
        let status = snapshot("local", "/repo", "checkout");
        assert!(change_request_for_chat(&chat, &[], [&status]).is_none());
    }

    #[test]
    fn shared_chats_deduplicate_and_last_archive_removes_target() {
        let first = with_source(
            chat("one", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut second = with_source(
            chat("two", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let targets = desired_watch_targets(&[first.clone(), second.clone()], &[], |_| false);
        assert_eq!(targets.len(), 1);

        second.archived = true;
        let targets = desired_watch_targets(&[first.clone(), second.clone()], &[], |_| false);
        assert_eq!(targets.len(), 1);

        let mut first = first;
        first.archived = true;
        assert!(desired_watch_targets(&[first, second], &[], |_| false).is_empty());
    }

    #[test]
    fn unsupported_device_has_no_targets_or_visible_snapshot() {
        let chat = with_source(
            chat("chat", "old-engine", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut state = ChangeRequestClientState::default();
        state.store(
            ChangeRequestWatchKey {
                device_id: "old-engine".into(),
                cwd: "/repo".into(),
                branch: "feature/pr".into(),
                checkout_id: Some("checkout".into()),
            },
            snapshot("old-engine", "/repo", "checkout"),
        );
        state.mark_unsupported("old-engine".into(), Some("0.2.2".into()));

        assert!(state.change_request_for_chat(&chat, &[]).is_none());
        assert!(
            desired_watch_targets(&[chat], &[], |device| !state.is_supported(device)).is_empty()
        );
    }

    #[test]
    fn host_upgrade_reenables_a_previously_unsupported_device() {
        let chat = with_source(
            chat("chat", "host", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let mut state = ChangeRequestClientState::default();
        state.mark_unsupported("host".into(), Some("0.2.2".into()));

        assert!(!state.is_supported("host"));
        assert!(!state.clear_unsupported_on_version_change("host", Some("0.2.2")));
        assert!(state.clear_unsupported_on_version_change("host", Some("0.2.3")));
        assert!(state.is_supported("host"));
        assert_eq!(
            desired_watch_targets(&[chat], &[], |device| !state.is_supported(device)).len(),
            1
        );
    }

    #[test]
    fn successful_none_clears_a_previous_pr() {
        let chat = with_source(
            chat("chat", "local", Some("/repo"), Some("checkout")),
            "feature/pr",
        );
        let key = ChangeRequestWatchKey {
            device_id: "local".into(),
            cwd: "/repo".into(),
            branch: "feature/pr".into(),
            checkout_id: Some("checkout".into()),
        };
        let mut state = ChangeRequestClientState::default();
        state.store(key.clone(), snapshot("local", "/repo", "checkout"));
        assert!(state.change_request_for_chat(&chat, &[]).is_some());

        let mut none = snapshot("local", "/repo", "checkout");
        none.change_request = None;
        state.store(key, none);
        assert!(state.change_request_for_chat(&chat, &[]).is_none());
    }

    #[test]
    fn local_and_remote_watch_params_route_to_the_checkout_host() {
        let target = ChangeRequestWatchKey {
            device_id: "host".into(),
            cwd: "/repo".into(),
            branch: "feature/pr".into(),
            checkout_id: Some("checkout".into()),
        };
        assert_eq!(
            watch_params(&target, Some("host")),
            serde_json::json!({
                "cwd": "/repo",
                "branch": "feature/pr"
            })
        );
        assert_eq!(
            watch_params(&target, Some("viewport")),
            serde_json::json!({
                "cwd": "/repo",
                "branch": "feature/pr",
                "targetDeviceId": "host"
            })
        );
    }

    #[test]
    fn badge_models_cover_open_merged_and_closed() {
        let cases = [
            (
                ChangeRequestState::Open,
                "Open",
                ChangeRequestBadgeTone::Open,
            ),
            (
                ChangeRequestState::Merged,
                "Merged",
                ChangeRequestBadgeTone::Merged,
            ),
            (
                ChangeRequestState::Closed,
                "Closed",
                ChangeRequestBadgeTone::Closed,
            ),
        ];
        for (state, label, tone) in cases {
            let mut summary = snapshot("local", "/repo", "checkout")
                .change_request
                .unwrap();
            summary.state = state;
            summary.title = "First line\nSecond line".into();
            let model = ChangeRequestBadgeModel::from_summary(&summary);
            assert_eq!(model.number, "#90");
            assert_eq!(model.state_label, label);
            assert_eq!(model.tone, tone);
            assert_eq!(model.title, "First line Second line");
        }
    }
}

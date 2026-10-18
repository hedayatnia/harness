//! Chat mode: a plain conversation instead of a coding session. No project,
//! graff on the hosted gateway's auto tiers only, offered as Auto (low),
//! Auto (high) and Auto (smart). Pure, so the picker rules are tested
//! without UI.

use gpui::{AnyElement, SharedString, div, prelude::*};
use harness_proto::{HarnessId, Model};

use crate::theme::Theme;

/// Chat mode always runs on graff, which reaches the hosted gateway.
pub const HARNESS: HarnessId = HarnessId::Graff;

/// One model chat mode offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    /// The picker id graff lists (`<provider>/<model>`).
    pub id: &'static str,
    /// Older gateways list the same tier under this id (`auto-medium` before
    /// `auto-low` existed).
    pub fallback_id: Option<&'static str>,
    pub label: &'static str,
    /// Shown under the row, and in the footer while it is selected.
    pub note: Option<&'static str>,
}

pub const TIERS: [Tier; 3] = [
    Tier {
        id: "codegraff/auto-low",
        fallback_id: Some("codegraff/auto-medium"),
        label: "Auto (low)",
        note: None,
    },
    Tier {
        id: "codegraff/auto-high",
        fallback_id: None,
        label: "Auto (high)",
        note: None,
    },
    Tier {
        id: "codegraff/auto-smart",
        fallback_id: None,
        label: "Auto (smart)",
        note: Some(SMART_COST_NOTE),
    },
];

/// Switching to Auto (smart) bills Gemini 4 Argon at API rates.
pub const SMART_COST_NOTE: &str = "Uses Gemini 4 Argon and may incur API costs.";

/// The chat-mode rows graff's catalog offers, in tier order, under the tier
/// names. A tier the gateway doesn't list yet is left out.
pub fn models(catalog: &[Model]) -> Vec<Model> {
    TIERS
        .iter()
        .filter_map(|tier| {
            let row = catalog.iter().find(|m| m.id == tier.id).or_else(|| {
                tier.fallback_id
                    .and_then(|id| catalog.iter().find(|m| m.id == id))
            })?;
            Some(Model {
                label: tier.label.into(),
                description: tier.note.map(str::to_owned),
                ..row.clone()
            })
        })
        .collect()
}

/// The model a new chat starts on: the first tier on offer (Auto (low)).
pub fn default_model(catalog: &[Model]) -> Option<String> {
    default_id(catalog).map(str::to_owned)
}

/// [`default_model`], borrowed from the catalog row.
pub fn default_id(catalog: &[Model]) -> Option<&str> {
    TIERS.iter().find_map(|tier| {
        catalog
            .iter()
            .find(|m| m.id == tier.id)
            .or_else(|| {
                tier.fallback_id
                    .and_then(|id| catalog.iter().find(|m| m.id == id))
            })
            .map(|m| m.id.as_str())
    })
}

/// Whether `id` is one of the models chat mode offers.
pub fn offers(id: &str) -> bool {
    TIERS
        .iter()
        .any(|tier| tier.id == id || tier.fallback_id == Some(id))
}

/// The note to show while `id` is selected (the API-cost warning for Auto (smart)).
pub fn note_for(id: Option<&str>) -> Option<&'static str> {
    let id = id?;
    TIERS
        .iter()
        .find(|tier| tier.id == id || tier.fallback_id == Some(id))
        .and_then(|tier| tier.note)
}

/// The composer's note while a model with one is selected.
pub fn note_chip(note: &'static str, theme: &Theme) -> AnyElement {
    div()
        .min_w_0()
        .truncate()
        .text_size(crate::typography::ui_rems(11.5))
        .text_color(theme.text_muted)
        .debug_selector(|| "chat-mode-note".into())
        .child(SharedString::from(note))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, label: &str) -> Model {
        Model {
            id: id.into(),
            label: label.into(),
            description: None,
            reasoning_levels: vec![],
            options: vec![],
            maker: None,
            billing: None,
        }
    }

    #[test]
    fn only_the_auto_tiers_are_offered_in_order_under_their_names() {
        let catalog = [
            model("codegraff/auto-smart", "auto-smart"),
            model("codegraff/claude-opus-5-5", "Claude Opus 5.5"),
            model("codegraff/auto-high", "Auto (high)"),
            model("chatgpt-new/gpt-6.1-sol", "GPT-6.1 Sol"),
            model("codegraff/auto-low", "Auto (low)"),
        ];
        let offered = models(&catalog);
        assert_eq!(
            offered
                .iter()
                .map(|m| (m.id.as_str(), m.label.as_str()))
                .collect::<Vec<_>>(),
            [
                ("codegraff/auto-low", "Auto (low)"),
                ("codegraff/auto-high", "Auto (high)"),
                ("codegraff/auto-smart", "Auto (smart)"),
            ]
        );
        assert_eq!(offered[2].description.as_deref(), Some(SMART_COST_NOTE));
        assert_eq!(
            default_model(&catalog).as_deref(),
            Some("codegraff/auto-low")
        );
    }

    #[test]
    fn an_older_gateway_offers_auto_medium_as_auto_low_and_omits_what_it_lacks() {
        let catalog = [
            model("codegraff/auto-medium", "Auto (medium)"),
            model("codegraff/auto-high", "Auto (high)"),
        ];
        let offered = models(&catalog);
        assert_eq!(
            offered
                .iter()
                .map(|m| (m.id.as_str(), m.label.as_str()))
                .collect::<Vec<_>>(),
            [
                ("codegraff/auto-medium", "Auto (low)"),
                ("codegraff/auto-high", "Auto (high)"),
            ]
        );
        assert!(models(&[]).is_empty());
        assert_eq!(default_model(&[]), None);
    }

    #[test]
    fn the_cost_note_belongs_to_auto_smart_only() {
        assert_eq!(
            note_for(Some("codegraff/auto-smart")),
            Some(SMART_COST_NOTE)
        );
        assert_eq!(note_for(Some("codegraff/auto-high")), None);
        assert_eq!(note_for(Some("codegraff/auto-medium")), None);
        assert_eq!(note_for(None), None);
        assert!(offers("codegraff/auto-medium"));
        assert!(!offers("codegraff/claude-opus-5-5"));
    }
}

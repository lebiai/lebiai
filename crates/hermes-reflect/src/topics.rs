//! Build topic cards: one LLM call that groups memories by **subject**.
//!
//! Two modes, one contract — the model always returns the complete card set:
//! - **merge** (default): the input is the memories no card points at yet, plus
//!   the existing cards. The model folds the new memories into those cards, or
//!   opens new ones.
//! - **rebuild**: the input is every active memory; the model is free to
//!   re-cut the themes (this is how a badly split card heals, and how a card
//!   that outgrew its summary gets split).
//!
//! Everything the model returns is validated against the memory store
//! ([`finalize_cards`]): unknown member ids are dropped, cards with no live
//! member disappear, and whatever the model left out lands on the 未归类 card —
//! so a build always covers every active memory, which is what makes
//! "has the card set fallen behind?" a clean question.

use std::collections::{HashMap, HashSet};

use hermes_core::{CompletionRequest, LlmProvider, Message};
use hermes_memory::topics::{TopicCard, TopicCards, MAX_CARDS, SPLIT_THRESHOLD, UNGROUPED_TITLE};
use hermes_memory::LoadedMemory;
use serde::Deserialize;

use crate::runner::{strip_code_fence_pub, ReflectError};

/// Reasoning models bill their thinking to the same budget, so this is set
/// well above the size of the answer we actually want (measured: 4096 was
/// consumed entirely by reasoning and returned MaxTokens with no text).
const CARDS_MAX_TOKENS: u32 = 16384;

const SYSTEM_PROMPT: &str = r#"You are the memory curator for lebi-AI, a local work companion. You group a
person's memory entries by SUBJECT and summarise each group.

The entries are the person's own words and they are the law. When an entry states a rule, a
red line, a number, a source, or a delivery requirement, carry its exact wording into the
summary. Never paraphrase a constraint, never soften a prohibition, never add a fact that is
not in the entries.

Rules:
- Group by subject (what the entries are about). Never group by how work is done.
- EVERY entry you are given must be covered by at least one card.
- An entry may appear on more than one card when it genuinely spans subjects.
- Title: short and concrete, in the person's own language (aim <= 12 characters in Chinese).
- Summary: at most 6 lines, one fact per line, no filler, no generic praise, no memory ids.
- At most 8 themed cards. Use the 未归类 card only for entries that fit no theme.
- If a card below already covers the same subject, reuse it: copy its id verbatim and return
  its complete member list (old members plus anything new).
- For a genuinely new card use "" as the id.
- Output ONLY the JSON object. No prose. No markdown fences.

{
  "cards": [
    {"id": "<existing id or empty>", "title": "<subject>", "summary": "<lines>", "members": ["mem_..."]}
  ]
}
"#;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CardDraft {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CardDraftList {
    #[serde(default)]
    cards: Vec<CardDraft>,
}

/// Group the active memories into topic cards.
///
/// `rebuild = false` only considers memories no card points at (cheap, keeps
/// card identity). `rebuild = true` re-cuts everything from the full set.
pub async fn build_topic_cards(
    provider: &dyn LlmProvider,
    active: &[LoadedMemory],
    existing: &TopicCards,
    rebuild: bool,
) -> Result<TopicCards, ReflectError> {
    if active.is_empty() {
        return Err(ReflectError::Provider("no memories to organise".into()));
    }

    let focused: Vec<&LoadedMemory> = if rebuild || existing.is_empty() {
        active.iter().collect()
    } else {
        hermes_memory::topics::unassigned(existing, active)
    };
    if focused.is_empty() {
        return Ok(existing.clone());
    }
    // Nothing new to fold in: keep the cards as they are rather than paying for
    // a call that can only reshuffle them.
    if !rebuild && focused.len() == active.len() && !existing.is_empty() {
        return Ok(existing.clone());
    }

    let req = CompletionRequest {
        model: String::new(),
        system: Some(SYSTEM_PROMPT.to_string()),
        messages: vec![Message::user_text(build_prompt(
            &focused, existing, active, rebuild,
        ))],
        tools: Vec::new(),
        max_tokens: CARDS_MAX_TOKENS,
        temperature: Some(0.2),
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| ReflectError::Provider(e.to_string()))?;

    let text = resp.text();
    if text.trim().is_empty() {
        // An empty answer is a provider problem, not a parse problem — say what
        // actually came back instead of "EOF while parsing".
        return Err(ReflectError::Provider(format!(
            "model returned no text (stop={:?}, in={} out={} tokens)",
            resp.stop_reason, resp.usage.input_tokens, resp.usage.output_tokens
        )));
    }
    let json_str = strip_code_fence_pub(&text);
    let drafts: CardDraftList =
        serde_json::from_str(json_str).map_err(|e| ReflectError::ParseFailed {
            error: e.to_string(),
            raw: text.clone(),
        })?;

    Ok(finalize_cards(drafts.cards, active, existing))
}

fn build_prompt(
    focused: &[&LoadedMemory],
    existing: &TopicCards,
    active: &[LoadedMemory],
    rebuild: bool,
) -> String {
    let mut buf = String::new();
    buf.push_str(&format!(
        "Active memories: {}. Mode: {}.\n\n",
        active.len(),
        if rebuild {
            "REBUILD — re-cut the themes from scratch"
        } else {
            "MERGE — extend the existing cards"
        }
    ));

    if !existing.is_empty() {
        buf.push_str("=== Existing cards (reuse the id when the subject still matches) ===\n");
        for c in &existing.cards {
            buf.push_str(&format!(
                "- id={} · {} · members: {}{}\n  summary: {}\n",
                c.id,
                c.title,
                c.members.join(", "),
                if c.members.len() > SPLIT_THRESHOLD {
                    format!(
                        "  ⚠ {} members — if this subject really divides, return it as two cards",
                        c.members.len()
                    )
                } else {
                    String::new()
                },
                c.summary.replace('\n', " · ")
            ));
        }
        buf.push('\n');
    }

    buf.push_str("=== Memories to organise (full text — this is the law) ===\n");
    for m in focused {
        let tags = if m.frontmatter.tags.is_empty() {
            String::new()
        } else {
            format!(" tags={}", m.frontmatter.tags.join(","))
        };
        buf.push_str(&format!("- id={}{}\n  {}\n", m.id(), tags, m.body.trim()));
    }

    buf.push_str("\nReturn the complete card set as JSON.\n");
    buf
}

/// Validate a model answer and turn it into the card set we will store.
///
/// Guarantees on return: every member id exists in `active`, no card is empty,
/// ids are unique, and every active memory is referenced (leftovers land on
/// 未归类).
pub(crate) fn finalize_cards(
    drafts: Vec<CardDraft>,
    active: &[LoadedMemory],
    existing: &TopicCards,
) -> TopicCards {
    let known: HashSet<&str> = active.iter().map(|m| m.id()).collect();
    let by_id: HashMap<&str, &TopicCard> =
        existing.cards.iter().map(|c| (c.id.as_str(), c)).collect();
    let now = chrono::Utc::now().to_rfc3339();

    let mut out: Vec<TopicCard> = Vec::new();
    for draft in drafts {
        let members = sanitize_members(draft.members, &known);
        if members.is_empty() {
            continue;
        }
        let title = {
            let t = draft.title.trim();
            if t.is_empty() {
                UNGROUPED_TITLE.to_string()
            } else {
                t.to_string()
            }
        };
        let summary = draft.summary.trim().to_string();

        if let Some(pos) = out
            .iter()
            .position(|c| c.id == draft.id && !draft.id.is_empty())
        {
            for id in members {
                if !out[pos].members.contains(&id) {
                    out[pos].members.push(id);
                }
            }
            continue;
        }
        // Only an id the store actually knows is inherited, so a hallucinated
        // id can never displace a real card.
        let inherited = by_id.get(draft.id.as_str());
        out.push(TopicCard {
            id: inherited.map(|c| c.id.clone()).unwrap_or_else(new_card_id),
            title,
            summary,
            members,
            built_at: inherited
                .map(|c| c.built_at.clone())
                .unwrap_or_else(|| now.clone()),
        });
    }

    out.sort_by_key(|c| std::cmp::Reverse(c.members.len()));
    inherit_ids_by_overlap(&mut out, existing);
    out.truncate(MAX_CARDS);

    // Anything the model forgot (or dropped when we capped the list) still has
    // to be reachable — otherwise "is it stale?" would answer yes forever.
    let referenced: HashSet<String> = out.iter().flat_map(|c| c.members.iter().cloned()).collect();
    let leftovers: Vec<String> = active
        .iter()
        .map(|m| m.id().to_string())
        .filter(|id| !referenced.contains(id))
        .collect();
    if !leftovers.is_empty() {
        match out.iter_mut().find(|c| c.title == UNGROUPED_TITLE) {
            Some(card) => card.members.extend(leftovers),
            None => out.push(TopicCard {
                id: new_card_id(),
                title: UNGROUPED_TITLE.to_string(),
                summary: "这些记忆暂时没有归入某个主题。".to_string(),
                members: leftovers,
                built_at: now,
            }),
        }
    }

    TopicCards { cards: out }
}

fn sanitize_members(members: Vec<String>, known: &HashSet<&str>) -> Vec<String> {
    let mut seen = HashSet::new();
    members
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| known.contains(m.as_str()))
        .filter(|m| seen.insert(m.clone()))
        .collect()
}

/// A rebuild re-words the themes, so the model often opens fresh ids for cards
/// that are still the same card. Keep the id when the membership clearly still
/// matches, so the list does not look like it was replaced wholesale.
fn inherit_ids_by_overlap(out: &mut [TopicCard], existing: &TopicCards) {
    let mut taken: HashSet<String> = out.iter().map(|c| c.id.clone()).collect();
    let known: HashSet<&str> = existing.cards.iter().map(|c| c.id.as_str()).collect();
    for card in out.iter_mut() {
        if known.contains(card.id.as_str()) {
            continue;
        }
        let current: HashSet<&str> = card.members.iter().map(String::as_str).collect();
        let best = existing
            .cards
            .iter()
            .filter(|old| !taken.contains(&old.id) || old.id == card.id)
            .map(|old| {
                let old_set: HashSet<&str> = old.members.iter().map(String::as_str).collect();
                let overlap = current.intersection(&old_set).count();
                let ratio = overlap as f64 / current.len().max(old_set.len()).max(1) as f64;
                (old, ratio)
            })
            .filter(|(_, ratio)| *ratio >= 0.5)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((old, _)) = best {
            taken.remove(&card.id);
            card.id = old.id.clone();
            card.built_at = old.built_at.clone();
            taken.insert(card.id.clone());
        }
    }
}

fn new_card_id() -> String {
    format!("topic_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_memory::{Confidence, MemoryFrontmatter, Scope, Source};
    use std::path::PathBuf;

    fn mem(id: &str, body: &str) -> LoadedMemory {
        let mut fm = MemoryFrontmatter::new(
            Source::User,
            Confidence::High,
            vec![],
            "general".to_string(),
        );
        fm.id = id.to_string();
        LoadedMemory {
            frontmatter: fm,
            body: body.to_string(),
            source_path: PathBuf::from("/dev/null"),
            scope: Scope::User,
        }
    }

    fn draft(id: &str, title: &str, members: &[&str]) -> CardDraft {
        CardDraft {
            id: id.into(),
            title: title.into(),
            summary: "s".into(),
            members: members.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn active() -> Vec<LoadedMemory> {
        vec![
            mem("mem_a", "财经口径"),
            mem("mem_b", "公众号思路"),
            mem("mem_c", "具身智能口径"),
        ]
    }

    #[test]
    fn hallucinated_member_ids_are_dropped() {
        let cards = finalize_cards(
            vec![draft("", "财经", &["mem_a", "mem_does_not_exist"])],
            &active(),
            &TopicCards::default(),
        );
        let finance = cards.cards.iter().find(|c| c.title == "财经").unwrap();
        assert_eq!(finance.members, vec!["mem_a"]);
    }

    #[test]
    fn forgotten_memories_land_on_the_ungrouped_card() {
        let cards = finalize_cards(
            vec![draft("", "财经", &["mem_a"])],
            &active(),
            &TopicCards::default(),
        );
        let ungrouped = cards
            .cards
            .iter()
            .find(|c| c.title == UNGROUPED_TITLE)
            .unwrap();
        assert_eq!(ungrouped.members.len(), 2);
        assert!(!hermes_memory::topics::is_stale(&cards, &active()));
    }

    #[test]
    fn card_of_only_dead_members_is_dropped_and_the_rest_is_kept_reachable() {
        let cards = finalize_cards(
            vec![draft("", "空", &["mem_nope"])],
            &active(),
            &TopicCards::default(),
        );
        assert!(!cards.cards.iter().any(|c| c.title == "空"));
        // A build must leave nothing behind, or the card set would read as
        // permanently stale.
        assert!(!hermes_memory::topics::is_stale(&cards, &active()));
        let ungrouped = cards
            .cards
            .iter()
            .find(|c| c.title == UNGROUPED_TITLE)
            .unwrap();
        assert_eq!(ungrouped.members.len(), 3);
    }

    #[test]
    fn reused_id_keeps_built_at() {
        let existing = TopicCards {
            cards: vec![TopicCard {
                id: "topic_old".into(),
                title: "财经".into(),
                summary: "s".into(),
                members: vec!["mem_a".into()],
                built_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        let cards = finalize_cards(
            vec![draft("topic_old", "财经内容", &["mem_a", "mem_c"])],
            &active(),
            &existing,
        );
        let card = cards.cards.iter().find(|c| c.id == "topic_old").unwrap();
        assert_eq!(card.built_at, "2026-01-01T00:00:00Z");
        assert_eq!(card.title, "财经内容");
        assert_eq!(card.members.len(), 2);
    }

    #[test]
    fn rebuild_that_reworks_a_card_keeps_its_id_by_overlap() {
        let existing = TopicCards {
            cards: vec![TopicCard {
                id: "topic_old".into(),
                title: "财经".into(),
                summary: "s".into(),
                members: vec!["mem_a".into()],
                built_at: "2026-01-01T00:00:00Z".into(),
            }],
        };
        // Fresh id, but the membership is the same card.
        let cards = finalize_cards(
            vec![draft("", "财经内容", &["mem_a", "mem_c"])],
            &active(),
            &existing,
        );
        assert!(cards.cards.iter().any(|c| c.id == "topic_old"));
    }

    #[test]
    fn one_memory_may_sit_on_two_cards() {
        let cards = finalize_cards(
            vec![
                draft("", "财经", &["mem_a", "mem_c"]),
                draft("", "交付方式", &["mem_a"]),
            ],
            &active(),
            &TopicCards::default(),
        );
        assert!(!hermes_memory::topics::is_stale(&cards, &active()));
        let delivery = cards.cards.iter().find(|c| c.title == "交付方式").unwrap();
        assert_eq!(delivery.members, vec!["mem_a"]);
    }

    #[test]
    fn a_card_over_the_split_threshold_is_flagged_for_splitting() {
        let memories: Vec<LoadedMemory> =
            (0..17).map(|i| mem(&format!("mem_{i:02}"), "x")).collect();
        let card = |id: &str, title: &str, slice: &[LoadedMemory]| TopicCard {
            id: id.into(),
            title: title.into(),
            summary: "s".into(),
            members: slice.iter().map(|m| m.id().to_string()).collect(),
            built_at: "2026-09-14T00:00:00Z".into(),
        };
        let existing = TopicCards {
            cards: vec![
                card("topic_big", "大卡", &memories[..13]),
                card("topic_small", "小卡", &memories[13..]),
            ],
        };
        let focused: Vec<&LoadedMemory> = memories.iter().collect();
        let prompt = build_prompt(&focused, &existing, &memories, false);

        assert_eq!(prompt.matches('⚠').count(), 1, "{prompt}");
        assert!(prompt.contains("⚠ 13 members"), "{prompt}");
    }

    #[test]
    fn never_more_than_the_cap_of_themed_cards() {
        let memories: Vec<LoadedMemory> =
            (0..40).map(|i| mem(&format!("mem_{i:03}"), "x")).collect();
        let drafts: Vec<CardDraft> = (0..40)
            .map(|i| draft("", &format!("t{i}"), &[&format!("mem_{i:03}")]))
            .collect();
        let cards = finalize_cards(drafts, &memories, &TopicCards::default());
        assert!(cards.cards.len() <= MAX_CARDS + 1);
        assert!(!hermes_memory::topics::is_stale(&cards, &memories));
    }
}

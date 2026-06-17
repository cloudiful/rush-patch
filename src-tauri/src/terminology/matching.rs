use crate::domain::TranslationUnit;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTerm {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default)]
pub struct TermMatchIndex {
    states: BTreeMap<String, MatchState>,
}

#[derive(Debug, Clone)]
struct MatchState {
    target: String,
    priority: i64,
    conflicted: bool,
}

impl TermMatchIndex {
    pub fn from_terms(terms: &[CanonicalTerm]) -> Self {
        let mut index = Self::default();
        for term in terms {
            index.ingest_term(term, 1_000);
        }
        index
    }

    pub fn ingest_term(&mut self, term: &CanonicalTerm, priority: i64) {
        match self.states.get_mut(&term.source) {
            None => {
                self.states.insert(
                    term.source.clone(),
                    MatchState {
                        target: term.target.clone(),
                        priority,
                        conflicted: false,
                    },
                );
            }
            Some(existing) if existing.target == term.target => {
                existing.priority = existing.priority.max(priority);
            }
            Some(existing) if priority > existing.priority => {
                existing.target = term.target.clone();
                existing.priority = priority;
                existing.conflicted = false;
            }
            Some(existing) if priority == existing.priority => {
                existing.conflicted = true;
            }
            Some(_) => {}
        }
    }

    pub fn extend<I>(&mut self, terms: I)
    where
        I: IntoIterator<Item = (CanonicalTerm, i64)>,
    {
        for (term, priority) in terms {
            self.ingest_term(&term, priority);
        }
    }

    pub fn has_terms(&self) -> bool {
        self.states.values().any(|state| !state.conflicted)
    }

    pub fn batch_terms_for_units(
        &self,
        units: &[TranslationUnit],
        limit: usize,
    ) -> Vec<CanonicalTerm> {
        if limit == 0 {
            return Vec::new();
        }

        let mut hits = self
            .states
            .iter()
            .filter(|(_, state)| !state.conflicted)
            .filter(|(source, _)| batch_contains_source_term(units, source))
            .map(|(source, state)| CanonicalTerm {
                source: source.clone(),
                target: state.target.clone(),
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .source
                .chars()
                .count()
                .cmp(&left.source.chars().count())
                .then_with(|| left.source.cmp(&right.source))
        });
        hits.truncate(limit);
        hits
    }

    pub fn source_text_terms<'a>(&'a self, source_text: &str, limit: usize) -> Vec<TermMatch<'a>> {
        if limit == 0 {
            return Vec::new();
        }

        let mut hits = self
            .states
            .iter()
            .filter(|(_, state)| !state.conflicted)
            .filter(|(source, _)| source_text.contains(source.as_str()))
            .map(|(source, state)| TermMatch {
                source: source.as_str(),
                target: state.target.as_str(),
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .source
                .chars()
                .count()
                .cmp(&left.source.chars().count())
                .then_with(|| left.source.cmp(right.source))
        });
        hits.truncate(limit);
        hits
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TermMatch<'a> {
    pub source: &'a str,
    pub target: &'a str,
}

fn batch_contains_source_term(units: &[TranslationUnit], source: &str) -> bool {
    units.iter().any(|unit| unit_contains_source_term(unit, source))
}

fn unit_contains_source_term(unit: &TranslationUnit, source: &str) -> bool {
    unit.source_text.contains(source)
        || unit
            .context
            .speaker_name
            .as_deref()
            .is_some_and(|speaker| speaker.contains(source))
        || unit.context.prev_texts.iter().any(|text| text.contains(source))
        || unit.context.next_texts.iter().any(|text| text.contains(source))
        || unit
            .context
            .block_text
            .as_deref()
            .is_some_and(|text| text.contains(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContextEnvelope, TranslationStatus};

    #[test]
    fn user_terms_override_lower_priority_auto_terms() {
        let mut index = TermMatchIndex::default();
        index.ingest_term(
            &CanonicalTerm {
                source: "ロマーシャ".to_owned(),
                target: "罗马夏".to_owned(),
            },
            100,
        );
        index.ingest_term(
            &CanonicalTerm {
                source: "ロマーシャ".to_owned(),
                target: "罗玛夏".to_owned(),
            },
            1_000,
        );

        let hits = index.batch_terms_for_units(&[unit("ロマーシャの物語")], 4);

        assert_eq!(hits[0].target, "罗玛夏");
    }

    fn unit(source_text: &str) -> TranslationUnit {
        TranslationUnit {
            id: "u1".to_owned(),
            group_id: "u1".to_owned(),
            semantic_kind: "text".to_owned(),
            context: ContextEnvelope {
                file: "Map001.json".to_owned(),
                json_path: None,
                map_id: None,
                event_id: None,
                page_id: None,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: None,
                glossary_hits: Vec::new(),
                notes: Vec::new(),
            },
            source_text: source_text.to_owned(),
            translated_text: None,
            status: TranslationStatus::Pending,
            span_ids: Vec::new(),
        }
    }
}

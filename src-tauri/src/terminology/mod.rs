mod candidates;
mod matching;

pub use candidates::{
    AutoGlossaryCandidate, build_auto_glossary_candidates, candidate_seeds_from_catalog,
};
pub use matching::{CanonicalTerm, TermMatchIndex};

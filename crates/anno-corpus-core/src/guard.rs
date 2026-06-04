use crate::ids::CorpusId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectiveCorpus {
    Single(CorpusId),
    CrossCorpus,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CorpusGuardError {
    #[error("index a folder before using this tool")]
    NoCorpus,
    #[error("corpus_id is required because multiple corpora are indexed")]
    CorpusRequired,
    #[error("unknown corpus_id: {0}")]
    UnknownCorpus(String),
}

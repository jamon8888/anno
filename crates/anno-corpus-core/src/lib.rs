pub mod guard;
pub mod ids;
pub mod model;
pub mod root;

pub use guard::{CorpusGuardError, EffectiveCorpus};
pub use ids::{ContentId, CorpusId, DocumentInstanceId};
pub use model::{CorpusBindingKind, CorpusDocumentRef, CorpusProfile, CorpusSummary};
pub use root::{normalize_path, roots_overlap, CorpusRoot, RootError};

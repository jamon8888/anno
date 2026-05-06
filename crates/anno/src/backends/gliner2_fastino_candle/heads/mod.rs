//! GLiNER2 inference heads (Candle).
//!
//! Stub for M3 — concrete head implementations land in M5.

pub mod token_gather;
pub mod span_rep;
pub mod schema_gather;
pub mod count_pred;
pub mod count_lstm;
pub mod scorer;
pub mod classifier;

/// Container for all 7 inference heads.
pub struct AllHeads {
    // M5 populates these.
    #[doc(hidden)]
    pub _phantom: (),
}

impl AllHeads {
    /// Empty stub — used by M3's smoke test before real heads land.
    pub fn stub() -> Self {
        Self { _phantom: () }
    }
}

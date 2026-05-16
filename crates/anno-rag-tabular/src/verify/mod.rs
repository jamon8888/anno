//! Citation verification layer. Two-stage:
//!
//! - `offsets`: cheap, deterministic — every citation's
//!   `char_start..char_end` slice of its parent chunk must equal
//!   `quoted_text`. Mismatches downgrade the cell to
//!   `Confidence::Low`.
//!
//! - `support` (T29): expensive, model-based — cross-encoder scores
//!   `(column.prompt, citation.quoted_text)` and bins to
//!   High/Medium/Low. Sets `cell.support_score`.

pub mod offsets;
pub mod support;

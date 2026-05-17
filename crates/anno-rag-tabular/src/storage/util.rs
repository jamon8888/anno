//! Small helpers shared across the per-table storage modules.
//!
//! Kept private to the `storage` namespace because the helpers are
//! conventions of *how we encode/query our Lance tables*, not part of
//! the crate's public surface.

use arrow_array::{Array, StringArray};

/// Render a UUID as the uppercase hex blob literal Lance/DataFusion's
/// SQL expects for `FixedSizeBinary` filters (`X'...'`).
///
/// DataFusion accepts the `X'…'` byte-literal form for binary equality
/// but not the canonical UUID string with dashes, so every `id = ?`
/// filter in this crate goes through here.
pub(crate) fn uuid_to_filter_lit(u: uuid::Uuid) -> String {
    let mut s = String::with_capacity(32);
    for b in u.as_bytes() {
        use std::fmt::Write as _;
        // Writing to a `String` is infallible.
        let _ = write!(&mut s, "{b:02X}");
    }
    s
}

/// Null-aware `&str → Option<String>` for nullable `StringArray`
/// columns. Pulled out because both `reviews` and `columns` decode
/// nullable text fields the same way; keeping it in one place avoids
/// drift if we ever change the null convention.
pub(crate) fn opt_str(a: &StringArray, i: usize) -> Option<String> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i).to_string())
    }
}

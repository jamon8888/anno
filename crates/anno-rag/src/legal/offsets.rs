//! Span translation between raw chunk text and pseudonymized chunk text.

/// One substitution recorded during pseudonymization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Substitution {
    /// Raw-text byte start.
    pub raw_start: u32,
    /// Raw-text byte end.
    pub raw_end: u32,
    /// Pseudonymized-text byte start.
    pub pseudo_start: u32,
    /// Pseudonymized-text byte end.
    pub pseudo_end: u32,
}

/// Ordered set of substitutions covering one chunk.
#[derive(Debug, Clone, Default)]
pub struct PseudoOffsetMap {
    /// Substitutions ordered by raw start offset.
    pub subs: Vec<Substitution>,
}

impl PseudoOffsetMap {
    /// Map a raw-text span to the corresponding pseudonymized-text span.
    ///
    /// Returns `None` if the raw span partially overlaps a substitution.
    #[must_use]
    pub fn translate(&self, raw_start: u32, raw_end: u32) -> Option<(u32, u32)> {
        let mut delta: i64 = 0;
        for sub in &self.subs {
            if sub.raw_end <= raw_start {
                delta += (i64::from(sub.pseudo_end) - i64::from(sub.pseudo_start))
                    - (i64::from(sub.raw_end) - i64::from(sub.raw_start));
                continue;
            }
            if sub.raw_start >= raw_end {
                break;
            }
            if sub.raw_start <= raw_start && sub.raw_end >= raw_end {
                return Some((sub.pseudo_start, sub.pseudo_end));
            }
            return None;
        }

        let pseudo_start = (i64::from(raw_start) + delta).max(0) as u32;
        let pseudo_end = (i64::from(raw_end) + delta).max(0) as u32;
        Some((pseudo_start, pseudo_end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(subs: &[Substitution]) -> PseudoOffsetMap {
        PseudoOffsetMap {
            subs: subs.to_vec(),
        }
    }

    #[test]
    fn translate_no_subs_is_identity() {
        let offset_map = map(&[]);
        assert_eq!(offset_map.translate(5, 10), Some((5, 10)));
    }

    #[test]
    fn translate_after_substitution_shifts_by_delta() {
        let offset_map = map(&[Substitution {
            raw_start: 0,
            raw_end: 4,
            pseudo_start: 0,
            pseudo_end: 5,
        }]);
        assert_eq!(offset_map.translate(5, 11), Some((6, 12)));
    }

    #[test]
    fn translate_inside_substitution_returns_sub_range() {
        let offset_map = map(&[Substitution {
            raw_start: 0,
            raw_end: 4,
            pseudo_start: 0,
            pseudo_end: 5,
        }]);
        assert_eq!(offset_map.translate(0, 4), Some((0, 5)));
    }

    #[test]
    fn translate_partial_overlap_returns_none() {
        let offset_map = map(&[Substitution {
            raw_start: 5,
            raw_end: 9,
            pseudo_start: 5,
            pseudo_end: 10,
        }]);
        assert_eq!(offset_map.translate(7, 12), None);
    }
}

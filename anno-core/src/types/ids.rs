//! Type-safe identifier types for the annotation hierarchy.
//!
//! These newtypes provide compile-time safety to prevent accidentally
//! mixing up different kinds of IDs (e.g., passing a `TrackId` where
//! a `SignalId` is expected).
//!
//! # Design
//!
//! - `#[repr(transparent)]` ensures zero-cost abstraction
//! - Serde serializes as plain u64 for backwards compatibility
//! - All common traits derived for ergonomic use
//!
//! # Example
//!
//! ```rust
//! use anno_core::types::{SignalId, TrackId};
//!
//! fn process_signal(id: SignalId) { /* ... */ }
//! fn process_track(id: TrackId) { /* ... */ }
//!
//! let signal_id = SignalId::new(1);
//! let track_id = TrackId::new(1);
//!
//! process_signal(signal_id);  // OK
//! process_track(track_id);    // OK
//! // process_signal(track_id); // Compile error! Type safety.
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Macro to define a type-safe ID newtype.
macro_rules! define_id {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        #[repr(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Zero ID constant.
            pub const ZERO: Self = Self(0);

            /// Create a new ID.
            #[inline]
            #[must_use]
            pub const fn new(id: u64) -> Self {
                Self(id)
            }

            /// Get the raw u64 value.
            #[inline]
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }

            /// Increment the ID and return the new value.
            #[inline]
            #[must_use]
            pub const fn next(self) -> Self {
                Self(self.0 + 1)
            }

            /// Increment this ID in place and return the old value.
            /// Useful for generating sequential IDs.
            #[inline]
            pub fn next_mut(&mut self) -> Self {
                let old = *self;
                self.0 += 1;
                old
            }
        }

        impl std::ops::AddAssign<u64> for $name {
            #[inline]
            fn add_assign(&mut self, rhs: u64) {
                self.0 += rhs;
            }
        }

        impl std::ops::Add<u64> for $name {
            type Output = Self;
            #[inline]
            fn add(self, rhs: u64) -> Self {
                Self(self.0 + rhs)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u64> for $name {
            #[inline]
            fn from(id: u64) -> Self {
                Self(id)
            }
        }

        impl From<usize> for $name {
            #[inline]
            fn from(id: usize) -> Self {
                Self(id as u64)
            }
        }

        impl From<i32> for $name {
            #[inline]
            fn from(id: i32) -> Self {
                Self(id as u64)
            }
        }

        impl From<$name> for u64 {
            #[inline]
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl From<$name> for usize {
            #[inline]
            fn from(id: $name) -> Self {
                id.0 as usize
            }
        }

        // Serde: serialize as plain u64 for backwards compatibility
        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                u64::deserialize(deserializer).map(Self)
            }
        }
    };
}

define_id! {
    /// Unique identifier for a signal within a document.
    ///
    /// Signals are Level 1 in the hierarchy: raw entity detections.
    /// Each signal has a unique ID within its containing document.
    SignalId
}

define_id! {
    /// Unique identifier for a track within a document.
    ///
    /// Tracks are Level 2: within-document coreference chains.
    /// A track groups multiple signals that refer to the same entity.
    TrackId
}

define_id! {
    /// Unique identifier for an identity within a corpus.
    ///
    /// Identities are Level 3: cross-document entities with optional KB links.
    /// An identity may span multiple documents and link to knowledge bases.
    IdentityId
}

define_id! {
    /// Unique identifier for a coreference cluster.
    ///
    /// Used on `Entity.canonical_id` to group coreferent mentions.
    /// Within a document, entities with the same `CanonicalId` refer to
    /// the same real-world entity.
    CanonicalId
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_safety() {
        let signal_id = SignalId::new(1);
        let track_id = TrackId::new(1);

        // Same underlying value but different types
        assert_eq!(signal_id.get(), track_id.get());
        assert_eq!(signal_id.get(), 1);
        assert_eq!(track_id.get(), 1);

        // Cannot compare directly (different types)
        // This would fail to compile:
        // assert_eq!(signal_id, track_id);
    }

    #[test]
    fn test_from_u64() {
        let id: SignalId = 42u64.into();
        assert_eq!(id.get(), 42);

        let back: u64 = id.into();
        assert_eq!(back, 42);
    }

    #[test]
    fn test_serde_roundtrip() {
        let id = SignalId::new(123);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "123"); // Serializes as plain number

        let recovered: SignalId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, recovered);
    }

    #[test]
    fn test_display() {
        let id = TrackId::new(42);
        assert_eq!(format!("{}", id), "42");
        assert_eq!(format!("{:?}", id), "TrackId(42)");
    }

    #[test]
    fn test_next() {
        let id = SignalId::new(5);
        let next = id.next();
        assert_eq!(next.get(), 6);
    }

    #[test]
    fn test_ordering() {
        let a = SignalId::new(1);
        let b = SignalId::new(2);
        let c = SignalId::new(1);

        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, c);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(SignalId::new(1));
        set.insert(SignalId::new(2));
        set.insert(SignalId::new(1)); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_default() {
        let id = SignalId::default();
        assert_eq!(id.get(), 0);
    }
}

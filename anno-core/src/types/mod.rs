//! Shared types for the anno toolbox.
//!
//! This module provides canonical definitions for types that are used across
//! multiple modules to avoid duplication and ensure consistency.
//!
//! # Types
//!
//! ## Identifiers (Type-Safe)
//!
//! - [`SignalId`] - Unique identifier for signals (Level 1 detections)
//! - [`TrackId`] - Unique identifier for tracks (Level 2 coreference chains)
//! - [`IdentityId`] - Unique identifier for identities (Level 3 KB-linked entities)
//! - [`CanonicalId`] - Coreference cluster ID for within-document grouping
//!
//! ## Linguistic Features
//!
//! - [`Gender`] - Gender classification for coreference and bias analysis
//! - [`MentionType`] - Types of referring expressions (proper, nominal, pronominal, zero)
//! - [`PhiFeatures`] - Person/number/gender features for morphological agreement
//!
//! ## Temporal & Statistical
//!
//! - [`MetricStats`] - Statistical metrics with variance and confidence intervals
//! - [`HistoricalDate`] - Dates that can represent BCE/CE with uncertainty
//! - [`TemporalValidity`] - Time range during which an entity is valid
//!
//! ## Discourse & Framing
//!
//! - [`FramingAttitude`] - Supportive/skeptical/neutral framing stance
//! - [`FrecoPair`] - Framing-divergent event coreference pair

pub mod framing;
mod gender;
mod ids;
mod mention_type;
mod metric;
mod phi_features;
mod temporal;

pub use framing::{
    EventArgument, EventCorefRelation, EventMention, FramingAttitude, FramingDivergenceType,
    FrecoCorpus, FrecoCorpusMetadata, FrecoPair,
};
pub use gender::Gender;
pub use ids::{CanonicalId, IdentityId, SignalId, TrackId};
pub use mention_type::MentionType;
pub use metric::MetricStats;
pub use phi_features::{Number, Person, PhiFeatures};
pub use temporal::{DatePrecision, HistoricalDate, TemporalValidity};

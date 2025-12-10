//! Shared types for the anno toolbox.
//!
//! This module provides canonical definitions for types that are used across
//! multiple modules to avoid duplication and ensure consistency.
//!
//! # Types
//!
//! - [`Gender`] - Gender classification for coreference and bias analysis
//! - [`MentionType`] - Types of referring expressions (proper, nominal, pronominal, zero)
//! - [`PhiFeatures`] - Person/number/gender features for morphological agreement
//! - [`MetricStats`] - Statistical metrics with variance and confidence intervals
//! - [`HistoricalDate`] - Dates that can represent BCE/CE with uncertainty
//! - [`TemporalValidity`] - Time range during which an entity is valid
//! - [`FramingAttitude`] - Supportive/skeptical/neutral framing stance
//! - [`FrecoPair`] - Framing-divergent event coreference pair

pub mod framing;
mod gender;
mod mention_type;
mod metric;
mod phi_features;
mod temporal;

pub use framing::{
    EventArgument, EventCorefRelation, EventMention, FramingAttitude, FramingDivergenceType,
    FrecoCorpus, FrecoCorpusMetadata, FrecoPair,
};
pub use gender::Gender;
pub use mention_type::MentionType;
pub use metric::MetricStats;
pub use phi_features::{Number, Person, PhiFeatures};
pub use temporal::{DatePrecision, HistoricalDate, TemporalValidity};

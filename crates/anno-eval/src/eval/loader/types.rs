//! Type definitions for the dataset loader.
//!
//! All data types used by the loader: dataset identifiers, cache structures,
//! annotated data, metadata, and dataset statistics.

use anno::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

pub use crate::eval::dataset_registry::DatasetId;

/// Dataset identifier guaranteed to be loadable by [`super::DatasetLoader`].
///
/// This is a thin wrapper over [`DatasetId`] that enforces loader support via
/// `TryFrom<DatasetId>` / `FromStr`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LoadableDatasetId(pub(crate) DatasetId);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum DatasetParsePlan {
    Conll,
    JsonlNer,
    WikiannJson,
    TweetNer7,
    DocredJson,
    GoogleReCorpus,
    ChisiecJson,
    CadecHybrid,
    Bc5cdr,
    NcbiDisease,
    GapTsv,
    PrecoJsonl,
    Litbank,
    EcbPlus,
    AfriSenti,
    AfriQa,
    MasakhaNews,
    Conllu,
    AgNews,
    Dbpedia14,
    YahooAnswers,
    Trec,
    TweetTopic,
    Maven,
    MavenArg,
    Casie,
    Rams,
    HfApiResponse,
    /// TSV-based NER format (HIPE-2022 style)
    TsvNer,
    /// CSV-based NER format (E-NER/EDGAR-NER style: Token,Tag)
    CsvNer,
}

impl LoadableDatasetId {
    /// All datasets that have loading implementations.
    #[must_use]
    pub fn all() -> Vec<Self> {
        DatasetId::all()
            .iter()
            .copied()
            .filter(|id| Self::is_loadable_dataset(*id))
            .map(Self)
            .collect()
    }

    /// Access the underlying catalog ID.
    #[must_use]
    pub fn into_inner(self) -> DatasetId {
        self.0
    }

    #[must_use]
    pub(crate) fn is_loadable_dataset(id: DatasetId) -> bool {
        // Loader truth: some catalog entries are intentionally non-loadable (blocked, deprecated,
        // or require manual access) even if they have a parse plan.
        //
        // Keep this list small and motivated. Prefer expressing "non-loadable" in the registry
        // metadata first, then enforcing it here so CLI counts and `LoadableDatasetId` remain
        // honest.
        if matches!(
            id,
            DatasetId::CoNLL2002 | DatasetId::CoNLL2002Spanish | DatasetId::CoNLL2002Dutch
        ) {
            return false;
        }

        Self::parse_plan(id).is_some()
    }

    #[must_use]
    pub(crate) fn parse_plan(id: DatasetId) -> Option<DatasetParsePlan> {
        if let Some(hint) = Self::registry_hint_plan(id) {
            return Some(hint);
        }

        Some(match id {
            // ===================================================================
            // CoNLL/BIO format datasets
            // ===================================================================
            // These datasets use standard CoNLL-2003 or BIO tagging format.
            // The CoNLL parser handles: space/tab-separated columns, IOB2/BIO schemes,
            // and standard entity type mappings (PER, ORG, LOC, etc.).
            //
            // Note: Some datasets listed here are also handled by registry_hint_plan()
            // but are kept for explicit control or historical reasons.
            //
            // To add more: If a dataset has format="CoNLL" or format="BIO" in the registry
            // and is NER-only, it should be auto-detected. Add here only if auto-detection
            // fails (e.g., multi-task datasets, ambiguous metadata).
            DatasetId::WikiGold
            | DatasetId::Wnut17
            | DatasetId::MitMovie
            | DatasetId::MitRestaurant
            | DatasetId::CoNLL2003Sample
            | DatasetId::OntoNotesSample
            | DatasetId::UniversalNERBench
            | DatasetId::LegNER
            | DatasetId::TwiConv
            | DatasetId::AIDA
            | DatasetId::TACKBP
            | DatasetId::BroadTwitterCorpus
            | DatasetId::BioMNER
            | DatasetId::MasakhaNER
            | DatasetId::MasakhaNER2
            | DatasetId::OntoNotes50
            | DatasetId::GermEval2014
            | DatasetId::HAREM
            | DatasetId::SemEval2013Task91
            | DatasetId::MUC6
            | DatasetId::MUC7
            | DatasetId::JNLPBA
            | DatasetId::BC2GMFull
            | DatasetId::CRAFT
            | DatasetId::FinNER
            | DatasetId::LegalNER
            | DatasetId::AGRONER
            | DatasetId::ATISFlightBooking
            | DatasetId::AerospaceNERDataset
            | DatasetId::AstrologyNER
            | DatasetId::AutomotiveNER
            | DatasetId::AviationProductsNER
            | DatasetId::BeekeepingNER
            | DatasetId::BrewingNER
            | DatasetId::ChineseEngineeringGeologyNER
            | DatasetId::CocktailNER
            | DatasetId::ConstructionNER
            | DatasetId::CropDiseaseNER
            | DatasetId::CryptoNER
            | DatasetId::CyNERAptner
            | DatasetId::DnDNERBenchmark
            | DatasetId::EFGC
            | DatasetId::EnergyNER
            | DatasetId::EquestrianNER
            | DatasetId::EsportsNER
            | DatasetId::FINERFood
            | DatasetId::FitnessNER
            | DatasetId::FourRegionsGeologyNER
            | DatasetId::GNERGeoscience
            | DatasetId::GardeningNER
            | DatasetId::HealthcareAdminNER
            | DatasetId::HindiEnglishSocialMediaNER
            | DatasetId::JobPostingNER
            | DatasetId::LogisticsNER
            | DatasetId::ArabicEventCoref
            | DatasetId::FrenchFullLengthFictionCoref
            | DatasetId::LongtoNotes
            | DatasetId::ManufacturingNER
            | DatasetId::MaritimeNER
            | DatasetId::MovieCoref
            | DatasetId::MusicNER
            | DatasetId::NDNER
            | DatasetId::OrigamiNER
            | DatasetId::PaleontologyNER
            | DatasetId::PharmaNER
            | DatasetId::PhotographyNER
            | DatasetId::RealEstateNER
            | DatasetId::RitterTwitterNER
            | DatasetId::SECFilingsNER
            | DatasetId::SanskritNERBhagavadGita
            | DatasetId::ScubaNER
            | DatasetId::SportsNERGeneral
            | DatasetId::TVShowMultilingualCoref
            | DatasetId::TarotNER
            | DatasetId::TelecomNER
            | DatasetId::TelenovelaNER
            | DatasetId::TourismNER
            | DatasetId::VeterinaryNER
            | DatasetId::WaterResourceNER
            | DatasetId::WeatherNER
            | DatasetId::WineNER
            | DatasetId::WoodworkingNER
            // Added 2025-12: More CoNLL-format datasets with direct download URLs
            | DatasetId::QxoRef
            | DatasetId::GICoref
            | DatasetId::WNUT16
            | DatasetId::NoiseBench
            | DatasetId::CrossWeigh
            | DatasetId::ZELDA
            | DatasetId::GENIANested => DatasetParsePlan::Conll,

            // ===================================================================
            // JSONL format (HuggingFace style)
            // ===================================================================
            // These datasets use JSON Lines format with standard fields:
            // - `tokens`: Array of token strings
            // - `ner_tags`: Array of BIO/IOB2 tags
            // - Optional: `text` (original sentence), `entities` (structured)
            //
            // The JsonlNer parser is flexible and handles variations in field names.
            DatasetId::MultiNERD
            | DatasetId::ACORD
            | DatasetId::ARFFiction
            | DatasetId::AgMNER
            | DatasetId::AgriNER
            | DatasetId::AnimeMangaNER
            | DatasetId::AntiquesNER
            | DatasetId::AstronomicalTelegramKEE
            | DatasetId::BirdwatchingNER
            | DatasetId::BoardGameNER
            | DatasetId::BookCorefBamman
            | DatasetId::CUAD
            | DatasetId::ChessNER
            | DatasetId::CoMTA
            | DatasetId::DeepFashion2
            | DatasetId::FIREBALL
            | DatasetId::FashionIQ
            | DatasetId::FragranceNER
            | DatasetId::IMDbSemiStructuredRE
            | DatasetId::InsuranceNER
            | DatasetId::KnittingNER
            | DatasetId::LLMRocMinNER
            | DatasetId::MOFDataset
            | DatasetId::MathDial
            | DatasetId::NHKRecipeDataset
            | DatasetId::NaturalProductsRE
            | DatasetId::NumismaticsNER
            | DatasetId::PhilatelyNER
            | DatasetId::PolyIE
            | DatasetId::RecipeDBAnnotated
            | DatasetId::ResumeNER
            | DatasetId::RetailInventoryNER
            | DatasetId::SPoRC
            | DatasetId::Saraga
            | DatasetId::SolidStateDoping
            | DatasetId::SpotifyPodcastsDataset
            | DatasetId::TattooNER
            | DatasetId::ThemeParkNER
            | DatasetId::VREN
            | DatasetId::VisDialCoref
            // Added 2025-12: More JSONL-format datasets with direct download URLs
            | DatasetId::REBEL
            | DatasetId::BBQ
            | DatasetId::RealToxicityPrompts
            | DatasetId::BookCoref
            | DatasetId::BookCorefSplit
            | DatasetId::WIESP2022NER
            | DatasetId::FewRel
            | DatasetId::PIIMasking200k
            | DatasetId::B2NERD
            | DatasetId::OpenNER
            | DatasetId::FictionNER750M => DatasetParsePlan::JsonlNer,

            // WikiANN-ish JSON format (from download_hf_datasets.py)
            DatasetId::UNER | DatasetId::MSNER => DatasetParsePlan::WikiannJson,

            // TweetNER7 uses JSON format with integer tags
            DatasetId::TweetNER7 => DatasetParsePlan::TweetNer7,

            // ===================================================================
            // JSON format (Relation extraction datasets)
            // ===================================================================
            // Relation extraction datasets use JSON format with entity pairs and relations.
            // Common structure: {entities: [...], relations: [...], text: "..."}
            //
            // The DocredJson parser handles DocRED, ReTACRED, and similar formats.
            // Some datasets (SciERC, PolyIE, EnzChemRED) are auto-detected via
            // registry_hint_plan() but listed here for explicit control.
            //
            // To add more: RE datasets with format="JSON" or format="JSONL" and is_re=true
            // should be auto-detected. Add here only for special cases.
            DatasetId::DocRED
            | DatasetId::ReTACRED
            | DatasetId::NYTFB
            | DatasetId::WEBNLG
            | DatasetId::GoogleRE
            | DatasetId::BioRED
            | DatasetId::SciER
            | DatasetId::MixRED
            | DatasetId::CovEReD
            | DatasetId::ACE2005
            | DatasetId::MuDoCo
            | DatasetId::SciERCNER
            // Added 2025-01-27: RE datasets with public URLs but missing hints
            // Note: SciERC, PolyIE, EnzChemRED are now handled by registry_hint_plan() auto-detection
            => DatasetParsePlan::DocredJson,

            // CHisIEC: Ancient Chinese historical NER+RE (custom JSON format)
            DatasetId::CHisIEC => DatasetParsePlan::ChisiecJson,

            // Discontinuous NER (HF datasets-server API or JSONL format)
            DatasetId::CADEC | DatasetId::ShARe13 | DatasetId::ShARe14 => {
                DatasetParsePlan::CadecHybrid
            }

            // Biomedical formats
            DatasetId::BC5CDR => DatasetParsePlan::Bc5cdr,
            DatasetId::NCBIDisease => DatasetParsePlan::NcbiDisease,

            // Coreference formats
            // Note: GUM uses CoNLL format and is handled by registry_hint_plan()
            DatasetId::GAP | DatasetId::WikiCoref => DatasetParsePlan::GapTsv,
            DatasetId::WinoBias => DatasetParsePlan::GapTsv,
            DatasetId::PreCo | DatasetId::SciCo => DatasetParsePlan::PrecoJsonl,
            DatasetId::LitBank => DatasetParsePlan::Litbank,
            DatasetId::ECBPlus => DatasetParsePlan::EcbPlus,

            // African language datasets
            DatasetId::AfriSenti => DatasetParsePlan::AfriSenti,
            DatasetId::AfriQA => DatasetParsePlan::AfriQa,
            DatasetId::MasakhaNEWS => DatasetParsePlan::MasakhaNews,
            DatasetId::MasakhaPOS => DatasetParsePlan::Conllu,

            // Text classification datasets
            DatasetId::AGNews => DatasetParsePlan::AgNews,
            DatasetId::DBPedia14 => DatasetParsePlan::Dbpedia14,
            DatasetId::YahooAnswers => DatasetParsePlan::YahooAnswers,
            DatasetId::TREC => DatasetParsePlan::Trec,
            DatasetId::TweetTopic => DatasetParsePlan::TweetTopic,

            // Event extraction datasets
            DatasetId::MAVEN => DatasetParsePlan::Maven,
            DatasetId::MAVENArg => DatasetParsePlan::MavenArg,
            DatasetId::CASIE => DatasetParsePlan::Casie,
            DatasetId::RAMS => DatasetParsePlan::Rams,

            // HF datasets-server API responses
            DatasetId::GENIA
            | DatasetId::AnatEM
            | DatasetId::BC2GM
            | DatasetId::BC4CHEMD
            | DatasetId::FewNERD
            | DatasetId::CrossNER
            | DatasetId::FabNER
            | DatasetId::WikiNeural
            | DatasetId::WikiANN
            | DatasetId::MultiCoNER
            | DatasetId::MultiCoNERv2
            | DatasetId::PolyglotNER
            | DatasetId::UniversalNER => DatasetParsePlan::HfApiResponse,

            _ => return None,
        })
    }

    /// Best-effort "hint" derived from registry metadata.
    ///
    /// This is intentionally conservative: it returns `None` if the registry
    /// doesn't provide enough signal to choose the same plan as `parse_plan`.
    ///
    /// The long-term goal is to have this cover most datasets so `parse_plan`
    /// shrinks to a small exception table.
    ///
    /// # Architecture Notes
    ///
    /// This function enables automatic format detection for datasets with:
    /// - Clear format metadata (`format:` field in registry)
    /// - Single primary task (NER, RE, coref, etc.)
    /// - Standard annotation schemes (BIO, IOB2, CoNLL, JSONL)
    ///
    /// Datasets with multiple tasks (e.g., NER+RE) or ambiguous formats
    /// should be added to explicit matches in `parse_plan()` instead.
    ///
    /// # Adding New Datasets
    ///
    /// 1. **Common formats (CoNLL, JSONL, TSV)**: Add format metadata to registry,
    ///    ensure single primary task, and this function will auto-detect.
    /// 2. **Special formats**: Add explicit match in `parse_plan()` match statement.
    /// 3. **Multi-task datasets**: Add explicit match (auto-detection is conservative).
    #[must_use]
    pub(crate) fn registry_hint_plan(id: DatasetId) -> Option<DatasetParsePlan> {
        // Hybrids / special cases first.
        if id == DatasetId::CHisIEC {
            // CHisIEC is multi-task and uses a custom JSON array format (not DocRED/CrossRE).
            // It MUST not be auto-detected as DocredJson.
            return Some(DatasetParsePlan::ChisiecJson);
        }
        if matches!(
            id,
            DatasetId::CADEC | DatasetId::ShARe13 | DatasetId::ShARe14
        ) {
            return Some(DatasetParsePlan::CadecHybrid);
        }
        if id == DatasetId::GoogleRE {
            // The Google relation-extraction-corpus files are not DocRED/CrossRE.
            // They are JSONL records with fields like {pred, sub, obj, evidences[], judgments[]}.
            return Some(DatasetParsePlan::GoogleReCorpus);
        }
        if id == DatasetId::TweetNER7 {
            return Some(DatasetParsePlan::TweetNer7);
        }

        // HuggingFace-hosted datasets with bespoke (non-generic) parse plans.
        //
        // These are marked `access_status: HuggingFace` in the registry, so they must be
        // hintable; but their parse plans are not derivable from `format` alone (or use
        // specialized parsing despite being hosted on the Hub).
        if id == DatasetId::BC5CDR {
            return Some(DatasetParsePlan::Bc5cdr);
        }
        if id == DatasetId::NCBIDisease {
            return Some(DatasetParsePlan::NcbiDisease);
        }
        if id == DatasetId::AfriSenti {
            return Some(DatasetParsePlan::AfriSenti);
        }
        if id == DatasetId::AfriQA {
            return Some(DatasetParsePlan::AfriQa);
        }
        if id == DatasetId::MasakhaNEWS {
            return Some(DatasetParsePlan::MasakhaNews);
        }
        if id == DatasetId::AGNews {
            return Some(DatasetParsePlan::AgNews);
        }
        if id == DatasetId::DBPedia14 {
            return Some(DatasetParsePlan::Dbpedia14);
        }
        if id == DatasetId::YahooAnswers {
            return Some(DatasetParsePlan::YahooAnswers);
        }
        if id == DatasetId::TREC {
            return Some(DatasetParsePlan::Trec);
        }
        if id == DatasetId::TweetTopic {
            return Some(DatasetParsePlan::TweetTopic);
        }

        // Coref datasets in CoNLL format (not GAP TSV): parse as regular CoNLL.
        // These have NER annotations alongside coref, so the CoNLL parser works.
        if matches!(
            id,
            DatasetId::QxoRef
                | DatasetId::GICoref
                | DatasetId::WNUT16
                | DatasetId::NoiseBench
                | DatasetId::CrossWeigh
                | DatasetId::ZELDA
                | DatasetId::GENIANested
                // Added 2025-12: More CoNLL NER datasets
                | DatasetId::HistNERo
                | DatasetId::DutchArchaeology
                | DatasetId::FINER
                | DatasetId::CALCS2018
                | DatasetId::MedievalCharterNER
                | DatasetId::RockNER
                | DatasetId::AIDACoNLL
                | DatasetId::NNE
                | DatasetId::GermEvalDiscontinuous
                | DatasetId::PubMedDiscontinuous
                | DatasetId::IndicNER
                | DatasetId::NorNE
                | DatasetId::CHEMDNER
                | DatasetId::GeoWebNews
                | DatasetId::TASTEset
                | DatasetId::RecipeNER
                | DatasetId::AstroNER
                | DatasetId::FinanceNER
                | DatasetId::TechNER
                | DatasetId::CALCS
                | DatasetId::LinCE
                | DatasetId::FinTechPatent
                | DatasetId::WaterAgriNER
                | DatasetId::NERsocialFood
                | DatasetId::RussianCulturalNER
                | DatasetId::EighteenthCenturyNER
                | DatasetId::GuaraniNER
                | DatasetId::ShipiboKoniboNER
                | DatasetId::BASHI
                | DatasetId::ENER
                // Added 2025-01-27: CoNLL NER datasets with public URLs but missing hints
                | DatasetId::OntoNotes50
                | DatasetId::GUM
                // CoNLL04RE is CoNLL format (works with CoNLL parser even though it's for RE)
                | DatasetId::CoNLL04RE
        ) {
            return Some(DatasetParsePlan::Conll);
        }

        // JSONL datasets that should use JsonlNer despite having coref/RE tasks.
        // The JsonlNer parser handles tokens + ner_tags fields generically.
        if matches!(
            id,
            DatasetId::REBEL
                | DatasetId::BBQ
                | DatasetId::RealToxicityPrompts
                | DatasetId::BookCoref
                | DatasetId::BookCorefSplit
                | DatasetId::WIESP2022NER
                | DatasetId::FewRel
                | DatasetId::PIIMasking200k
                | DatasetId::B2NERD
                | DatasetId::OpenNER
                | DatasetId::FictionNER750M
                // Added 2025-12: More JSONL NER datasets
                | DatasetId::MultiWOZNER
                | DatasetId::HinglishNER
                | DatasetId::ChineseNestedNER
                | DatasetId::AgCNER
                | DatasetId::LongDocNER
                | DatasetId::MultiBioNERLong
                | DatasetId::ReasoningNER
                | DatasetId::BioNERLLaMA
                | DatasetId::LexGLUENER
                | DatasetId::FinBenNER
                | DatasetId::FiNER139
                | DatasetId::SciNER
                | DatasetId::CharacterCodex
                | DatasetId::AIONER
                | DatasetId::WIESPAstro
                | DatasetId::CEREC
                | DatasetId::DELICATE
                | DatasetId::CSN
                // Added 2025-01-27: JSONL NER datasets with public URLs but missing hints
                | DatasetId::SCINERNested
                | DatasetId::AgriNER
                | DatasetId::MOFDataset
                | DatasetId::SolidStateDoping
        ) {
            return Some(DatasetParsePlan::JsonlNer);
        }

        // CoNLLU datasets (Universal Dependencies treebanks with NER annotations)
        if matches!(
            id,
            DatasetId::AncientGreekUD
                | DatasetId::LatinUD
                | DatasetId::SanskritUD
                | DatasetId::OldEnglishUD
                | DatasetId::OldNorseUD
                | DatasetId::UDEsperantoCairo
                // Added 2025-12: More UD treebanks (ancient/historical/constructed)
                | DatasetId::CopticScriptorium
                | DatasetId::TaggedPBCEsperanto
                | DatasetId::TaggedPBCKlingon
                | DatasetId::AkkadianUD
                | DatasetId::AncientHebrewUD
                | DatasetId::ClassicalChineseUD
                | DatasetId::CopticUD
                | DatasetId::GothicUD
                | DatasetId::HittiteUD
                | DatasetId::OldChurchSlavonicUD
                | DatasetId::LatinITTB
                | DatasetId::LatinPROIEL
                | DatasetId::EsperantoUD
                | DatasetId::NavajoMorph
        ) {
            return Some(DatasetParsePlan::Conllu);
        }

        // TSV NER datasets (HIPE-2022 style)
        if matches!(id, DatasetId::HIPE2022) {
            return Some(DatasetParsePlan::TsvNer);
        }

        // CSV NER datasets
        if matches!(id, DatasetId::ENer) {
            return Some(DatasetParsePlan::CsvNer);
        }

        // Some datasets are fetched via HF datasets-server (JSON response), regardless of their
        // "canonical" source format in the registry.
        if matches!(
            id,
            DatasetId::GENIA
                | DatasetId::AnatEM
                | DatasetId::BC2GM
                | DatasetId::BC4CHEMD
                | DatasetId::FewNERD
                | DatasetId::CrossNER
                | DatasetId::FabNER
                | DatasetId::WikiNeural
                | DatasetId::WikiANN
                | DatasetId::MultiCoNER
                | DatasetId::MultiCoNERv2
                | DatasetId::PolyglotNER
                | DatasetId::UniversalNER
        ) {
            return Some(DatasetParsePlan::HfApiResponse);
        }

        // Use tasks_or_inferred() to support datasets that have task-like categories
        // (e.g., `categories: [ner, ancient]`) but no explicit `tasks` field yet.
        // This enables automatic loading for datasets with correct format + category.
        let inferred_tasks = id.tasks_or_inferred();
        let is_ner = inferred_tasks.contains(&"ner");
        let is_coref = inferred_tasks.contains(&"coref");
        let is_re = inferred_tasks.contains(&"re");
        let is_event = inferred_tasks.contains(&"event_extraction");

        let annotation_scheme = id.annotation_scheme().unwrap_or("");

        // Format string is about the *source* dataset, not necessarily our cached
        // representation; still useful for many direct-download datasets.
        if let Some(format) = id.format() {
            let plan = match format {
                "CoNLL" | "BIO" | "IOB2" if is_ner && !is_coref && !is_re && !is_event => {
                    Some(DatasetParsePlan::Conll)
                }
                "CoNLL-U" | "CoNLLU" if is_ner && !is_coref && !is_re && !is_event => {
                    Some(DatasetParsePlan::Conllu)
                }
                "JSONL" if is_ner && !is_coref && !is_re && !is_event => {
                    Some(DatasetParsePlan::JsonlNer)
                }
                // Coreference: we only treat these as unambiguous when the dataset is *only* coref.
                // Check both format field and annotation_scheme for CoNLLCoref datasets.
                "TSV" | "ConllCoref" | "CoNLLCoref"
                    if is_coref && !is_ner && !is_re && !is_event =>
                {
                    // GAP-style TSV and similar "coref-only" exports.
                    Some(DatasetParsePlan::GapTsv)
                }
                // CoNLL format with CoNLLCoref annotation scheme (e.g., GICoref, qxoRef)
                "CoNLL"
                    if annotation_scheme == "CoNLLCoref"
                        && is_coref
                        && !is_ner
                        && !is_re
                        && !is_event =>
                {
                    Some(DatasetParsePlan::GapTsv)
                }
                "JSONL" if is_coref && !is_ner && !is_re && !is_event => {
                    // PreCo/SciCo style JSONL exports.
                    Some(DatasetParsePlan::PrecoJsonl)
                }

                // Relation extraction: unambiguous only when the dataset is *only* RE.
                // Also handle RE datasets that might have NER as secondary task (e.g., SciERCNER).
                "JSON" | "JSONL" if is_re && !is_coref && !is_event => {
                    Some(DatasetParsePlan::DocredJson)
                }

                // Event extraction: treat JSONL as MAVEN-ish only when the dataset is event-only.
                "JSONL" if is_event && !is_ner && !is_coref && !is_re => {
                    Some(DatasetParsePlan::Maven)
                }

                // TSV NER format (HIPE-2022 style)
                "TSV" if is_ner && !is_coref && !is_re && !is_event => {
                    Some(DatasetParsePlan::TsvNer)
                }

                // CSV NER format (E-NER/EDGAR-NER style: Token,Tag)
                "CSV" if is_ner && !is_coref && !is_re && !is_event => {
                    Some(DatasetParsePlan::CsvNer)
                }

                // These are too ambiguous across sources; treat as unknown for now.
                _ => None,
            };

            if plan.is_some() {
                return plan;
            }
        }

        // HuggingFace datasets: if the registry provides an HF id/config and marks the
        // dataset as HuggingFace-accessible, we can treat it as an HfApiResponse plan.
        //
        // Keep this as a *fallback*: registry format/task signals (and explicit hint lists
        // above) take precedence, so we don't override e.g. CoNLL/JSONL datasets that are
        // hosted on HuggingFace but should be parsed as raw files.
        if id.hf_id().is_some()
            && id.access_status()
                == crate::eval::dataset_registry::DatasetAccessibility::HuggingFace
        {
            return Some(DatasetParsePlan::HfApiResponse);
        }

        None
    }
}

impl std::ops::Deref for LoadableDatasetId {
    type Target = DatasetId;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<LoadableDatasetId> for DatasetId {
    fn from(value: LoadableDatasetId) -> Self {
        value.0
    }
}

impl std::str::FromStr for LoadableDatasetId {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let id: DatasetId = s
            .parse()
            .map_err(|e| Error::InvalidInput(format!("Invalid dataset id '{}': {}", s, e)))?;
        Self::try_from(id)
    }
}

impl TryFrom<DatasetId> for LoadableDatasetId {
    type Error = Error;

    fn try_from(value: DatasetId) -> std::result::Result<Self, Self::Error> {
        if Self::is_loadable_dataset(value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidInput(format!(
                "Dataset {:?} is in the registry but has no loading implementation",
                value
            )))
        }
    }
}

// =============================================================================
// Loader-specific extensions for `DatasetId`
// =============================================================================
//
// `DatasetId` is defined in `dataset_registry` (the full catalog of datasets).
// This module is intentionally split:
// - `DatasetId`: metadata/catalog (can include datasets that are not loadable here)
// - `LoadableDatasetId`: the subset that this loader+parsers support

// Extension methods for DatasetId that delegate to registry
impl DatasetId {
    /// Get default metadata for this dataset.
    ///
    /// Delegates to registry methods.
    #[must_use]
    pub fn default_metadata(&self) -> DatasetMetadata {
        DatasetMetadata {
            description: Some(self.description().to_string()),
            license: self.license().map(|s| s.to_string()),
            citation: self.citation().map(|s| s.to_string()),
            split: Some("test".to_string()), // Default to test split
            language: Some(self.language().to_string()),
            domain: Some(self.domain().to_string()),
            original_source: Some(self.download_url().to_string()),
            version: None,
            annotators: None,
            guidelines_url: None,
        }
    }
}

// =============================================================================
// Cache Manifest
// =============================================================================

/// Entry in the cache manifest tracking a downloaded dataset file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifestEntry {
    /// Dataset identifier.
    pub dataset_id: String,
    /// Source URL from which the data was downloaded.
    ///
    /// This is the *logical* registry URL (what the dataset entry says).
    pub source_url: String,
    /// The URL that was actually fetched (after any rewrites / mirrors).
    ///
    /// This is helpful for reproducibility (and debugging) when the registry URL is a
    /// homepage (e.g., HuggingFace dataset page) but we fetched via datasets-server
    /// or `resolve/main/...`.
    #[serde(default)]
    pub resolved_url: Option<String>,
    /// SHA-256 checksum of the downloaded file.
    pub sha256: String,
    /// File size in bytes.
    pub file_size: u64,
    /// ISO 8601 timestamp of when the file was downloaded.
    pub downloaded_at: String,
    /// Number of sentences parsed from the dataset.
    pub sentence_count: usize,
    /// Number of entities parsed from the dataset.
    pub entity_count: usize,
    /// Anno version that downloaded this file.
    pub anno_version: String,
}

/// Cache manifest tracking all downloaded datasets.
///
/// Stored as `manifest.json` in the cache directory.
/// Enables:
/// - Reproducibility: know exactly what was downloaded
/// - Validation: verify checksums match
/// - Debugging: trace when data was fetched
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheManifest {
    /// Version of the manifest format.
    pub version: u32,
    /// Entries for each cached dataset.
    pub entries: HashMap<String, CacheManifestEntry>,
}

impl CacheManifest {
    /// Current manifest format version.
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new empty manifest.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            entries: HashMap::new(),
        }
    }

    /// Load manifest from a cache directory.
    ///
    /// Returns empty manifest if file doesn't exist.
    pub fn load(cache_dir: &std::path::Path) -> Result<Self> {
        let manifest_path = cache_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| Error::InvalidInput(format!("Failed to read manifest: {}", e)))?;

        serde_json::from_str(&content)
            .map_err(|e| Error::InvalidInput(format!("Failed to parse manifest: {}", e)))
    }

    /// Save manifest to a cache directory.
    pub fn save(&self, cache_dir: &std::path::Path) -> Result<()> {
        let manifest_path = cache_dir.join("manifest.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| Error::InvalidInput(format!("Failed to serialize manifest: {}", e)))?;

        fs::write(&manifest_path, content)
            .map_err(|e| Error::InvalidInput(format!("Failed to write manifest: {}", e)))
    }

    /// Add or update an entry in the manifest.
    pub fn update_entry(&mut self, entry: CacheManifestEntry) {
        self.entries.insert(entry.dataset_id.clone(), entry);
    }

    /// Get entry for a dataset.
    #[must_use]
    pub fn get(&self, dataset_id: &str) -> Option<&CacheManifestEntry> {
        self.entries.get(dataset_id)
    }

    /// Check if a dataset's cached file matches the manifest.
    ///
    /// Returns `true` if the file exists and checksum matches.
    #[must_use]
    pub fn verify_entry(&self, dataset_id: &str, cache_dir: &std::path::Path) -> bool {
        let Some(entry) = self.entries.get(dataset_id) else {
            return false;
        };

        // Check file exists with expected size
        let file_path = cache_dir.join(&entry.dataset_id);
        let Ok(metadata) = fs::metadata(&file_path) else {
            return false;
        };

        metadata.len() == entry.file_size
        // Note: Full SHA-256 verification is expensive; size check is usually sufficient
    }
}

// =============================================================================
// Temporal Metadata
// =============================================================================

/// Temporal metadata for datasets (optional).
///
/// Used for temporal stratification of evaluation metrics.
/// Most datasets don't have temporal metadata, so this is optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalMetadata {
    /// KB version used for entity linking (if applicable)
    pub kb_version: Option<String>,
    /// Temporal cutoff date (entities before this date are "old", after are "new")
    pub temporal_cutoff: Option<String>, // ISO 8601 date string
    /// Entity creation dates (if available)
    pub entity_creation_dates: Option<HashMap<String, String>>, // entity_id -> ISO 8601 date
}

// =============================================================================
// Annotated Structures
// =============================================================================

/// Annotated token with NER tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedToken {
    /// Token text
    pub text: String,
    /// NER tag (BIO format)
    pub ner_tag: String,
}

/// Annotated sentence with tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotatedSentence {
    /// Tokens in the sentence
    pub tokens: Vec<AnnotatedToken>,
    /// Source dataset
    pub source_dataset: DatasetId,
}

impl AnnotatedSentence {
    /// Get text of the sentence.
    #[must_use]
    pub fn text(&self) -> String {
        self.tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get entities from this sentence.
    #[must_use]
    pub fn entities(&self) -> Vec<crate::eval::datasets::GoldEntity> {
        // Precompute character offsets for each token in `self.text()` (tokens joined by a
        // single ASCII space). Offsets are in *character* units, not bytes.
        let mut token_starts: Vec<usize> = Vec::with_capacity(self.tokens.len());
        let mut pos: usize = 0;
        for (i, tok) in self.tokens.iter().enumerate() {
            token_starts.push(pos);
            pos += tok.text.chars().count();
            if i + 1 < self.tokens.len() {
                pos += 1; // the space inserted by `text()`
            }
        }

        let mut entities = Vec::new();
        let mut i = 0;
        while i < self.tokens.len() {
            let tag = &self.tokens[i].ner_tag;

            // Check for BIO format (B-TYPE, I-TYPE)
            if tag.starts_with("B-") {
                let entity_type = tag.trim_start_matches("B-");
                let mut end = i + 1;
                // Only continue with I-tags that match the same entity type
                while end < self.tokens.len()
                    && self.tokens[end].ner_tag.starts_with("I-")
                    && self.tokens[end].ner_tag.trim_start_matches("I-") == entity_type
                {
                    end += 1;
                }
                let entity_text: String = self.tokens[i..end]
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                let start = token_starts.get(i).copied().unwrap_or(0);
                let end_char = if end <= i {
                    start
                } else {
                    let last = end - 1;
                    token_starts.get(last).copied().unwrap_or(start)
                        + self.tokens[last].text.chars().count()
                };

                entities.push(crate::eval::datasets::GoldEntity {
                    text: entity_text,
                    original_label: entity_type.to_string(),
                    entity_type: anno::EntityType::from_label(entity_type),
                    start,
                    end: end_char,
                });
                i = end;
            }
            // Fallback: non-BIO format (simple type names like "person", "organization")
            // Used by datasets like FewNERD that use ClassLabels
            else if tag != "O" && !tag.starts_with("I-") && !tag.starts_with("TAG_") {
                let entity_type = tag.as_str();
                let mut end = i + 1;
                // Continue while same entity type (treating consecutive same labels as one entity)
                while end < self.tokens.len() && self.tokens[end].ner_tag == *tag {
                    end += 1;
                }
                let entity_text: String = self.tokens[i..end]
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                let start = token_starts.get(i).copied().unwrap_or(0);
                let end_char = if end <= i {
                    start
                } else {
                    let last = end - 1;
                    token_starts.get(last).copied().unwrap_or(start)
                        + self.tokens[last].text.chars().count()
                };

                entities.push(crate::eval::datasets::GoldEntity {
                    text: entity_text,
                    original_label: entity_type.to_string(),
                    entity_type: anno::EntityType::from_label(entity_type),
                    start,
                    end: end_char,
                });
                i = end;
            } else {
                i += 1;
            }
        }
        entities
    }
}

/// Dataset metadata for provenance and reproducibility.
///
/// Captures all information needed to understand where data came from
/// and how to attribute/license it correctly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetMetadata {
    /// Human-readable description.
    pub description: Option<String>,
    /// License (e.g., "CC-BY-4.0", "Apache-2.0", "Research-only").
    pub license: Option<String>,
    /// Citation reference (BibTeX key or paper title).
    pub citation: Option<String>,
    /// Split type (e.g., "train", "dev", "test").
    pub split: Option<String>,
    /// Language code (e.g., "en", "de", "zh").
    pub language: Option<String>,
    /// Domain (e.g., "news", "biomedical", "social-media").
    pub domain: Option<String>,
    /// Original source repository or homepage.
    pub original_source: Option<String>,
    /// Version string if the dataset has versions.
    pub version: Option<String>,
    /// Number of annotators (for IAA reporting).
    pub annotators: Option<u32>,
    /// Annotation guidelines URL if available.
    pub guidelines_url: Option<String>,
}

/// Where the dataset was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DataSource {
    /// Loaded from S3 cache (fast, reliable).
    S3Cache,
    /// Loaded from local file cache.
    LocalCache,
    /// Downloaded from original URL (may be slow or unavailable).
    OriginalUrl,
    /// Dataset was skipped (URL broken or unavailable).
    #[default]
    Skipped,
    /// Embedded in binary (test fixtures).
    Embedded,
}

impl DataSource {
    /// Human-readable description of the source.
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::S3Cache => "S3 cache",
            Self::LocalCache => "local cache",
            Self::OriginalUrl => "original URL",
            Self::Skipped => "skipped (unavailable)",
            Self::Embedded => "embedded",
        }
    }

    /// Whether this source is cached (fast).
    #[must_use]
    pub fn is_cached(&self) -> bool {
        matches!(self, Self::S3Cache | Self::LocalCache | Self::Embedded)
    }

    /// Whether this source indicates the data is unavailable.
    #[must_use]
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Skipped)
    }
}

impl std::fmt::Display for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// A loaded dataset with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedDataset {
    /// Dataset identifier.
    pub id: DatasetId,
    /// Annotated sentences.
    pub sentences: Vec<AnnotatedSentence>,
    /// When the dataset was loaded.
    pub loaded_at: String, // ISO 8601
    /// Source URL.
    pub source_url: String,
    /// Where the data was actually loaded from.
    #[serde(default)]
    pub data_source: DataSource,
    /// Optional temporal metadata for temporal stratification
    #[serde(default)]
    pub temporal_metadata: Option<TemporalMetadata>,
    /// Dataset provenance and attribution metadata.
    #[serde(default)]
    pub metadata: DatasetMetadata,
}

impl LoadedDataset {
    /// Total number of sentences.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sentences.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sentences.is_empty()
    }

    /// Total number of entities.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.sentences.iter().map(|s| s.entities().len()).sum()
    }

    /// Count entities by type.
    #[must_use]
    pub fn entity_counts_by_type(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for sentence in &self.sentences {
            for entity in sentence.entities() {
                *counts.entry(entity.original_label.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Get statistics summary.
    #[must_use]
    pub fn stats(&self) -> DatasetStats {
        DatasetStats {
            name: self.id.name().to_string(),
            sentences: self.len(),
            tokens: self.sentences.iter().map(|s| s.tokens.len()).sum(),
            entities: self.entity_count(),
            entities_by_type: self.entity_counts_by_type(),
        }
    }

    /// Convert to test cases format for evaluation.
    #[must_use]
    pub fn to_test_cases(&self) -> Vec<(String, Vec<crate::eval::datasets::GoldEntity>)> {
        self.sentences
            .iter()
            .map(|s| (s.text(), s.entities()))
            .collect()
    }
}

// =============================================================================
// Dataset Statistics
// =============================================================================

/// Dataset statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetStats {
    /// Dataset name.
    pub name: String,
    /// Number of sentences.
    pub sentences: usize,
    /// Total token count.
    pub tokens: usize,
    /// Total entity count.
    pub entities: usize,
    /// Entities by type.
    pub entities_by_type: HashMap<String, usize>,
}

/// A document with text and gold relations for relation extraction evaluation.
#[derive(Debug, Clone)]
pub struct RelationDocument {
    /// Document text
    pub text: String,
    /// Gold standard relations
    pub relations: Vec<crate::eval::relation::RelationGold>,
}

impl std::fmt::Display for DatasetStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Dataset: {}", self.name)?;
        writeln!(f, "  Sentences: {}", self.sentences)?;
        writeln!(f, "  Tokens: {}", self.tokens)?;
        writeln!(f, "  Entities: {}", self.entities)?;
        writeln!(f, "  Entity types:")?;
        let mut types: Vec<_> = self.entities_by_type.iter().collect();
        types.sort_by(|a, b| b.1.cmp(a.1));
        for (etype, count) in types {
            writeln!(f, "    {}: {}", etype, count)?;
        }
        Ok(())
    }
}

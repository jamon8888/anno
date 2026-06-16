//! Dataset content parsers, grouped by task family.
pub(crate) mod classification;
pub(crate) mod coref;
pub(crate) mod event;
pub(crate) mod ner;
pub(crate) mod relation;
pub(crate) mod util;

use crate::eval::loader::types::{DatasetParsePlan, LoadableDatasetId, LoadedDataset};
use crate::eval::loader::DatasetId;
use anno::{Error, Result};

/// Parse content based on dataset format.
///
/// Internal dispatcher backing [`crate::eval::loader::DatasetLoader::parse_content_str`].
pub(crate) fn parse_content(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    if content.trim().is_empty() {
        return Err(Error::InvalidInput(format!(
            "Dataset {:?} file is empty",
            id
        )));
    }

    let plan = LoadableDatasetId::parse_plan(id).ok_or_else(|| {
        Error::InvalidInput(format!("No parser configured for dataset {:?}", id))
    })?;

    let result = match plan {
        DatasetParsePlan::Conll => ner::parse_conll(content, id),
        DatasetParsePlan::JsonlNer => ner::parse_jsonl_ner(content, id),
        DatasetParsePlan::WikiannJson => ner::parse_wikiann_json(content, id),
        DatasetParsePlan::TweetNer7 => ner::parse_tweetner7(content, id),
        DatasetParsePlan::DocredJson => relation::parse_docred(content, id),
        DatasetParsePlan::GoogleReCorpus => relation::parse_google_re_corpus(content, id),
        DatasetParsePlan::ChisiecJson => relation::parse_chisiec(content, id),
        DatasetParsePlan::CadecHybrid => {
            if is_hf_api_response(content) {
                ner::parse_cadec_hf_api(content, id)
            } else {
                ner::parse_cadec_jsonl(content, id)
            }
        }
        DatasetParsePlan::Bc5cdr => ner::parse_bc5cdr(content, id),
        DatasetParsePlan::NcbiDisease => ner::parse_ncbi_disease(content, id),
        DatasetParsePlan::GapTsv => coref::parse_gap(content, id),
        DatasetParsePlan::PrecoJsonl => coref::parse_preco_jsonl(content, id),
        DatasetParsePlan::Litbank => coref::parse_litbank(content, id),
        DatasetParsePlan::EcbPlus => coref::parse_ecb_plus(content, id),
        DatasetParsePlan::AfriSenti => classification::parse_afrisenti(content, id),
        DatasetParsePlan::AfriQa => classification::parse_afriqa(content, id),
        DatasetParsePlan::MasakhaNews => classification::parse_masakhanews(content, id),
        DatasetParsePlan::Conllu => ner::parse_conllu(content, id),
        DatasetParsePlan::AgNews => classification::parse_agnews(content, id),
        DatasetParsePlan::Dbpedia14 => classification::parse_dbpedia14(content, id),
        DatasetParsePlan::YahooAnswers => classification::parse_yahoo_answers(content, id),
        DatasetParsePlan::Trec => classification::parse_trec(content, id),
        DatasetParsePlan::TweetTopic => classification::parse_tweettopic(content, id),
        DatasetParsePlan::Maven => event::parse_maven(content, id),
        DatasetParsePlan::MavenArg => event::parse_maven_arg(content, id),
        DatasetParsePlan::Casie => event::parse_casie(content, id),
        DatasetParsePlan::Rams => event::parse_rams(content, id),
        DatasetParsePlan::HfApiResponse => ner::parse_hf_api_response(content, id),
        DatasetParsePlan::TsvNer => ner::parse_tsv_ner(content, id),
        DatasetParsePlan::CsvNer => ner::parse_csv_ner(content, id),
    }?;

    // Validate parsed dataset is not empty
    if result.sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "Dataset {:?} parsed successfully but contains no sentences. \
             This may indicate a parsing issue or empty dataset file.",
            id
        )));
    }

    Ok(result)
}

/// Check if content is HuggingFace datasets-server API response.
fn is_hf_api_response(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with("{\"rows\":")
        || trimmed.starts_with("{\"features\":")
        || (trimmed.starts_with("{")
            && trimmed.contains("\"rows\":[")
            && trimmed.contains("\"features\":["))
}

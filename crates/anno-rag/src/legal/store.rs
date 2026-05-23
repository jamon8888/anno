//! LanceDB storage for `legal_chunk_enrichment`.

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use crate::legal::types::{LegalChunkEnrichment, LegalSearchFilters};
use arrow_array::{Array, FixedSizeBinaryArray, RecordBatch, RecordBatchIterator};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use futures::TryStreamExt;
use lancedb::{Connection, Table};
use std::sync::Arc;
use uuid::Uuid;

/// LanceDB table name for chunk-level legal projection.
pub const LEGAL_ENRICHMENT_TABLE: &str = "legal_chunk_enrichment";

/// Arrow schema for the chunk-level legal projection.
#[must_use]
pub fn legal_enrichment_schema() -> Arc<Schema> {
    let utf8_list = || DataType::List(Arc::new(Field::new("item", DataType::Utf8, true)));
    let i64_list = || DataType::List(Arc::new(Field::new("item", DataType::Int64, true)));
    let ts_list = || {
        DataType::List(Arc::new(Field::new(
            "item",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        )))
    };

    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_type", DataType::Utf8, true),
        Field::new("legal_domain", DataType::Utf8, true),
        Field::new("jurisdiction", DataType::Utf8, true),
        Field::new(
            "document_date",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new("dossier_id", DataType::Utf8, true),
        Field::new("parties", utf8_list(), false),
        Field::new("party_roles", utf8_list(), false),
        Field::new("legal_refs", utf8_list(), false),
        Field::new("clause_types", utf8_list(), false),
        Field::new("obligation_kinds", utf8_list(), false),
        Field::new("amounts_eur_cents", i64_list(), false),
        Field::new("deadlines", ts_list(), false),
        Field::new("event_kinds", utf8_list(), false),
        Field::new("risk_flags", utf8_list(), false),
        Field::new("mandatory_clause_status", DataType::Utf8, true),
        Field::new("confidence_min", DataType::Float32, false),
        Field::new("confidence_avg", DataType::Float32, false),
        Field::new("extractor_version", DataType::Utf8, false),
        Field::new("model_id", DataType::Utf8, false),
    ]))
}

pub(crate) fn sql_string_lit(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// LanceDB handle for legal enrichment.
#[derive(Clone)]
pub struct LegalStore {
    table: Table,
}

impl LegalStore {
    /// Open or create the legal enrichment table.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] on invalid paths or LanceDB failures.
    pub async fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        let path = cfg.index_path();
        let uri = path
            .to_str()
            .ok_or_else(|| Error::Legal(format!("non-utf8 index path: {}", path.display())))?;
        let conn: Connection = lancedb::connect(uri)
            .execute()
            .await
            .map_err(|err| Error::Legal(format!("legal_store connect: {err}")))?;
        let names = conn
            .table_names()
            .execute()
            .await
            .map_err(|err| Error::Legal(format!("legal_store list: {err}")))?;
        let schema = legal_enrichment_schema();
        let table = if names.iter().any(|name| name == LEGAL_ENRICHMENT_TABLE) {
            conn.open_table(LEGAL_ENRICHMENT_TABLE).execute().await
        } else {
            let empty = RecordBatchIterator::new(std::iter::empty(), schema);
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(LEGAL_ENRICHMENT_TABLE, reader)
                .execute()
                .await
        }
        .map_err(|err| Error::Legal(format!("legal_store open: {err}")))?;

        Ok(Self { table })
    }

    /// Build a SQL predicate from filters.
    #[must_use]
    pub fn filter_sql(filters: &LegalSearchFilters) -> Option<String> {
        let mut clauses = Vec::new();

        if let Some(value) = &filters.doc_type {
            clauses.push(format!("doc_type = {}", sql_string_lit(value)));
        }
        if let Some(value) = &filters.legal_domain {
            clauses.push(format!("legal_domain = {}", sql_string_lit(value)));
        }
        if let Some(value) = &filters.jurisdiction {
            clauses.push(format!("jurisdiction = {}", sql_string_lit(value)));
        }
        if let Some(value) = &filters.dossier_id {
            clauses.push(format!("dossier_id = {}", sql_string_lit(value)));
        }
        if let Some(value) = &filters.mandatory_clause_status {
            clauses.push(format!(
                "mandatory_clause_status = {}",
                sql_string_lit(value)
            ));
        }

        push_array_contains(&mut clauses, "parties", &filters.parties);
        push_array_contains(&mut clauses, "party_roles", &filters.party_roles);
        push_array_contains(&mut clauses, "legal_refs", &filters.legal_refs);
        push_array_contains(&mut clauses, "clause_types", &filters.clause_types);
        push_array_contains(&mut clauses, "obligation_kinds", &filters.obligation_kinds);
        push_array_contains(&mut clauses, "event_kinds", &filters.event_kinds);
        push_array_contains(&mut clauses, "risk_flags", &filters.risk_flags);

        if let Some(date_from) = filters.date_from {
            clauses.push(format!(
                "document_date >= CAST({} AS TIMESTAMP)",
                date_from.timestamp_micros()
            ));
        }
        if let Some(date_to) = filters.date_to {
            clauses.push(format!(
                "document_date <= CAST({} AS TIMESTAMP)",
                date_to.timestamp_micros()
            ));
        }
        if let Some(confidence) = filters.min_confidence {
            clauses.push(format!("confidence_min >= {confidence}"));
        }

        if clauses.is_empty() {
            None
        } else {
            Some(clauses.join(" AND "))
        }
    }

    /// Build the LanceDB scalar indexes required by the filter path.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when LanceDB index listing or creation fails.
    pub async fn setup_indexes(&self) -> Result<()> {
        use lancedb::index::scalar::{
            BTreeIndexBuilder, BitmapIndexBuilder, LabelListIndexBuilder,
        };
        use lancedb::index::Index;

        let existing = self
            .table
            .list_indices()
            .await
            .map_err(|err| Error::Legal(format!("list_indices: {err}")))?;
        let has_index_on = |column: &str| {
            existing
                .iter()
                .any(|index| index.columns.iter().any(|candidate| candidate == column))
        };

        for column in ["chunk_id", "doc_id", "document_date"] {
            if !has_index_on(column) {
                self.table
                    .create_index(&[column], Index::BTree(BTreeIndexBuilder::default()))
                    .execute()
                    .await
                    .map_err(|err| Error::Legal(format!("btree {column}: {err}")))?;
            }
        }
        for column in [
            "doc_type",
            "legal_domain",
            "jurisdiction",
            "dossier_id",
            "mandatory_clause_status",
        ] {
            if !has_index_on(column) {
                self.table
                    .create_index(&[column], Index::Bitmap(BitmapIndexBuilder::default()))
                    .execute()
                    .await
                    .map_err(|err| Error::Legal(format!("bitmap {column}: {err}")))?;
            }
        }
        for column in [
            "parties",
            "party_roles",
            "legal_refs",
            "clause_types",
            "obligation_kinds",
            "deadlines",
            "event_kinds",
            "risk_flags",
        ] {
            if !has_index_on(column) {
                self.table
                    .create_index(
                        &[column],
                        Index::LabelList(LabelListIndexBuilder::default()),
                    )
                    .execute()
                    .await
                    .map_err(|err| Error::Legal(format!("label_list {column}: {err}")))?;
            }
        }

        Ok(())
    }

    /// Upsert a batch of enrichment rows.
    ///
    /// # Errors
    /// Currently never errors; Arrow conversion lands with ingest wiring.
    pub async fn upsert(&self, rows: &[LegalChunkEnrichment]) -> Result<()> {
        let _ = rows;
        Ok(())
    }

    /// Delete all rows belonging to one document id.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when LanceDB deletion fails.
    pub async fn delete_doc(&self, doc_id: Uuid) -> Result<()> {
        let id_hex = hex::encode(doc_id.as_bytes());
        self.table
            .delete(&format!("doc_id = X'{id_hex}'"))
            .await
            .map_err(|err| Error::Legal(format!("delete_doc: {err}")))?;
        Ok(())
    }

    /// Return candidate chunk ids matching the filters.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when query execution or UUID decoding fails.
    pub async fn filter_chunk_ids(
        &self,
        filters: &LegalSearchFilters,
        limit: usize,
    ) -> Result<Vec<Uuid>> {
        use lancedb::query::{ExecutableQuery, QueryBase, Select};

        if !filters.has_any_filter() {
            return Ok(Vec::new());
        }

        let filter =
            Self::filter_sql(filters).expect("has_any_filter true means filter_sql is some");
        let stream = self
            .table
            .query()
            .select(Select::columns(&["chunk_id"]))
            .only_if(filter)
            .limit(limit)
            .execute()
            .await
            .map_err(|err| Error::Legal(format!("filter_chunk_ids: {err}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|err| Error::Legal(format!("filter_chunk_ids stream: {err}")))?;

        let mut out = Vec::new();
        for batch in batches {
            let arr = batch
                .column_by_name("chunk_id")
                .ok_or_else(|| Error::Legal("missing chunk_id column".into()))?
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .ok_or_else(|| Error::Legal("chunk_id wrong type".into()))?;
            for idx in 0..arr.len() {
                if arr.is_null(idx) {
                    continue;
                }
                out.push(
                    Uuid::from_slice(arr.value(idx))
                        .map_err(|err| Error::Legal(format!("chunk uuid: {err}")))?,
                );
            }
        }

        Ok(out)
    }
}

fn push_array_contains(clauses: &mut Vec<String>, column: &str, values: &[String]) {
    for value in values {
        clauses.push(format!(
            "array_contains({column}, {})",
            sql_string_lit(value)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_all_required_filter_columns() {
        let schema = legal_enrichment_schema();
        let names: Vec<&str> = schema
            .fields()
            .iter()
            .map(|field| field.name().as_str())
            .collect();
        for expected in [
            "chunk_id",
            "doc_id",
            "doc_type",
            "legal_domain",
            "jurisdiction",
            "document_date",
            "dossier_id",
            "parties",
            "party_roles",
            "legal_refs",
            "clause_types",
            "obligation_kinds",
            "amounts_eur_cents",
            "deadlines",
            "event_kinds",
            "risk_flags",
            "mandatory_clause_status",
            "confidence_min",
            "confidence_avg",
            "extractor_version",
            "model_id",
        ] {
            assert!(names.contains(&expected), "missing column {expected}");
        }
    }

    #[test]
    fn sql_string_lit_escapes_single_quotes() {
        assert_eq!(sql_string_lit("l'avocat"), "'l''avocat'");
    }

    #[test]
    fn filter_sql_combines_all_clauses_with_and() {
        let filters = LegalSearchFilters {
            doc_type: Some("contract".into()),
            parties: vec!["org:acme".into()],
            risk_flags: vec!["overdue_obligation".into()],
            ..LegalSearchFilters::default()
        };
        let sql = LegalStore::filter_sql(&filters).expect("filter");
        assert!(sql.contains("doc_type = 'contract'"));
        assert!(sql.contains("array_contains(parties, 'org:acme')"));
        assert!(sql.contains("array_contains(risk_flags, 'overdue_obligation')"));
        assert_eq!(sql.matches(" AND ").count(), 2);
    }
}

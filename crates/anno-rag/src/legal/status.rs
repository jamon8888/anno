//! LanceDB sidecar table tracking dual-write retry state.

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use arrow_array::{
    Array, FixedSizeBinaryArray, Int32Array, RecordBatch, RecordBatchIterator, StringArray,
    TimestampMicrosecondArray,
};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::{Connection, Table};
use std::sync::Arc;
use uuid::Uuid;

/// LanceDB table name for legal enrichment retry status rows.
pub const ENRICHMENT_STATUS_TABLE: &str = "enrichment_status";

/// Legal enrichment retry status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichmentStatusKind {
    /// Enrichment failed transiently and should be retried.
    Pending,
    /// Enrichment completed successfully.
    Ok,
    /// Enrichment exhausted the retry budget.
    FailedMaxRetries,
}

impl EnrichmentStatusKind {
    /// Stable storage representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            EnrichmentStatusKind::Pending => "pending",
            EnrichmentStatusKind::Ok => "ok",
            EnrichmentStatusKind::FailedMaxRetries => "failed_max_retries",
        }
    }
}

/// Arrow schema for the enrichment status sidecar table.
#[must_use]
pub fn enrichment_status_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("status", DataType::Utf8, false),
        Field::new("attempts", DataType::Int32, false),
        Field::new("last_error", DataType::Utf8, true),
        Field::new(
            "last_attempt_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("chunk_count", DataType::Int32, false),
    ]))
}

/// Status store handle.
#[derive(Clone)]
pub struct EnrichmentStatusStore {
    table: Table,
}

impl EnrichmentStatusStore {
    /// Open or create the enrichment status table.
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
            .map_err(|err| Error::Legal(format!("status connect: {err}")))?;
        let names = conn
            .table_names()
            .execute()
            .await
            .map_err(|err| Error::Legal(format!("status list: {err}")))?;
        let schema = enrichment_status_schema();
        let table = if names.iter().any(|name| name == ENRICHMENT_STATUS_TABLE) {
            conn.open_table(ENRICHMENT_STATUS_TABLE).execute().await
        } else {
            let empty = RecordBatchIterator::new(std::iter::empty(), schema);
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(ENRICHMENT_STATUS_TABLE, reader)
                .execute()
                .await
        }
        .map_err(|err| Error::Legal(format!("status open: {err}")))?;

        Ok(Self { table })
    }

    /// Upsert a `pending` row for a transient enrichment failure.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when reading or writing status fails.
    pub async fn mark_pending(&self, doc_id: Uuid, chunk_count: i32, err: &str) -> Result<()> {
        let attempts = self.attempts(doc_id).await?.unwrap_or(0) + 1;
        self.upsert_row(
            doc_id,
            EnrichmentStatusKind::Pending,
            attempts,
            Some(err.to_string()),
            chunk_count,
        )
        .await
    }

    /// Mark the document as successfully enriched.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when reading or writing status fails.
    pub async fn mark_ok(&self, doc_id: Uuid) -> Result<()> {
        let attempts = self.attempts(doc_id).await?.unwrap_or(0);
        self.upsert_row(doc_id, EnrichmentStatusKind::Ok, attempts, None, 0)
            .await
    }

    /// Mark the document as permanently failed after retry exhaustion.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when reading or writing status fails.
    pub async fn mark_failed_max_retries(&self, doc_id: Uuid, err: &str) -> Result<()> {
        let attempts = self.attempts(doc_id).await?.unwrap_or(0);
        self.upsert_row(
            doc_id,
            EnrichmentStatusKind::FailedMaxRetries,
            attempts,
            Some(err.to_string()),
            0,
        )
        .await
    }

    /// List pending documents sorted by oldest attempt first.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when query execution or decoding fails.
    pub async fn list_pending(&self, max: usize) -> Result<Vec<PendingDoc>> {
        use lancedb::query::{ExecutableQuery, QueryBase};

        let stream = self
            .table
            .query()
            .only_if("status = 'pending'".to_string())
            .limit(max)
            .execute()
            .await
            .map_err(|err| Error::Legal(format!("list_pending: {err}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|err| Error::Legal(format!("list_pending stream: {err}")))?;

        let mut out = Vec::new();
        for batch in batches {
            let id_col = required_col::<FixedSizeBinaryArray>(&batch, "doc_id")?;
            let attempts_col = required_col::<Int32Array>(&batch, "attempts")?;
            let error_col = batch
                .column_by_name("last_error")
                .and_then(|column| column.as_any().downcast_ref::<StringArray>());
            let ts_col = required_col::<TimestampMicrosecondArray>(&batch, "last_attempt_at")?;
            let chunk_count_col = required_col::<Int32Array>(&batch, "chunk_count")?;

            for idx in 0..batch.num_rows() {
                let last_attempt_at = DateTime::<Utc>::from_timestamp_micros(ts_col.value(idx))
                    .ok_or_else(|| Error::Legal(format!("invalid last_attempt_at at row {idx}")))?;
                out.push(PendingDoc {
                    doc_id: Uuid::from_slice(id_col.value(idx))
                        .map_err(|err| Error::Legal(format!("pending doc uuid: {err}")))?,
                    attempts: attempts_col.value(idx),
                    last_error: error_col.and_then(|column| {
                        (!column.is_null(idx)).then(|| column.value(idx).to_string())
                    }),
                    last_attempt_at,
                    chunk_count: chunk_count_col.value(idx),
                });
            }
        }
        out.sort_by_key(|doc| doc.last_attempt_at);
        Ok(out)
    }

    async fn attempts(&self, doc_id: Uuid) -> Result<Option<i32>> {
        use lancedb::query::{ExecutableQuery, QueryBase, Select};
        let id_hex = hex::encode(doc_id.as_bytes());
        let stream = self
            .table
            .query()
            .only_if(format!("doc_id = X'{id_hex}'"))
            .select(Select::columns(&["attempts"]))
            .limit(1)
            .execute()
            .await
            .map_err(|e| Error::Legal(format!("attempts query: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Legal(format!("attempts stream: {e}")))?;
        for batch in &batches {
            if batch.num_rows() > 0 {
                let col = required_col::<Int32Array>(batch, "attempts")?;
                return Ok(Some(col.value(0)));
            }
        }
        Ok(None)
    }

    async fn upsert_row(
        &self,
        doc_id: Uuid,
        status: EnrichmentStatusKind,
        attempts: i32,
        last_error: Option<String>,
        chunk_count: i32,
    ) -> Result<()> {
        use arrow_array::builder::FixedSizeBinaryBuilder;

        let now_micros = chrono::Utc::now().timestamp_micros();
        let mut doc_id_b = FixedSizeBinaryBuilder::with_capacity(1, 16);
        doc_id_b
            .append_value(doc_id.as_bytes())
            .map_err(Error::Arrow)?;
        let status_arr = StringArray::from(vec![status.as_str()]);
        let attempts_arr = Int32Array::from(vec![attempts]);
        let last_error_arr = StringArray::from(vec![last_error.as_deref()]);
        let ts_arr = TimestampMicrosecondArray::from(vec![now_micros]);
        let chunk_count_arr = Int32Array::from(vec![chunk_count]);

        let schema = enrichment_status_schema();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(doc_id_b.finish()),
                Arc::new(status_arr),
                Arc::new(attempts_arr),
                Arc::new(last_error_arr),
                Arc::new(ts_arr),
                Arc::new(chunk_count_arr),
            ],
        )
        .map_err(Error::Arrow)?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(reader);
        self.table
            .add(reader)
            .execute()
            .await
            .map_err(|e| Error::Legal(format!("upsert_row: {e}")))?;
        Ok(())
    }
}

fn required_col<'a, T: 'static>(batch: &'a RecordBatch, name: &str) -> Result<&'a T> {
    batch
        .column_by_name(name)
        .ok_or_else(|| Error::Legal(format!("missing {name} column")))?
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| Error::Legal(format!("{name} wrong type")))
}

/// Pending enrichment retry row.
#[derive(Debug, Clone)]
pub struct PendingDoc {
    /// Document UUID.
    pub doc_id: Uuid,
    /// Number of enrichment attempts already recorded.
    pub attempts: i32,
    /// Last error message, if any.
    pub last_error: Option<String>,
    /// Last attempt timestamp.
    pub last_attempt_at: DateTime<Utc>,
    /// Chunk count recorded for retry work.
    pub chunk_count: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_required_columns() {
        let schema = enrichment_status_schema();
        let names: Vec<&str> = schema
            .fields()
            .iter()
            .map(|field| field.name().as_str())
            .collect();
        for expected in [
            "doc_id",
            "status",
            "attempts",
            "last_error",
            "last_attempt_at",
            "chunk_count",
        ] {
            assert!(names.contains(&expected), "missing column {expected}");
        }
    }

    #[test]
    fn status_kind_as_str_is_stable() {
        assert_eq!(EnrichmentStatusKind::Pending.as_str(), "pending");
        assert_eq!(EnrichmentStatusKind::Ok.as_str(), "ok");
        assert_eq!(
            EnrichmentStatusKind::FailedMaxRetries.as_str(),
            "failed_max_retries"
        );
    }
}

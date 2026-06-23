//! Async ingestion job tracking on the `index_jobs` table.

use crate::control_store::KnowledgeControlStore;
use crate::Result;
use chrono::Utc;
use rusqlite::params;

/// Lifecycle state of an ingestion job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// In-flight.
    Running,
    /// Completed successfully (may have per-file failures recorded).
    Done,
    /// Process died mid-job; needs a re-run.
    Interrupted,
    /// Zero files succeeded.
    Failed,
}

impl JobStatus {
    /// Serialized form stored in SQLite.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }
}

/// A row from `index_jobs`.
#[derive(Debug, Clone)]
pub struct JobRow {
    /// Job id (caller-generated UUID string).
    pub job_id: String,
    /// Job family, e.g. `legal_ingest`.
    pub job_type: String,
    /// Owning corpus id (string form), if any.
    pub corpus_id: Option<String>,
    /// Status string (`running`/`done`/`interrupted`/`failed`).
    pub status: String,
    /// Files processed so far.
    pub files_done: i64,
    /// Total files discovered for the job.
    pub files_total: i64,
    /// Last non-fatal error, if any.
    pub last_error: Option<String>,
}

impl KnowledgeControlStore {
    /// Insert a new `running` job row.
    pub fn insert_job(
        &self,
        job_id: &str,
        job_type: &str,
        corpus_id: Option<&str>,
        files_total: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        conn.execute(
            "INSERT INTO index_jobs \
             (job_id, job_type, corpus_id, status, attempts, files_done, files_total, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'running', 0, 0, ?4, ?5, ?5)",
            params![job_id, job_type, corpus_id, files_total, now],
        )?;
        Ok(())
    }

    /// Return the `job_id` of a `running` job for `corpus_id`, if one exists.
    pub fn running_job_for_corpus(&self, corpus_id: &str) -> Result<Option<String>> {
        let conn = self.conn_lock();
        let mut stmt = conn.prepare(
            "SELECT job_id FROM index_jobs \
             WHERE corpus_id = ?1 AND status = 'running' LIMIT 1",
        )?;
        let mut rows = stmt.query(params![corpus_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get::<_, String>(0)?)),
            None => Ok(None),
        }
    }

    /// Update `files_done` for an in-flight job.
    pub fn update_job_progress(&self, job_id: &str, files_done: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        conn.execute(
            "UPDATE index_jobs SET files_done = ?2, updated_at = ?3 WHERE job_id = ?1",
            params![job_id, files_done, now],
        )?;
        Ok(())
    }

    /// Set terminal (or any) status, optionally recording `last_error`.
    pub fn set_job_status(
        &self,
        job_id: &str,
        status: JobStatus,
        last_error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        conn.execute(
            "UPDATE index_jobs SET status = ?2, last_error = ?3, updated_at = ?4 WHERE job_id = ?1",
            params![job_id, status.as_str(), last_error, now],
        )?;
        Ok(())
    }

    /// Fetch a single job row.
    pub fn get_job(&self, job_id: &str) -> Result<Option<JobRow>> {
        let conn = self.conn_lock();
        let mut stmt = conn.prepare(
            "SELECT job_id, job_type, corpus_id, status, files_done, files_total, last_error \
             FROM index_jobs WHERE job_id = ?1",
        )?;
        let mut rows = stmt.query(params![job_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(JobRow {
                job_id: row.get(0)?,
                job_type: row.get(1)?,
                corpus_id: row.get(2)?,
                status: row.get(3)?,
                files_done: row.get(4)?,
                files_total: row.get(5)?,
                last_error: row.get(6)?,
            })),
            None => Ok(None),
        }
    }

    /// Mark every `running` job `interrupted`. Run once at startup to recover
    /// from a process that died mid-ingest. Returns the number of rows changed.
    pub fn mark_running_jobs_interrupted(&self) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        let changed = conn.execute(
            "UPDATE index_jobs SET status = 'interrupted', updated_at = ?1 WHERE status = 'running'",
            params![now],
        )?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> KnowledgeControlStore {
        let dir = tempdir().expect("temp dir");
        KnowledgeControlStore::open(dir.path().join("k.sqlite3")).expect("open")
    }

    #[test]
    fn job_lifecycle_running_to_done() {
        let s = store();
        s.insert_job("job-1", "legal_ingest", Some("corp-1"), 3)
            .expect("insert");
        assert_eq!(
            s.running_job_for_corpus("corp-1").expect("q"),
            Some("job-1".into())
        );

        s.update_job_progress("job-1", 2).expect("progress");
        s.set_job_status("job-1", JobStatus::Done, None)
            .expect("done");

        let row = s.get_job("job-1").expect("get").expect("some");
        assert_eq!(row.status, "done");
        assert_eq!(row.files_done, 2);
        assert_eq!(row.files_total, 3);
        assert_eq!(s.running_job_for_corpus("corp-1").expect("q"), None);
    }

    #[test]
    fn interrupted_sweep_marks_running() {
        let s = store();
        s.insert_job("job-2", "legal_ingest", Some("corp-2"), 1)
            .expect("insert");
        let changed = s.mark_running_jobs_interrupted().expect("sweep");
        assert_eq!(changed, 1);
        assert_eq!(
            s.get_job("job-2").expect("get").expect("some").status,
            "interrupted"
        );
    }
}

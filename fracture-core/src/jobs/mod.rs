use std::collections::HashMap;
use std::error::Error;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::listing::FormField;
use crate::models::_entities::{job_definitions, job_runs};

pub mod permissions;
pub mod runner;

pub use permissions::{JobAccess, JobAccessLevel, JobPermissions};

/// The result of executing a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub summary: serde_json::Value,
    pub diffs: Vec<JobDiff>,
}

/// A single diff produced by a job execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDiff {
    pub diff_type: String,
    pub entity_key: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}

/// Trait that job executors must implement.
#[async_trait::async_trait]
pub trait JobExecutor: Send + Sync {
    /// Returns the job type identifier for this executor.
    fn job_type(&self) -> &str;

    /// Human-readable name shown in the job-creation picker (defaults to the
    /// job type identifier).
    fn label(&self) -> &str {
        self.job_type()
    }

    /// One-line description shown in the picker (empty by default).
    fn description(&self) -> &'static str {
        ""
    }

    /// Declares the friendly config form for this job type, so org owners get
    /// real inputs (a project dropdown, a title field, …) instead of raw JSON.
    /// The form is built per-request with `db`/`org_id` so options can be
    /// dynamic (e.g. the org's projects). The empty default means "no custom
    /// form" — the create UI then falls back to a raw JSON config textarea.
    ///
    /// Submitted field values are collected into the definition's `config`
    /// JSON object under each field's `name`, which `execute` reads back.
    ///
    /// # Errors
    ///
    /// Returns an error if building the form needs the database and it fails.
    async fn config_form(
        &self,
        _db: &sea_orm::DatabaseConnection,
        _org_id: i32,
    ) -> Result<Vec<FormField>, sea_orm::DbErr> {
        Ok(Vec::new())
    }

    /// Executes the job, given the definition and optionally the previous run.
    async fn execute(
        &self,
        db: &sea_orm::DatabaseConnection,
        definition: &job_definitions::Model,
        previous_run: Option<&job_runs::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>>;
}

/// Display metadata for a registered job type, for the creation picker.
#[derive(Debug, Clone, Serialize)]
pub struct JobTypeInfo {
    pub job_type: String,
    pub label: String,
    pub description: String,
}

/// Registry for mapping job types to their executors.
pub struct JobRegistry {
    executors: HashMap<String, Box<dyn JobExecutor>>,
}

impl JobRegistry {
    /// Creates a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
        }
    }

    /// Registers a job executor for its declared job type.
    pub fn register(&mut self, executor: Box<dyn JobExecutor>) {
        let job_type = executor.job_type().to_string();
        self.executors.insert(job_type, executor);
    }

    /// Retrieves an executor by job type.
    #[must_use]
    pub fn get(&self, job_type: &str) -> Option<&dyn JobExecutor> {
        self.executors.get(job_type).map(AsRef::as_ref)
    }

    /// Returns the registered job type identifiers, sorted. Used by the UI
    /// to offer valid choices when creating a job definition.
    #[must_use]
    pub fn job_types(&self) -> Vec<&str> {
        let mut types: Vec<&str> = self.executors.keys().map(String::as_str).collect();
        types.sort_unstable();
        types
    }

    /// Returns display metadata for every registered job type, sorted by label.
    /// Drives the friendly "Create a job" picker.
    #[must_use]
    pub fn job_type_infos(&self) -> Vec<JobTypeInfo> {
        let mut infos: Vec<JobTypeInfo> = self
            .executors
            .values()
            .map(|e| JobTypeInfo {
                job_type: e.job_type().to_string(),
                label: e.label().to_string(),
                description: e.description().to_string(),
            })
            .collect();
        infos.sort_by(|a, b| a.label.cmp(&b.label));
        infos
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// --- Global registry access ---

static JOB_REGISTRY: OnceLock<JobRegistry> = OnceLock::new();

/// Initialises the global job registry. Safe to call multiple times
/// (only the first call takes effect).
pub fn init_job_registry(registry: JobRegistry) {
    JOB_REGISTRY.get_or_init(|| registry);
}

/// Returns a reference to the global job registry.
///
/// # Panics
///
/// Panics if `init_job_registry` has not been called.
#[must_use]
pub fn job_registry() -> &'static JobRegistry {
    JOB_REGISTRY
        .get()
        .expect("JobRegistry not initialised — call init_job_registry() first")
}

/// Returns the global job registry if it has been initialised.
#[must_use]
pub fn try_job_registry() -> Option<&'static JobRegistry> {
    JOB_REGISTRY.get()
}

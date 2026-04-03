use std::collections::HashMap;
use std::error::Error;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::models::_entities::{job_definitions, job_runs};

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

    /// Executes the job, given the definition and optionally the previous run.
    async fn execute(
        &self,
        db: &sea_orm::DatabaseConnection,
        definition: &job_definitions::Model,
        previous_run: Option<&job_runs::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>>;
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

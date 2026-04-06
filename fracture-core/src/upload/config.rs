use serde::Deserialize;

/// Configuration for the upload subsystem, deserialized from `settings.uploads`.
#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    /// Maximum size of a single file in bytes (default: 5 MiB).
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,

    /// Maximum total upload size per request in bytes (default: 20 MiB).
    #[serde(default = "default_max_total_size")]
    pub max_total_size: u64,

    /// Root directory for file storage (default: `/app/data/uploads`).
    #[serde(default = "default_storage_root")]
    pub storage_root: String,

    /// Allowed MIME content types.
    #[serde(default = "default_allowed_types")]
    pub allowed_types: Vec<String>,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_file_size: default_max_file_size(),
            max_total_size: default_max_total_size(),
            storage_root: default_storage_root(),
            allowed_types: default_allowed_types(),
        }
    }
}

const fn default_max_file_size() -> u64 {
    5 * 1024 * 1024 // 5 MiB
}

const fn default_max_total_size() -> u64 {
    20 * 1024 * 1024 // 20 MiB
}

fn default_storage_root() -> String {
    "/app/data/uploads".to_string()
}

fn default_allowed_types() -> Vec<String> {
    vec![
        "image/png".to_string(),
        "image/jpeg".to_string(),
        "image/gif".to_string(),
        "image/webp".to_string(),
        "image/svg+xml".to_string(),
    ]
}

impl UploadConfig {
    /// Reads upload config from the application settings.
    /// Falls back to defaults if the `uploads` key is missing.
    pub fn from_settings(settings: Option<&serde_json::Value>) -> Self {
        settings
            .and_then(|s| s.get("uploads"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

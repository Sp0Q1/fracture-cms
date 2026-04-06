use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::models::_entities::uploads;

use super::config::UploadConfig;
use super::storage::{FilesystemBackend, StorageBackend, StorageError};
use super::validate::{ValidatedFile, ValidationError, ValidationPipeline};

/// Errors from the upload service.
#[derive(Debug, Error)]
pub enum UploadError {
    #[error("file too large: {size} bytes exceeds limit of {limit} bytes")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("validation failed: {0}")]
    Validation(#[from] ValidationError),

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

/// Result of a successful upload operation.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// The public UUID of the upload.
    pub pid: Uuid,
    /// The validated content type.
    pub content_type: String,
    /// The file size in bytes.
    pub size_bytes: u64,
    /// The SHA-256 checksum of the stored file.
    pub checksum_sha256: String,
}

/// Orchestrates file upload: validation, storage, and database record creation.
pub struct UploadService {
    config: UploadConfig,
    storage: FilesystemBackend,
    pipeline: ValidationPipeline,
}

impl UploadService {
    /// Creates a new upload service with the given config.
    ///
    /// # Errors
    ///
    /// Returns a `StorageError` if the storage backend cannot be initialized.
    pub async fn new(config: UploadConfig) -> Result<Self, StorageError> {
        let storage = FilesystemBackend::new(&config.storage_root).await?;
        let pipeline = ValidationPipeline::new(config.allowed_types.clone());
        Ok(Self {
            config,
            storage,
            pipeline,
        })
    }

    /// Processes an uploaded file through the full pipeline:
    /// 1. Size check
    /// 2. Validation (extension, content-type, magic bytes, SVG sanitization)
    /// 3. Generate UUID-based storage path
    /// 4. Compute SHA-256 checksum
    /// 5. Store on disk
    /// 6. Insert database record
    /// 7. Return result
    ///
    /// # Errors
    ///
    /// Returns an `UploadError` if the file fails validation, storage, or DB insert.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload(
        &self,
        db: &DatabaseConnection,
        org_id: i32,
        uploaded_by: i32,
        original_name: &str,
        declared_content_type: &str,
        data: Vec<u8>,
        visibility: &str,
    ) -> Result<UploadResult, UploadError> {
        // Step 1: Size check
        let size = data.len() as u64;
        if size > self.config.max_file_size {
            return Err(UploadError::FileTooLarge {
                size,
                limit: self.config.max_file_size,
            });
        }

        // Step 2: Validate
        let ValidatedFile {
            content_type,
            extension,
            clean_data,
        } = self
            .pipeline
            .validate(original_name, declared_content_type, data)?;

        // Step 3: Generate UUID-based storage path
        let file_uuid = Uuid::new_v4();
        let relative_path = format!(
            "{}/{}/{}.{}",
            &file_uuid.to_string()[..2],
            &file_uuid.to_string()[2..4],
            file_uuid,
            extension,
        );

        // Step 4: Compute SHA-256 checksum
        let mut hasher = Sha256::new();
        hasher.update(&clean_data);
        let hash_bytes = hasher.finalize();
        let checksum = hash_bytes
            .iter()
            .fold(String::with_capacity(64), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            });

        // Step 5: Store on disk
        let storage_path = self.storage.store(&relative_path, &clean_data).await?;

        // Step 6: Insert database record
        let size_bytes = i64::try_from(clean_data.len()).unwrap_or(0);
        let active_model = uploads::ActiveModel {
            org_id: sea_orm::ActiveValue::Set(org_id),
            uploaded_by: sea_orm::ActiveValue::Set(uploaded_by),
            original_name: sea_orm::ActiveValue::Set(
                original_name.chars().take(255).collect::<String>(),
            ),
            storage_path: sea_orm::ActiveValue::Set(storage_path),
            content_type: sea_orm::ActiveValue::Set(content_type.clone()),
            size_bytes: sea_orm::ActiveValue::Set(size_bytes),
            visibility: sea_orm::ActiveValue::Set(visibility.to_string()),
            checksum_sha256: sea_orm::ActiveValue::Set(checksum.clone()),
            ..Default::default()
        };

        let model =
            <uploads::ActiveModel as sea_orm::ActiveModelTrait>::insert(active_model, db).await?;

        // Step 7: Return result
        Ok(UploadResult {
            pid: model.pid,
            content_type,
            size_bytes: u64::try_from(size_bytes).unwrap_or(0),
            checksum_sha256: checksum,
        })
    }

    /// Reads the file data for a given upload record.
    ///
    /// # Errors
    ///
    /// Returns a `StorageError` if the file cannot be read from storage.
    pub async fn read_file(&self, upload: &uploads::Model) -> Result<Vec<u8>, StorageError> {
        // The storage_path is absolute; we need the relative portion.
        // Derive relative path from the stored absolute path by stripping the root.
        let root = &self.config.storage_root;
        let relative = upload
            .storage_path
            .strip_prefix(root)
            .unwrap_or(&upload.storage_path)
            .trim_start_matches('/');
        self.storage.read(relative).await
    }

    /// Deletes the file and returns Ok. The caller is responsible for deleting the DB record.
    ///
    /// # Errors
    ///
    /// Returns a `StorageError` if the file cannot be deleted from storage.
    pub async fn delete_file(&self, upload: &uploads::Model) -> Result<(), StorageError> {
        let root = &self.config.storage_root;
        let relative = upload
            .storage_path
            .strip_prefix(root)
            .unwrap_or(&upload.storage_path)
            .trim_start_matches('/');
        self.storage.delete(relative).await
    }

    /// Returns the configured maximum file size.
    #[must_use]
    pub const fn max_file_size(&self) -> u64 {
        self.config.max_file_size
    }
}

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path traversal detected")]
    PathTraversal,

    #[error("storage root does not exist and could not be created")]
    RootCreationFailed,
}

/// Trait for pluggable storage backends.
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Stores `data` at the given relative `path` within the storage root.
    /// Returns the canonicalized absolute path on success.
    async fn store(&self, relative_path: &str, data: &[u8]) -> Result<String, StorageError>;

    /// Reads the file at the given relative `path` within the storage root.
    async fn read(&self, relative_path: &str) -> Result<Vec<u8>, StorageError>;

    /// Deletes the file at the given relative `path` within the storage root.
    async fn delete(&self, relative_path: &str) -> Result<(), StorageError>;
}

/// Filesystem-backed storage implementation.
#[derive(Debug, Clone)]
pub struct FilesystemBackend {
    root: PathBuf,
}

impl FilesystemBackend {
    /// Creates a new filesystem backend rooted at `root`.
    /// Creates the root directory if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns a `StorageError` if the root directory cannot be created or canonicalized.
    pub async fn new(root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = root.into();
        tokio::fs::create_dir_all(&root)
            .await
            .map_err(|_| StorageError::RootCreationFailed)?;
        // Canonicalize after creation to get the real path
        let root = tokio::fs::canonicalize(&root)
            .await
            .map_err(|_| StorageError::RootCreationFailed)?;
        Ok(Self { root })
    }

    /// Resolves a relative path against the storage root, preventing traversal.
    fn resolve(&self, relative_path: &str) -> Result<PathBuf, StorageError> {
        let path = self.root.join(relative_path);

        // Normalize the path components to detect traversal attempts.
        // We check that no component is ".." after joining.
        for component in path.components() {
            if component == std::path::Component::ParentDir {
                return Err(StorageError::PathTraversal);
            }
        }

        // Verify the resolved path is still under our root
        if !path.starts_with(&self.root) {
            return Err(StorageError::PathTraversal);
        }

        Ok(path)
    }
}

#[async_trait::async_trait]
impl StorageBackend for FilesystemBackend {
    async fn store(&self, relative_path: &str, data: &[u8]) -> Result<String, StorageError> {
        let path = self.resolve(relative_path)?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&path, data).await?;

        // Canonicalize after write to verify the final path is still under root
        let canonical = tokio::fs::canonicalize(&path).await?;
        if !canonical.starts_with(&self.root) {
            // Clean up the file we just wrote and reject
            let _ = tokio::fs::remove_file(&canonical).await;
            return Err(StorageError::PathTraversal);
        }

        Ok(canonical.to_string_lossy().into_owned())
    }

    async fn read(&self, relative_path: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.resolve(relative_path)?;
        let data = tokio::fs::read(&path).await?;
        Ok(data)
    }

    async fn delete(&self, relative_path: &str) -> Result<(), StorageError> {
        let path = self.resolve(relative_path)?;
        tokio::fs::remove_file(&path).await?;
        Ok(())
    }
}

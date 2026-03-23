//! Content-addressed file storage.
//!
//! Files are stored by their SHA-256 hash, providing natural deduplication.
//! The [`FileStore`] trait abstracts over storage backends (filesystem, SQLite BLOB, etc.).

use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::error::Result;

/// Compute the SHA-256 hash of file bytes, returned as a hex string.
pub fn compute_file_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Content-addressed file storage interface.
///
/// Implementations store raw bytes keyed by SHA-256 hash. Duplicate writes
/// (same hash) are no-ops. Used for media attachments (images, PDFs, etc.).
#[async_trait::async_trait]
pub trait FileStore: Send + Sync {
    /// Store file bytes. Returns the content-addressed SHA-256 hash.
    /// If a file with the same hash already exists, this is a no-op.
    async fn put_file(&self, data: &[u8]) -> Result<String>;

    /// Retrieve file bytes by hash. Returns `None` if not found.
    async fn get_file(&self, hash: &str) -> Result<Option<Vec<u8>>>;

    /// Check if a file exists by hash.
    async fn file_exists(&self, hash: &str) -> Result<bool>;

    /// Delete a file by hash. Returns `true` if it was deleted, `false` if not found.
    async fn delete_file(&self, hash: &str) -> Result<bool>;
}

/// Filesystem-backed content-addressed file store.
///
/// Files are stored in a two-level fanout directory structure:
/// `{base_dir}/{hash[0..2]}/{hash[2..4]}/{hash}`
///
/// This prevents any single directory from growing too large.
pub struct FsFileStore {
    base_dir: PathBuf,
}

impl FsFileStore {
    /// Create a new filesystem file store at the given directory.
    /// Creates the directory if it doesn't exist.
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base_dir).map_err(|e| {
            crate::error::Error::Internal(format!(
                "Failed to create file store directory {}: {}",
                base_dir.display(),
                e
            ))
        })?;
        Ok(Self { base_dir })
    }

    /// Get the filesystem path for a given hash.
    fn path_for_hash(&self, hash: &str) -> PathBuf {
        // Two-level fanout: ab/cd/abcdef1234...
        let (a, rest) = hash.split_at(2.min(hash.len()));
        let (b, _) = rest.split_at(2.min(rest.len()));
        self.base_dir.join(a).join(b).join(hash)
    }
}

#[async_trait::async_trait]
impl FileStore for FsFileStore {
    async fn put_file(&self, data: &[u8]) -> Result<String> {
        let hash = compute_file_hash(data);
        let path = self.path_for_hash(&hash);

        // Skip if already exists (content-addressed dedup)
        if path.exists() {
            return Ok(hash);
        }

        // Ensure parent directories exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::error::Error::Internal(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Write atomically: write to temp file then rename
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, data).map_err(|e| {
            crate::error::Error::Internal(format!("Failed to write file {}: {}", tmp_path.display(), e))
        })?;
        std::fs::rename(&tmp_path, &path).map_err(|e| {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            crate::error::Error::Internal(format!("Failed to rename file: {}", e))
        })?;

        Ok(hash)
    }

    async fn get_file(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_for_hash(hash);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path).map_err(|e| {
            crate::error::Error::Internal(format!("Failed to read file {}: {}", path.display(), e))
        })?;
        Ok(Some(data))
    }

    async fn file_exists(&self, hash: &str) -> Result<bool> {
        Ok(self.path_for_hash(hash).exists())
    }

    async fn delete_file(&self, hash: &str) -> Result<bool> {
        let path = self.path_for_hash(hash);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path).map_err(|e| {
            crate::error::Error::Internal(format!(
                "Failed to delete file {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compute_file_hash() {
        let hash = compute_file_hash(b"hello world");
        assert_eq!(hash.len(), 64); // SHA-256 hex = 64 chars
        // Known SHA-256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[tokio::test]
    async fn test_fs_file_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsFileStore::new(dir.path().join("files")).unwrap();

        let data = b"test file content";
        let hash = store.put_file(data).await.unwrap();

        assert!(store.file_exists(&hash).await.unwrap());

        let retrieved = store.get_file(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_fs_file_store_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsFileStore::new(dir.path().join("files")).unwrap();

        let data = b"duplicate content";
        let hash1 = store.put_file(data).await.unwrap();
        let hash2 = store.put_file(data).await.unwrap();

        assert_eq!(hash1, hash2); // Same hash
    }

    #[tokio::test]
    async fn test_fs_file_store_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsFileStore::new(dir.path().join("files")).unwrap();

        assert!(!store.file_exists("nonexistent").await.unwrap());
        assert!(store.get_file("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_fs_file_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsFileStore::new(dir.path().join("files")).unwrap();

        let hash = store.put_file(b"to delete").await.unwrap();
        assert!(store.file_exists(&hash).await.unwrap());

        assert!(store.delete_file(&hash).await.unwrap());
        assert!(!store.file_exists(&hash).await.unwrap());

        // Delete again returns false
        assert!(!store.delete_file(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_fs_file_store_fanout_paths() {
        let store = FsFileStore::new(PathBuf::from("/tmp/test-files")).unwrap();
        let path = store.path_for_hash("abcdef1234567890");
        assert_eq!(
            path,
            PathBuf::from("/tmp/test-files/ab/cd/abcdef1234567890")
        );
    }
}

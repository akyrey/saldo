//! File storage for attachments: `<data dir>/attachments/<sha256>`. Equal
//! files are stored once.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::AppError;

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join("attachments")
}

pub(crate) fn path_of(data_dir: &Path, sha256: &str) -> PathBuf {
    folder(data_dir).join(sha256)
}

/// Writes the file unless it exists. Writing to a temporary name and renaming
/// means a crash never leaves a half-written file under the real name.
pub(crate) async fn store(data_dir: &Path, sha256: &str, bytes: &[u8]) -> Result<(), AppError> {
    let target = path_of(data_dir, sha256);
    if tokio::fs::try_exists(&target).await.map_err(io_error)? {
        return Ok(());
    }
    tokio::fs::create_dir_all(folder(data_dir))
        .await
        .map_err(io_error)?;
    let temporary = target.with_extension("partial");
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(io_error)?;
    tokio::fs::rename(&temporary, &target)
        .await
        .map_err(io_error)
}

pub(crate) async fn read(data_dir: &Path, sha256: &str) -> Result<Vec<u8>, AppError> {
    tokio::fs::read(path_of(data_dir, sha256))
        .await
        .map_err(io_error)
}

pub(crate) async fn remove(data_dir: &Path, sha256: &str) -> Result<(), AppError> {
    match tokio::fs::remove_file(path_of(data_dir, sha256)).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::Internal(format!("attachment storage: {error}"))
}

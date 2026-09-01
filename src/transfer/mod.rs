use axum::body::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedFile {
    pub id: String,
    pub original_name: String,
    pub file_size: u64,
    pub content_type: String,
    pub uploaded_at: String,
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct TransferManager {
    pub staging_dir: PathBuf,
}

impl TransferManager {
    pub fn new<P: AsRef<Path>>(staging_dir: P) -> Self {
        let path = staging_dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&path);
        Self { staging_dir: path }
    }

    pub async fn save_upload(
        &self,
        original_name: String,
        content_type: String,
        data: Bytes,
    ) -> Result<StagedFile, String> {
        let id = Uuid::new_v4().to_string();
        let safe_name = Path::new(&original_name)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload.bin".to_string());

        let target_filename = format!("{}_{}", id, safe_name);
        let target_path = self.staging_dir.join(&target_filename);

        tokio::fs::write(&target_path, &data)
            .await
            .map_err(|e| format!("Failed to save staged file: {}", e))?;

        let file_size = data.len() as u64;
        let uploaded_at = Utc::now().to_rfc3339();

        info!(
            "Staged uploaded file '{}' ({} bytes) -> {}",
            original_name,
            file_size,
            target_path.display()
        );

        Ok(StagedFile {
            id,
            original_name,
            file_size,
            content_type,
            uploaded_at,
            path: target_path,
        })
    }

    pub fn get_staged_file(&self, id: &str) -> Option<PathBuf> {
        if let Ok(entries) = std::fs::read_dir(&self.staging_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(id) {
                    return Some(entry.path());
                }
            }
        }
        None
    }

    pub fn delete_staged_file(&self, id: &str) -> bool {
        if let Some(path) = self.get_staged_file(id) {
            let _ = std::fs::remove_file(path);
            return true;
        }
        false
    }
}

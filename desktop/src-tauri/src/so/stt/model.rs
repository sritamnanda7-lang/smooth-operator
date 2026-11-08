// SPDX-License-Identifier: FSL-1.1-MIT
//! The one shipped speech model: pinned URL + SHA-256, registry seed. Tests: `so/tests/stt_model_tests.rs`.
//!
//! Model: NVIDIA Parakeet Unified English 0.6B, Q8_0 GGUF, hosted in our own Hugging Face repo.
//! Licence: NVIDIA Open Model License (NOT CC-BY-4.0). Credit line: "Licensed by NVIDIA Corporation
//! under the NVIDIA Open Model License" (NOTICE, the repo's model card, one line in the Models pane).
//!
//! The URL pins a repo COMMIT, not `main`, so the bytes can never change under us; the SHA-256 is
//! checked by Handy's download path (it deletes the file on a mismatch).
use crate::managers::model::{ModelInfo, ModelSource};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const SO_MODEL_ID: &str = "so/parakeet-unified-en-0.6b-q8";

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub id: &'static str,
    pub filename: &'static str,
    /// `https://huggingface.co/smoothoperator3/<repo>/resolve/<40-hex commit>/<file>`
    pub url: &'static str,
    /// 64 lowercase hex.
    pub sha256: &'static str,
    /// Bytes.
    pub size: u64,
}

pub const SO_MODEL: ModelSpec = ModelSpec {
    id: SO_MODEL_ID,
    filename: "parakeet-unified-en-0.6b-Q8_0.gguf",
    url: "https://huggingface.co/smoothoperator3/parakeet-unified-en-0.6b-gguf/resolve/0426565e21bbd125c139caed83f5d9c16e218e8b/parakeet-unified-en-0.6b-Q8_0.gguf",
    sha256: "4b50b6dd862bf6e346929aaf4f5eaacec003bfa3f56462d6c874b41ef2f38795",
    size: 731_357_568,
};

impl ModelSpec {
    /// The registry entry. `source` is `ModelSource::Url { url, sha256: Some(sha256) }`, so Handy's
    /// download path verifies the hash. `is_downloaded` is true only when `models_dir/filename`
    /// exists with exactly `size` bytes.
    pub fn model_info(&self, models_dir: &Path) -> ModelInfo {
        let on_disk = fs::metadata(models_dir.join(self.filename)).map(|m| m.len()).ok();
        let partial = fs::metadata(models_dir.join(format!("{}.partial", self.filename)))
            .map(|m| m.len())
            .unwrap_or(0);
        ModelInfo {
            id: self.id.to_string(),
            name: "Speech model".to_string(),
            description: "English speech recognition (Parakeet Unified 0.6B)".to_string(),
            filename: self.filename.to_string(),
            source: ModelSource::Url {
                url: self.url.to_string(),
                sha256: Some(self.sha256.to_string()),
            },
            size_mb: self.size / (1024 * 1024),
            is_downloaded: on_disk == Some(self.size),
            is_downloading: false,
            partial_size: partial,
            accuracy_score: 0.9,
            speed_score: 0.95,
            supports_translation: false,
            is_recommended: true,
            supported_languages: vec!["en".to_string()],
            supports_language_selection: false,
            is_custom: false,
            supports_streaming: false,
            supports_language_detection: false,
        }
    }

    /// Test-only (T23): in the app, Handy's download path does this same check and deletes a bad file.
    /// SHA-256 of the file at `path` must equal `self.sha256`. On mismatch (or a read error) the file
    /// is deleted and `Err` returned; on a match `Ok`. A missing file is an `Err` (nothing to delete).
    #[cfg(test)]
    pub fn verify_file(&self, path: &Path) -> Result<(), String> {
        let hash = file_sha256(path);
        match hash {
            Ok(h) if h == self.sha256 => Ok(()),
            other => {
                let _ = fs::remove_file(path);
                match other {
                    Ok(_) => Err("model file does not match its pinned SHA-256".to_string()),
                    Err(e) => Err(format!("cannot read model file: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
fn file_sha256(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// The whole model registry for `models_dir`: exactly one entry, `SO_MODEL`. Other files in the
/// directory and any Hugging Face cache are ignored.
pub fn seed(models_dir: &Path) -> HashMap<String, ModelInfo> {
    let mut map = HashMap::new();
    map.insert(SO_MODEL.id.to_string(), SO_MODEL.model_info(models_dir));
    map
}

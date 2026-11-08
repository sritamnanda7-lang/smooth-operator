// SPDX-License-Identifier: FSL-1.1-MIT
//! The one shipped speech model: pinned URL + SHA-256, registry seed. Tests: `so/tests/stt_model_tests.rs`.
//!
//! PLACEHOLDER (TBD after the benchmark and the founder's HF upload): the `SO_MODEL` values below are
//! deliberately invalid, so T21 fails until sub-step 3/4 fills in the real repo, 40-hex commit, file
//! name, SHA-256 and size.
use crate::managers::model::ModelInfo;
use std::collections::HashMap;
use std::path::Path;

pub const SO_MODEL_ID: &str = "so/TBD";

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub id: &'static str,
    pub filename: &'static str,
    /// `https://huggingface.co/smoothoperator/<repo>/resolve/<40-hex commit>/<file>`
    pub url: &'static str,
    /// 64 lowercase hex.
    pub sha256: &'static str,
    /// Bytes.
    pub size: u64,
}

/// PLACEHOLDER values (see module doc).
pub const SO_MODEL: ModelSpec = ModelSpec {
    id: SO_MODEL_ID,
    filename: "TBD.gguf",
    url: "TBD",
    sha256: "TBD",
    size: 0,
};

impl ModelSpec {
    /// STEP-2 STUB (`todo!()`): the registry entry. `source` is `ModelSource::Url { url, sha256:
    /// Some(sha256) }`, so Handy's download path verifies the hash. `is_downloaded` is true only when
    /// `models_dir/filename` exists with exactly `size` bytes.
    pub fn model_info(&self, _models_dir: &Path) -> ModelInfo {
        todo!("step 2: ModelSpec::model_info")
    }

    /// STEP-2 STUB (`todo!()`): SHA-256 of the file at `path` must equal `self.sha256`. On mismatch
    /// (or a read error) the file is deleted and `Err` returned; on a match `Ok`.
    pub fn verify_file(&self, _path: &Path) -> Result<(), String> {
        todo!("step 2: ModelSpec::verify_file")
    }
}

/// STEP-2 STUB (`todo!()`): the whole model registry for `models_dir`: exactly one entry, `SO_MODEL`.
/// Other files in the directory and any Hugging Face cache are ignored.
pub fn seed(_models_dir: &Path) -> HashMap<String, ModelInfo> {
    todo!("step 2: model::seed")
}

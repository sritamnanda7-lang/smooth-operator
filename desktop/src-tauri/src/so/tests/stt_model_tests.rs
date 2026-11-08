// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 tests T21-T24: the pinned model, the one-model registry, download verification, settings.
//!
//! PLACEHOLDER NOTE: `SO_MODEL` in `so/stt/model.rs` holds invalid placeholder values until the benchmark
//! picks a model and the founder uploads it to `smoothoperator/<repo>` (sub-steps 2-3). T21 fails until
//! the real repo, 40-hex commit, file name, SHA-256 and size are filled in. The tests below never name
//! them, so they need no change then.
use crate::managers::model::ModelSource;
use crate::settings::{get_default_settings, ModelUnloadTimeout};
use crate::so::stt::model::{seed, ModelSpec, SO_MODEL, SO_MODEL_ID};
use crate::so::stt::enforce_settings;
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::TempDir;

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A spec for `bytes` with the real hash of `bytes`, so tests need no real model.
fn spec_for(bytes: &[u8]) -> ModelSpec {
    let sha: String = format!("{:x}", Sha256::digest(bytes));
    ModelSpec {
        id: "so/test-model",
        filename: "test-model.gguf",
        url: "https://huggingface.co/smoothoperator/test/resolve/0000000000000000000000000000000000000000/test-model.gguf",
        sha256: Box::leak(sha.into_boxed_str()),
        size: bytes.len() as u64,
    }
}

#[test]
fn t21_so_model_is_pinned_to_our_hf_repo_commit_and_sha256() {
    let m = SO_MODEL;
    assert!(m.url.starts_with("https://huggingface.co/smoothoperator/"), "url: {}", m.url);
    let parts: Vec<&str> = m.url.split('/').collect();
    // https: "" huggingface.co smoothoperator <repo> resolve <commit> <file...>
    assert!(parts.len() >= 8, "url shape: {}", m.url);
    assert!(!parts[4].is_empty(), "repo name");
    assert_eq!(parts[5], "resolve");
    assert!(is_lower_hex(parts[6], 40), "commit must be 40 lowercase hex, got {:?}", parts[6]);
    assert_eq!(parts[7..].join("/"), m.filename, "url ends with the file name");
    assert!(is_lower_hex(m.sha256, 64), "sha256 must be 64 lowercase hex, got {:?}", m.sha256);
    assert!(m.size > 0 && m.size < 2 * 1024 * 1024 * 1024, "size {}", m.size);
    assert!(m.id.starts_with("so/"), "id: {}", m.id);
    assert_eq!(m.id, SO_MODEL_ID);
    assert!(!m.filename.is_empty());
}

#[test]
fn t21_registry_entry_is_a_url_source_with_the_hash_so_handys_download_verifies_it() {
    let dir = TempDir::new().unwrap();
    let info = SO_MODEL.model_info(dir.path());
    assert_eq!(info.id, SO_MODEL.id);
    assert_eq!(info.filename, SO_MODEL.filename);
    match info.source {
        ModelSource::Url { url, sha256 } => {
            assert_eq!(url, SO_MODEL.url);
            assert_eq!(sha256.as_deref(), Some(SO_MODEL.sha256), "hash must be set, never None");
        }
        other => panic!("source must be Url, got {other:?}"),
    }
    assert!(!info.is_downloaded && !info.is_downloading);
    assert!(!info.is_custom);
}

#[test]
fn t21_is_downloaded_only_when_the_file_has_exactly_the_pinned_size() {
    let spec = spec_for(b"12345");
    let dir = TempDir::new().unwrap();
    assert!(!spec.model_info(dir.path()).is_downloaded, "missing file");
    fs::write(dir.path().join(spec.filename), b"1234").unwrap();
    assert!(!spec.model_info(dir.path()).is_downloaded, "short file");
    fs::write(dir.path().join(spec.filename), b"12345").unwrap();
    assert!(spec.model_info(dir.path()).is_downloaded);
}

#[test]
fn t22_registry_has_exactly_one_model_whatever_else_is_on_disk() {
    let dir = TempDir::new().unwrap();
    let models = dir.path().join("models");
    fs::create_dir_all(&models).unwrap();
    // Decoys in the models dir: other .gguf/.bin files, Handy's old model names, a custom model.
    for name in ["other.gguf", "ggml-small.bin", "ggml-large-v3-turbo.bin", "custom-model.bin", SO_MODEL.filename] {
        fs::write(models.join(name), b"decoy").unwrap();
    }
    fs::create_dir_all(models.join("parakeet-tdt-0.6b-v3-int8")).unwrap();
    // A Handy model in a fake Hugging Face cache, found through HF_HOME.
    let hf = dir.path().join("hf");
    let snap = hf.join("hub/models--handy-computer--test-model/snapshots/rev1");
    fs::create_dir_all(&snap).unwrap();
    fs::write(snap.join("model.gguf"), b"model").unwrap();
    std::env::set_var("HF_HOME", &hf);
    let reg = seed(&models);
    std::env::remove_var("HF_HOME");
    assert_eq!(reg.len(), 1, "registry: {:?}", reg.keys().collect::<Vec<_>>());
    let info = reg.get(SO_MODEL.id).expect("the SO model");
    assert!(!info.is_downloaded, "a decoy file with the right name but the wrong size is not the model");
    assert!(!info.is_custom);
}

#[test]
fn t23_wrong_bytes_fail_verification_and_the_file_is_deleted() {
    let spec = spec_for(b"the real model bytes");
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(spec.filename);
    fs::write(&path, b"the WRONG model bytes").unwrap();
    assert!(spec.verify_file(&path).is_err());
    assert!(!path.exists(), "a corrupt file must be deleted so the next try starts clean");
    // And it is not marked downloaded.
    assert!(!spec.model_info(dir.path()).is_downloaded);
}

#[test]
fn t23_right_bytes_pass_and_stay() {
    let spec = spec_for(b"the real model bytes");
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(spec.filename);
    fs::write(&path, b"the real model bytes").unwrap();
    assert!(spec.verify_file(&path).is_ok());
    assert!(path.exists());
}

#[test]
fn t23_a_missing_file_fails_verification() {
    let spec = spec_for(b"x");
    let dir = TempDir::new().unwrap();
    assert!(spec.verify_file(&dir.path().join("nope.gguf")).is_err());
}

#[test]
fn t24_settings_enforcement_sets_everything_and_is_idempotent() {
    let mut s = get_default_settings();
    // Stale: a Handy model, auto language, Handy defaults, mic always on.
    s.selected_model = "whisper-large-v3-turbo".into();
    s.selected_language = "auto".into();
    s.model_unload_timeout = ModelUnloadTimeout::Min5;
    s.always_on_microphone = true;
    s.lazy_stream_close = true;
    assert!(enforce_settings(&mut s), "something changed");
    assert_eq!(s.selected_model, SO_MODEL_ID);
    assert_eq!(s.selected_language, "en");
    assert_eq!(s.model_unload_timeout, ModelUnloadTimeout::Min2);
    assert!(!s.always_on_microphone);
    assert!(!s.lazy_stream_close);
    assert!(!enforce_settings(&mut s), "second run changes nothing");
    assert_eq!(s.selected_model, SO_MODEL_ID);
    assert_eq!(s.model_unload_timeout, ModelUnloadTimeout::Min2);
}

#[test]
fn t24_each_setting_is_enforced_on_its_own() {
    for which in 0..5 {
        let mut s = get_default_settings();
        assert!(enforce_settings(&mut s));
        assert!(!enforce_settings(&mut s));
        match which {
            0 => s.selected_model = "small".into(),
            1 => s.selected_language = "auto".into(),
            2 => s.model_unload_timeout = ModelUnloadTimeout::Never,
            3 => s.always_on_microphone = true,
            _ => s.lazy_stream_close = true,
        }
        assert!(enforce_settings(&mut s), "setting #{which} was not enforced");
        assert!(!enforce_settings(&mut s));
    }
}

// SPDX-License-Identifier: FSL-1.1-MIT
//! The provider and model choice, kept in our own `so_settings.json`. Nothing secret is stored here.

use super::brain::BrainChoice;
use super::provider::{ProviderId, PROVIDERS};
use crate::so::settings::STORE_PATH;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const PROVIDER_KEY: &str = "brain_provider";

fn model_key(p: ProviderId) -> &'static str {
    match p {
        ProviderId::Claude => "brain_model_claude",
        ProviderId::Gemini => "brain_model_gemini",
    }
}

/// The chosen provider (the recommended one if nothing valid is saved) and its model: a well-formed
/// override from the settings file, else the provider's default.
pub fn load_choice(app: &AppHandle) -> BrainChoice {
    let store = app.store(crate::portable::store_path(STORE_PATH)).ok();
    let text = |key: &str| store.as_ref().and_then(|s| s.get(key)).and_then(|v| v.as_str().map(str::to_owned));
    let provider = text(PROVIDER_KEY)
        .and_then(|t| serde_json::from_value::<ProviderId>(serde_json::Value::String(t)).ok())
        .or_else(|| PROVIDERS.iter().find(|p| p.recommended).map(|p| p.id))
        .unwrap_or(ProviderId::Claude);
    let default_model = PROVIDERS.iter().find(|p| p.id == provider).map_or("", |p| p.default_model);
    let model = text(model_key(provider)).filter(|m| valid_model(m)).unwrap_or_else(|| default_model.to_string());
    BrainChoice { provider, model }
}

pub fn save_provider(app: &AppHandle, p: ProviderId) -> Result<(), String> {
    let store = app.store(crate::portable::store_path(STORE_PATH)).map_err(|_| String::from("could not open settings"))?;
    let value = serde_json::to_value(p).map_err(|_| String::from("could not save settings"))?;
    store.set(PROVIDER_KEY, value);
    store.save().map_err(|_| String::from("could not save settings"))
}

/// `[a-z0-9][a-z0-9.-]{0,63}`; a bad override falls back to the default model.
pub fn valid_model(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    first_ok && name.len() <= 64 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

// Not wired into the app until increment 5 (`load_choice`) and 6 (`save_provider`). Delete it then.
const _: () = {
    let _ = load_choice;
    let _ = save_provider;
};

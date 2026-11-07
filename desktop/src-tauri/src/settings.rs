// SPDX-License-Identifier: MIT
// Modified from Handy v0.10.0 (Copyright (c) 2025 CJ Pais): see git history for changes.
use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    Top,
    // `none` is retired: overlay visibility is owned by `OverlayStyle` now. The
    // alias keeps legacy stores (`"overlay_position": "none"`) deserializing
    // instead of failing the whole load; the one-time overlay migration reads the
    // raw stored string to recover the old "hidden" intent as `OverlayStyle::None`.
    #[serde(alias = "none")]
    Bottom,
}

/// Which recording overlay to display. `Minimal` and `Live` share one base
/// (the pill); `Live` grows into the panel that shows live transcription text.
/// `None` hides the overlay entirely. Decoupled from whether the model runs in
/// streaming mode (that is driven purely by model capability).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayStyle {
    None,
    Minimal,
    Live,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    #[default]
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
}

/// How the transcribe shortcut's key events drive a recording.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutActivation {
    /// Press to start, press again to stop.
    Toggle,
    /// Hold to record, release to stop.
    PushToTalk,
    /// Hold to record and release to stop, or tap to keep recording until the
    /// next press. Which one it was is decided by how long the key was held
    /// (`hold_threshold_ms`).
    #[default]
    HoldOrToggle,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    DontModify,
    CopyToClipboard,
}

/// Script applied to Mandarin and Cantonese output. Traditional carries its region
/// (Taiwan vs Hong Kong forms, e.g. 裡/裏) rather than guessing it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChineseScript {
    /// Keep whatever script the model produced.
    #[default]
    AsTranscribed,
    Simplified,
    /// Traditional, Taiwan forms. Also reads legacy `traditional` (migrated on load).
    #[serde(alias = "traditional")]
    TraditionalTw,
    /// Traditional, in Hong Kong's standard forms.
    TraditionalHk,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    #[default]
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    HandyKeys,
    /// No built-in global shortcuts; trigger Handy via CLI flags or signals
    /// (e.g. from a Wayland compositor keybinding).
    None,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::HandyKeys;
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0), // Special case for debug - handled separately
        }
    }

    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

/// UI appearance mode. `System` follows the OS `prefers-color-scheme`; `Light`
/// and `Dark` force one of the two palettes Handy already ships.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    #[default]
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum VadBackend {
    #[default]
    Silero,
    Earshot,
}

/// When Handy's spoken-language detector picks the language for Auto.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum LanguageDetectionMode {
    /// Only for models that can't detect the language themselves.
    #[default]
    WhenNeeded,
    /// For every model except streaming ones, restricted to the languages
    /// the person speaks.
    Always,
}

/* still handy for composing the initial JSON in the store ------------- */
/// The container-level `serde(default)` (backed by the `Default` impl below)
/// guarantees every field — including ones added in the future — falls back to
/// its `get_default_settings()` value when missing from a stored settings
/// object, so a partial store can never fail the whole load (#1619).
/// Field-level defaults below take precedence where present.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct AppSettings {
    /// Internal settings schema marker for one-time migrations. Fresh installs
    /// start at the current version; existing stores missing this key are
    /// treated as version 0 and migrated forward.
    #[serde(default = "default_settings_schema_version")]
    pub settings_schema_version: u32,
    /// Defaults to empty on partial stores; the load path merges in the
    /// default bindings for any missing keys before the settings are used.
    #[serde(default)]
    pub bindings: HashMap<String, ShortcutBinding>,
    /// Replaces the pre-0.10 `push_to_talk` bool; stores missing this key are
    /// migrated from it in `apply_settings_migrations`.
    #[serde(default)]
    pub shortcut_activation: ShortcutActivation,
    /// Hold-or-toggle only: a press held at least this long is push-to-talk,
    /// anything shorter is a tap that locks recording on.
    #[serde(default = "default_hold_threshold_ms")]
    pub hold_threshold_ms: u64,
    #[serde(default)]
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    /// Which input channel to use on the selected microphone device.
    /// None means "average all channels" (original behavior).
    #[serde(default)]
    pub selected_channel: Option<u16>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    /// Every language the person dictates in (codes, e.g. ["en", "zh"]),
    /// picked during onboarding. Empty until then.
    #[serde(default)]
    pub spoken_languages: Vec<String>,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default = "default_theme")]
    pub theme: Theme,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    /// Experimental, macOS only: move a Bluetooth headset (AirPods) to this
    /// Mac while recording, so its microphone does not return silence while
    /// another Apple device is playing through it.
    #[serde(default)]
    pub airpods_mode: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_paste_delay_after_ms")]
    pub paste_delay_after_ms: u64,
    /// Debug-gated ("beta") receipt-sequenced paste: restore the clipboard only
    /// after the target app actually reads the transcript, instead of after a
    /// fixed delay. See `paste_tx`. macOS and Windows only.
    #[serde(default)]
    pub reliable_paste: bool,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    #[serde(default = "default_filler_word_removal_enabled")]
    pub filler_word_removal_enabled: bool,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    /// Fresh installs default from the OS locale; existing stores are migrated
    /// in `apply_settings_migrations`.
    #[serde(default)]
    pub chinese_script: ChineseScript,
    #[serde(default)]
    pub transcribe_accelerator: TranscribeAcceleratorSetting,
    /// Stable transcribe.cpp device selector. This is derived from the backend's
    /// `device_id` when available (or its name for backends such as Metal),
    /// never from the process-local device registry index.
    #[serde(
        default = "default_transcribe_gpu_device",
        deserialize_with = "deserialize_transcribe_gpu_device"
    )]
    pub transcribe_gpu_device: Option<String>,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    /// Experimental detector implementation. Silero remains the stable default.
    #[serde(default)]
    pub vad_backend: VadBackend,
    /// Decode long audio in chunks cut at pauses, for models whose accuracy
    /// degrades on long audio. Independent of `vad_enabled`, which only
    /// decides whether silence is dropped from recordings.
    #[serde(default = "default_long_form_chunking")]
    pub long_form_chunking: bool,
    #[serde(default)]
    pub language_detection: LanguageDetectionMode,
    /// Seconds of speech the language detector hears (debug). Read through
    /// [`AppSettings::language_detection_window_samples`], which clamps it.
    #[serde(default = "default_language_detection_window_secs")]
    pub language_detection_window_secs: u32,
    /// Which recording overlay to show: None / Minimal / Live. Streaming mode is
    /// not gated on this — that follows model capability. Migrated from the old
    /// `overlay_position` (position `none` → style `None`).
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
}

fn default_model() -> String {
    "".to_string()
}

const CURRENT_SETTINGS_SCHEMA_VERSION: u32 = 2;

fn default_settings_schema_version() -> u32 {
    CURRENT_SETTINGS_SCHEMA_VERSION
}

fn default_hold_threshold_ms() -> u64 {
    300
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_start_hidden() -> bool {
    false
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    // Position only matters when the overlay is shown; whether it shows at all is
    // `overlay_style` (Linux defaults that to None). So a single default suffices.
    OverlayPosition::Bottom
}

fn default_overlay_style() -> OverlayStyle {
    // Linux hides the overlay by default; other platforms show the live overlay.
    // Position is independent and only selects top vs. bottom placement.
    #[cfg(target_os = "linux")]
    return OverlayStyle::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayStyle::Live;
}

fn default_vad_enabled() -> bool {
    true
}

fn default_long_form_chunking() -> bool {
    true
}

fn default_language_detection_window_secs() -> u32 {
    5
}

fn default_filler_word_removal_enabled() -> bool {
    true
}

fn default_chinese_script() -> ChineseScript {
    crate::system_languages::suggested_chinese_script(
        crate::system_languages::system_languages()
            .iter()
            .map(String::as_str),
    )
    .unwrap_or_default()
}

fn default_debug_mode() -> bool {
    false
}

fn default_log_level() -> LogLevel {
    LogLevel::Debug
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_paste_delay_after_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}

fn default_theme() -> Theme {
    Theme::System
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_transcribe_gpu_device() -> Option<String> {
    None // automatic device selection
}

/// Accept the 0.1-era integer registry index long enough for the schema
/// migration to clear it. Device indices are process-local in transcribe.cpp
/// 0.2 and must never be carried across launches.
fn deserialize_transcribe_gpu_device<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<serde_json::Value>::deserialize(deserializer)? {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(serde_json::Value::Number(_)) => Ok(None),
        Some(_) => Err(de::Error::custom(
            "transcribe GPU device must be a string, integer, or null",
        )),
    }
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_default_settings() -> AppSettings {
    let mut bindings = HashMap::new();
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );

    AppSettings {
        settings_schema_version: default_settings_schema_version(),
        bindings,
        shortcut_activation: ShortcutActivation::default(),
        hold_threshold_ms: default_hold_threshold_ms(),
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        selected_model: "".to_string(),
        onboarding_completed: false,
        always_on_microphone: false,
        spoken_languages: Vec::new(),
        selected_microphone: None,
        selected_channel: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        mute_while_recording: false,
        append_trailing_space: false,
        app_language: default_app_language(),
        theme: default_theme(),
        experimental_enabled: false,
        lazy_stream_close: false,
        airpods_mode: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        paste_delay_after_ms: default_paste_delay_after_ms(),
        reliable_paste: false,
        typing_tool: default_typing_tool(),
        filler_word_removal_enabled: default_filler_word_removal_enabled(),
        custom_filler_words: None,
        chinese_script: default_chinese_script(),
        transcribe_accelerator: TranscribeAcceleratorSetting::default(),
        transcribe_gpu_device: default_transcribe_gpu_device(),
        extra_recording_buffer_ms: 0,
        vad_enabled: default_vad_enabled(),
        vad_backend: VadBackend::default(),
        long_form_chunking: default_long_form_chunking(),
        language_detection: LanguageDetectionMode::default(),
        language_detection_window_secs: default_language_detection_window_secs(),
        overlay_style: default_overlay_style(),
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        get_default_settings()
    }
}

/// The language detector's window bounds: below 2 s it guesses, and it only
/// scores the first 30 s of what it's given.
pub const LANGUAGE_DETECTION_WINDOW_SECS: std::ops::RangeInclusive<u32> = 2..=30;

impl AppSettings {
    /// Samples (16 kHz) of speech the language detector hears.
    pub fn language_detection_window_samples(&self) -> usize {
        let (min, max) = LANGUAGE_DETECTION_WINDOW_SECS.into_inner();
        self.language_detection_window_secs.clamp(min, max) as usize * 16_000
    }
}

/// Startup entry point. Same load-or-create/salvage/migrate behavior as
/// `get_settings`; kept as a named alias for call-site clarity, plus a
/// one-time debug dump of the loaded settings.
pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    let settings = get_settings(app);
    debug!("Loaded settings: {:?}", settings);
    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    // Settings reads also persist one-time migrations. Migration helpers are
    // idempotent, so this converges after the first read of an older store.
    let settings = if let Some(settings_value) = store.get("settings") {
        let (mut settings, mut updated) =
            match serde_json::from_value::<AppSettings>(settings_value.clone()) {
                Ok(settings) => (settings, false),
                Err(e) => {
                    warn!("Failed to parse stored settings ({e}); salvaging valid fields");
                    (salvage_settings(&settings_value), true)
                }
            };

        if apply_settings_migrations(&mut settings, &settings_value) {
            updated = true;
        }

        // Merge in any bindings added since this store was written.
        for (key, value) in get_default_settings().bindings {
            if let std::collections::hash_map::Entry::Vacant(entry) = settings.bindings.entry(key) {
                debug!("Adding missing binding: {}", entry.key());
                entry.insert(value);
                updated = true;
            }
        }

        if updated {
            store.set("settings", serde_json::to_value(&settings).unwrap());
        }

        settings
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    settings
}

/// Rebuilds settings from a store value that failed to deserialize as a whole.
/// Every stored field that is individually valid is kept; only broken values
/// (e.g. an enum variant written by a newer or older version) fall back to
/// their default. This means one bad field can never reset the rest of the
/// user's configuration (#1619).
fn salvage_settings(stored: &serde_json::Value) -> AppSettings {
    let Some(stored_map) = stored.as_object() else {
        warn!("Stored settings are not a JSON object; falling back to defaults");
        return get_default_settings();
    };

    let mut merged = serde_json::to_value(get_default_settings())
        .expect("default settings serialize to a JSON object");

    for (key, value) in stored_map {
        let previous = merged
            .as_object_mut()
            .expect("merged settings stay an object")
            .insert(key.clone(), value.clone());
        if serde_json::from_value::<AppSettings>(merged.clone()).is_err() {
            // Log only the key: values may hold secrets (e.g. API keys).
            warn!("Dropping invalid settings field '{key}', keeping its default");
            let map = merged
                .as_object_mut()
                .expect("merged settings stay an object");
            match previous {
                Some(previous) => map.insert(key.clone(), previous),
                None => map.remove(key),
            };
        }
    }

    serde_json::from_value(merged).unwrap_or_else(|e| {
        warn!("Failed to reassemble salvaged settings ({e}); falling back to defaults");
        get_default_settings()
    })
}

fn apply_settings_migrations(
    settings: &mut AppSettings,
    settings_value: &serde_json::Value,
) -> bool {
    let mut updated = false;

    // One-time onboarding migration: users with an explicit selected model have
    // already made it through model selection. Users who merely have compatible
    // files on disk should still see onboarding.
    if settings_value.get("onboarding_completed").is_none() {
        settings.onboarding_completed = !settings.selected_model.is_empty();
        updated = true;
    }

    // One-time shortcut activation migration (only while the new key is
    // absent): the retired `push_to_talk` bool maps onto the two legacy modes so
    // upgrading users keep exactly the behavior they had. Only fresh installs
    // get the hold-or-toggle default.
    if settings_value.get("shortcut_activation").is_none() {
        if let Some(push_to_talk) = settings_value.get("push_to_talk").and_then(|v| v.as_bool()) {
            settings.shortcut_activation = if push_to_talk {
                ShortcutActivation::PushToTalk
            } else {
                ShortcutActivation::Toggle
            };
            updated = true;
        }
    }

    // One-time spoken languages backfill for users onboarded before it was asked.
    // The guess always includes English, so it never decides a transcription
    // (`effective_language` only uses a lone language). Must run before the
    // Chinese script migrations: it reads `zh-Hans`/`zh-Hant`, and they read it.
    if settings.onboarding_completed && settings.spoken_languages.is_empty() {
        settings.spoken_languages = guessed_spoken_languages(settings, settings_value);
        updated = true;
    }

    // One-time Chinese script migration: the script used to be chosen through
    // `zh-Hans`/`zh-Hant` language intents. Split those into the recognition
    // language and the script setting; every other upgrading user keeps the
    // unconverted output they had. Only fresh installs get the locale default.
    if settings_value.get("chinese_script").is_none() {
        settings.chinese_script = match settings.selected_language.as_str() {
            "zh-Hans" => ChineseScript::Simplified,
            "zh-Hant" => ChineseScript::TraditionalTw,
            _ => ChineseScript::AsTranscribed,
        };
        if settings.chinese_script != ChineseScript::AsTranscribed {
            settings.selected_language = "zh".to_string();
        }
        updated = true;
    }

    // One-time Traditional split: `traditional` took its regional forms from
    // what was spoken (Hong Kong's for Cantonese, Taiwan's for Mandarin).
    // Keep Hong Kong's for people who speak Cantonese or whose OS's first
    // Traditional Chinese language is Hong Kong's or Macau's. People who
    // dictated both now get one region's forms for both.
    if settings_value
        .get("chinese_script")
        .and_then(|v| v.as_str())
        == Some("traditional")
    {
        let hong_kong = settings.spoken_languages.iter().any(|l| l == "yue")
            || os_languages().iter().find_map(|tag| {
                match crate::chinese_script::chinese_script_for_locale(tag) {
                    Some(s @ (ChineseScript::TraditionalTw | ChineseScript::TraditionalHk)) => {
                        Some(s)
                    }
                    _ => None,
                }
            }) == Some(ChineseScript::TraditionalHk);
        settings.chinese_script = if hong_kong {
            ChineseScript::TraditionalHk
        } else {
            ChineseScript::TraditionalTw
        };
        updated = true;
    }

    let stored_schema_version = settings_value
        .get("settings_schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if stored_schema_version < 1 {
        // Before schema 1 this was a UI ordinal. Preserve the original safety
        // migration: a positive selection was ambiguous even in 0.1.
        let had_positive_legacy_selection = settings_value
            .get("transcribe_gpu_device")
            .and_then(|value| value.as_i64())
            .is_some_and(|value| value > 0);
        if had_positive_legacy_selection {
            settings.transcribe_accelerator = TranscribeAcceleratorSetting::Auto;
        }
    }
    if stored_schema_version < 2 {
        // transcribe.cpp 0.2 replaced integer registry indices with opaque
        // process-local handles. Clear every old index once.
        settings.transcribe_gpu_device = default_transcribe_gpu_device();
        settings.settings_schema_version = CURRENT_SETTINGS_SCHEMA_VERSION;
        updated = true;
    }

    // The generic GPU choice was removed in favor of Auto or an exact device.
    // Normalize settings created by builds that exposed that short-lived option.
    if settings.transcribe_accelerator == TranscribeAcceleratorSetting::Gpu
        && settings.transcribe_gpu_device.is_none()
    {
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Auto;
        updated = true;
    }

    // One-time overlay migration (only while the new key is absent): the retired
    // overlay_position `none` meant "hide the overlay" → OverlayStyle::None; any
    // other position had it visible → Live. The position enum no longer has a
    // `none` variant (legacy "none" deserializes to Bottom via a serde alias), so
    // read the raw stored string to recover the old intent.
    if settings_value.get("overlay_style").is_none() {
        let was_hidden = settings_value
            .get("overlay_position")
            .and_then(|v| v.as_str())
            == Some("none");
        settings.overlay_style = if was_hidden {
            OverlayStyle::None
        } else {
            OverlayStyle::Live
        };
        updated = true;
    }

    updated
}

/// Likely spoken languages from existing settings and the OS; always includes English.
fn guessed_spoken_languages(
    settings: &AppSettings,
    settings_value: &serde_json::Value,
) -> Vec<String> {
    // Read the stored value: a missing key may hold the locale default, not a choice.
    let picked_script = settings_value
        .get("chinese_script")
        .and_then(|v| v.as_str())
        .is_some_and(|script| script != "as_transcribed");
    let intent = settings.selected_language.as_str();
    // A script says Chinese, unless the language they picked already does.
    let picked_chinese = crate::chinese_script::chinese_script_for_locale(intent).is_some();
    let tags = [
        (intent != "auto").then_some(intent),
        (picked_script && !picked_chinese).then_some("zh"),
        Some(settings.app_language.as_str()),
    ]
    .into_iter()
    .flatten()
    .chain(os_languages().iter().map(String::as_str));
    crate::system_languages::suggested_languages(tags)
}

/// The OS's languages; empty under test so migrations are deterministic.
fn os_languages() -> &'static [String] {
    if cfg!(test) {
        &[]
    } else {
        crate::system_languages::system_languages()
    }
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    store.set("settings", serde_json::to_value(&settings).unwrap());
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(settings: &AppSettings, id: &str) -> Result<ShortcutBinding, String> {
    settings
        .bindings
        .get(id)
        .cloned()
        .ok_or_else(|| format!("Binding with id '{}' not found", id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_binding_returns_the_requested_binding() {
        let settings = get_default_settings();

        let result = get_stored_binding(&settings, "cancel");

        assert_eq!(result.unwrap().id, "cancel");
    }

    #[test]
    fn unknown_stored_binding_returns_an_error() {
        let settings = get_default_settings();

        let result = get_stored_binding(&settings, "unknown");

        assert_eq!(result.unwrap_err(), "Binding with id 'unknown' not found");
    }

    fn default_settings_json() -> serde_json::Value {
        serde_json::to_value(get_default_settings()).unwrap()
    }

    /// Every field must survive a partial store: a missing key must never fail
    /// the whole-settings parse (#1619). `json!({})` is the extreme case.
    #[test]
    fn empty_store_parses_with_defaults() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("all AppSettings fields need serde defaults");
        assert_eq!(
            settings.shortcut_activation,
            ShortcutActivation::HoldOrToggle
        );
        assert_eq!(settings.hold_threshold_ms, default_hold_threshold_ms());
        assert!(!settings.audio_feedback);
        assert!(settings.filler_word_removal_enabled);
        // Bindings default to empty; the load path merges the real defaults in.
        assert!(settings.bindings.is_empty());
    }

    /// Frozen snapshot of a real v0.9.0-era settings store, as written to
    /// disk. This pins backwards compatibility: it must always parse strictly
    /// (no salvage). Schema migrations may then rewrite fields whose native
    /// meaning changed.
    ///
    /// If a schema change breaks this test, do NOT just update the fixture —
    /// it stands in for the stores on users' machines. Add a
    /// `#[serde(alias)]`/`#[serde(other)]` or a one-time migration in
    /// `apply_settings_migrations` so old values keep loading, and only extend
    /// the fixture alongside that.
    #[test]
    fn frozen_v0_9_store_parses_strictly_then_migrates_device_index() {
        // Note "log_level": 2 — the legacy numeric format, kept deliberately.
        let stored: serde_json::Value = serde_json::from_str(
            r##"{
            "settings_schema_version": 1,
            "bindings": {
                "transcribe": {
                    "id": "transcribe",
                    "name": "Transcribe",
                    "description": "Converts your speech into text.",
                    "default_binding": "option+space",
                    "current_binding": "f13"
                },
                "cancel": {
                    "id": "cancel",
                    "name": "Cancel",
                    "description": "Cancels the current recording.",
                    "default_binding": "escape",
                    "current_binding": "escape"
                }
            },
            "push_to_talk": false,
            "audio_feedback": true,
            "audio_feedback_volume": 0.8,
            "sound_theme": "pop",
            "start_hidden": false,
            "autostart_enabled": true,
            "selected_model": "whisper-large-v3-turbo",
            "onboarding_completed": true,
            "always_on_microphone": false,
            "selected_microphone": "MacBook Pro Microphone",
            "clamshell_microphone": null,
            "selected_output_device": null,
            "translate_to_english": false,
            "selected_language": "en",
            "overlay_position": "bottom",
            "debug_mode": false,
            "log_level": 2,
            "custom_words": ["Handy", "cjpais"],
            "model_unload_timeout": "min5",
            "word_correction_threshold": 0.18,
            "paste_method": "ctrl_v",
            "clipboard_handling": "dont_modify",
            "auto_submit": false,
            "auto_submit_key": "enter",
            "mute_while_recording": false,
            "append_trailing_space": false,
            "app_language": "en",
            "experimental_enabled": false,
            "lazy_stream_close": false,
            "keyboard_implementation": "handy_keys",
            "show_tray_icon": true,
            "paste_delay_ms": 60,
            "typing_tool": "auto",
            "custom_filler_words": null,
            "transcribe_accelerator": "gpu",
            "ort_accelerator": "auto",
            "transcribe_gpu_device": 0,
            "extra_recording_buffer_ms": 0,
            "vad_enabled": true,
            "overlay_style": "live"
        }"##,
        )
        .expect("fixture is valid JSON");

        let mut settings: AppSettings = serde_json::from_value(stored.clone())
            .expect("a stored v0.9.0 settings object must keep parsing strictly");

        assert_eq!(settings.selected_model, "whisper-large-v3-turbo");
        assert_eq!(settings.bindings["transcribe"].current_binding, "f13");
        assert_eq!(settings.log_level, LogLevel::Debug);
        assert_eq!(settings.sound_theme, SoundTheme::Pop);
        assert!(settings.filler_word_removal_enabled);
        assert_eq!(settings.vad_backend, VadBackend::Silero);
        assert!(
            settings.long_form_chunking,
            "chunking is on for settings saved before it existed"
        );

        // The 0.1 integer device index is cleared once for transcribe.cpp 0.2.
        // Without an exact device, the retired generic GPU choice becomes Auto.
        assert!(apply_settings_migrations(&mut settings, &stored));
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        // The retired push_to_talk bool (false in this fixture) becomes the
        // matching legacy mode rather than the new hold-or-toggle default.
        assert_eq!(settings.shortcut_activation, ShortcutActivation::Toggle);
        assert_eq!(settings.transcribe_gpu_device, None);
    }

    #[test]
    fn salvage_preserves_valid_fields_when_one_value_is_invalid() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "selected_model".into(),
            serde_json::json!("parakeet-tdt-0.6b-v3"),
        );
        map.insert("onboarding_completed".into(), serde_json::json!(true));
        // An enum variant this build doesn't know, e.g. written by a newer
        // version before a downgrade.
        map.insert("sound_theme".into(), serde_json::json!("theremin"));
        stored["bindings"]["cancel"]["current_binding"] = serde_json::json!("f13");

        // Precondition: this is exactly the whole-store parse failure from
        // #1619 that used to reset everything to defaults.
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "parakeet-tdt-0.6b-v3");
        assert!(salvaged.onboarding_completed);
        assert_eq!(salvaged.bindings["cancel"].current_binding, "f13");
        assert_eq!(salvaged.sound_theme, default_sound_theme());
    }

    #[test]
    fn salvage_drops_only_wrong_typed_fields() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert("paste_delay_ms".into(), serde_json::json!("sixty"));
        map.insert("sound_theme".into(), serde_json::json!(42));
        map.insert("custom_words".into(), serde_json::json!(["handy"]));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.paste_delay_ms, default_paste_delay_ms());
        assert_eq!(salvaged.sound_theme, default_sound_theme());
        assert_eq!(salvaged.custom_words, vec!["handy".to_string()]);
    }

    #[test]
    fn salvage_of_poisoned_bindings_keeps_other_fields() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        // One malformed entry poisons the whole bindings map, but must not
        // take the rest of the settings down with it.
        map.insert(
            "bindings".into(),
            serde_json::json!({ "cancel": { "id": 42 } }),
        );
        map.insert("selected_model".into(), serde_json::json!("whisper-small"));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "whisper-small");
        let defaults = get_default_settings();
        assert_eq!(
            salvaged.bindings["cancel"].current_binding,
            defaults.bindings["cancel"].current_binding
        );
    }

    #[test]
    fn salvage_tolerates_unknown_keys() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "field_from_the_future".into(),
            serde_json::json!({ "nested": true }),
        );
        map.insert("selected_model".into(), serde_json::json!("kept"));
        map.insert("sound_theme".into(), serde_json::json!("theremin"));

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "kept");
        assert_eq!(salvaged.sound_theme, default_sound_theme());
    }

    #[test]
    fn salvage_of_non_object_store_falls_back_to_defaults() {
        for stored in [
            serde_json::json!("corrupt"),
            serde_json::json!(null),
            serde_json::json!([1, 2, 3]),
        ] {
            let salvaged = salvage_settings(&stored);
            assert_eq!(
                serde_json::to_value(&salvaged).unwrap(),
                default_settings_json()
            );
        }
    }

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn default_overlay_style_is_live_when_overlay_defaults_on() {
        let settings = get_default_settings();
        assert_eq!(settings.overlay_style, OverlayStyle::Live);
    }

    #[test]
    fn overlay_migration_keeps_disabled_overlay_off() {
        let mut settings = get_default_settings();

        // Legacy store: overlay was hidden via the retired position "none".
        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "none"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::None);
    }

    #[test]
    fn legacy_none_overlay_position_deserializes_to_bottom() {
        // A persisted "none" must not fail the whole settings load; the serde
        // alias folds it onto Bottom (visibility is owned by overlay_style).
        let raw = serde_json::json!({ "overlay_position": "none" });
        let position: OverlayPosition =
            serde_json::from_value(raw.get("overlay_position").unwrap().clone())
                .expect("legacy \"none\" should deserialize, not error");
        assert_eq!(position, OverlayPosition::Bottom);
    }

    #[test]
    fn overlay_migration_promotes_enabled_overlay_to_live() {
        let mut settings = get_default_settings();
        settings.overlay_position = OverlayPosition::Top;
        settings.overlay_style = OverlayStyle::Minimal;

        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "top"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::Live);
        assert_eq!(settings.overlay_position, OverlayPosition::Top);
    }

    #[test]
    fn chinese_script_migration_only_carries_over_legacy_intents() {
        for (intent, language, script) in [
            ("zh-Hans", "zh", ChineseScript::Simplified),
            ("zh-Hant", "zh", ChineseScript::TraditionalTw),
            ("auto", "auto", ChineseScript::AsTranscribed),
        ] {
            let mut settings = get_default_settings();
            settings.selected_language = intent.to_string();
            settings.chinese_script = ChineseScript::TraditionalTw;
            let raw = serde_json::json!({ "selected_language": intent });

            assert!(apply_settings_migrations(&mut settings, &raw));
            assert_eq!(settings.selected_language, language);
            assert_eq!(settings.chinese_script, script);
        }
    }

    /// Spoken languages backfilled for an onboarded upgrader with these stored
    /// settings (merged over the defaults, as a stored file deserializes).
    fn backfilled(stored: serde_json::Value) -> AppSettings {
        let mut raw = serde_json::json!({
            "onboarding_completed": true,
            "selected_language": "auto",
            "app_language": "en"
        });
        raw.as_object_mut()
            .unwrap()
            .extend(stored.as_object().unwrap().clone());
        let mut settings: AppSettings = serde_json::from_value(raw.clone()).unwrap();
        apply_settings_migrations(&mut settings, &raw);
        settings
    }

    #[test]
    fn spoken_languages_backfill_guesses_with_english_included() {
        for (stored, spoken) in [
            (
                serde_json::json!({ "selected_language": "de" }),
                vec!["de", "en"],
            ),
            (serde_json::json!({ "selected_language": "en" }), vec!["en"]),
            (
                serde_json::json!({ "selected_language": "zh-Hant" }),
                vec!["zh", "en"],
            ),
            (
                serde_json::json!({ "chinese_script": "traditional" }),
                vec!["zh", "en"],
            ),
            (
                serde_json::json!({ "chinese_script": "as_transcribed" }),
                vec!["en"],
            ),
            // A picked language leads, the app's follows.
            (
                serde_json::json!({ "selected_language": "fr", "app_language": "de" }),
                vec!["fr", "de", "en"],
            ),
        ] {
            assert_eq!(
                backfilled(stored.clone()).spoken_languages,
                spoken,
                "{stored}"
            );
        }
    }

    #[test]
    fn spoken_languages_backfill_keeps_cantonese_on_hong_kong_forms() {
        let settings = backfilled(serde_json::json!({
            "selected_language": "yue",
            "chinese_script": "traditional"
        }));

        assert_eq!(settings.spoken_languages, vec!["yue", "en"]);
        assert_eq!(settings.chinese_script, ChineseScript::TraditionalHk);
    }

    #[test]
    fn spoken_languages_backfill_leaves_chosen_and_unasked_alone() {
        let chosen = backfilled(serde_json::json!({
            "selected_language": "de",
            "spoken_languages": ["ja"]
        }));
        assert_eq!(chosen.spoken_languages, vec!["ja"]);

        let unasked = backfilled(serde_json::json!({
            "onboarding_completed": false,
            "selected_language": "de"
        }));
        assert!(unasked.spoken_languages.is_empty());
    }

    #[test]
    fn shortcut_activation_migration_maps_push_to_talk_true() {
        let mut settings = get_default_settings();
        let raw = serde_json::json!({
            "selected_model": "",
            "push_to_talk": true
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.shortcut_activation, ShortcutActivation::PushToTalk);
    }

    #[test]
    fn shortcut_activation_migration_maps_push_to_talk_false() {
        let mut settings = get_default_settings();
        let raw = serde_json::json!({
            "selected_model": "",
            "push_to_talk": false
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.shortcut_activation, ShortcutActivation::Toggle);
    }

    #[test]
    fn shortcut_activation_migration_respects_explicit_new_key() {
        let mut settings = get_default_settings();
        settings.shortcut_activation = ShortcutActivation::HoldOrToggle;
        let raw = serde_json::json!({
            "selected_model": "",
            "push_to_talk": true,
            "shortcut_activation": "hold_or_toggle"
        });

        apply_settings_migrations(&mut settings, &raw);
        assert_eq!(
            settings.shortcut_activation,
            ShortcutActivation::HoldOrToggle
        );
    }

    #[test]
    fn shortcut_activation_defaults_to_hold_or_toggle_without_legacy_key() {
        let mut settings = get_default_settings();
        let raw = serde_json::json!({ "selected_model": "" });

        apply_settings_migrations(&mut settings, &raw);
        assert_eq!(
            settings.shortcut_activation,
            ShortcutActivation::HoldOrToggle
        );
    }

    #[test]
    fn gpu_device_migration_resets_legacy_positive_selection_to_auto() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;

        let raw = serde_json::json!({
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        assert_eq!(settings.transcribe_gpu_device, None);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn gpu_device_migration_maps_v1_automatic_gpu_to_auto() {
        let raw = serde_json::json!({
            "settings_schema_version": 1,
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });
        let mut settings: AppSettings = serde_json::from_value(raw.clone()).unwrap();

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        assert_eq!(settings.transcribe_gpu_device, None);
    }

    #[test]
    fn gpu_device_migration_maps_current_automatic_gpu_to_auto() {
        let raw = serde_json::json!({
            "settings_schema_version": CURRENT_SETTINGS_SCHEMA_VERSION,
            "onboarding_completed": false,
            "overlay_style": "live",
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": null
        });
        let mut settings: AppSettings = serde_json::from_value(raw.clone()).unwrap();

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        assert_eq!(settings.transcribe_gpu_device, None);
    }

    #[test]
    fn gpu_device_migration_keeps_current_stable_selection() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;
        settings.transcribe_gpu_device = Some("[\"vulkan\",\"id\",\"0000:01:00.0\"]".into());

        let raw = serde_json::json!({
            "settings_schema_version": CURRENT_SETTINGS_SCHEMA_VERSION,
            "onboarding_completed": false,
            "overlay_style": "live",
            "chinese_script": "as_transcribed",
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": settings.transcribe_gpu_device
        });

        assert!(!apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_gpu_device.as_deref(),
            Some("[\"vulkan\",\"id\",\"0000:01:00.0\"]")
        );
    }
}

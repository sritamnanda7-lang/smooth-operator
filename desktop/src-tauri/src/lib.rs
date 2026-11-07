// SPDX-License-Identifier: MIT
// Modified from Handy v0.10.0 (Copyright (c) 2025 CJ Pais): see git history for changes.
// Handy's dictation flow (record, transcribe, paste) is stripped. Its paste,
// audio-feedback and language code stays for steps 3, 4 and 6 but nothing calls
// it yet, so dead-code warnings are silenced until step 3 wires it back in.
#![allow(dead_code)]

mod audio_feedback;
pub mod audio_toolkit;
mod autostart;
mod catalog;
mod chinese_script;
pub mod cli;
mod clipboard;
mod commands;
pub mod engine_supervisor;
mod helpers;
mod input;
mod language_detection;
mod languages;
mod managers;
mod memory;
mod notices;
mod overlay;
mod paste_tx;
pub mod portable;
mod secure_input;
mod settings;
mod shortcut;
mod so;
mod system_languages;
mod tray;
mod tray_i18n;
mod utils;
pub mod webkit_renderer;

pub use cli::CliArgs;
#[cfg(debug_assertions)]
use specta_typescript::{BigIntExportBehavior, Typescript};
use tauri_specta::{collect_commands, collect_events, Builder};
pub use utils::env_flag_enabled;

use env_filter::Builder as EnvFilterBuilder;
use managers::audio::AudioRecordingManager;
use managers::model::ModelManager;
use managers::transcription::TranscriptionManager;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tauri::image::Image;

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Listener, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

// Global atomic to store the file log level filter
// We use u8 to store the log::LevelFilter as a number
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);

/// When `true`, log records are also forwarded to the webview via the
/// `log://log` event for the debug panel's live log viewer. Gated on debug
/// mode — the live log viewer is its only consumer and only exists in debug
/// mode — so normal runs never broadcast log records (which can include file
/// paths or transcribed text) onto the frontend event bus. Synced at startup
/// and whenever debug mode is toggled (see `shortcut::change_debug_mode_setting`).
pub static WEBVIEW_LOG_STREAMING: AtomicBool = AtomicBool::new(false);

fn level_filter_from_u8(value: u8) -> log::LevelFilter {
    match value {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Trace,
    }
}

fn build_console_filter() -> env_filter::Filter {
    let mut builder = EnvFilterBuilder::new();

    match std::env::var("RUST_LOG") {
        Ok(spec) if !spec.trim().is_empty() => {
            if let Err(err) = builder.try_parse(&spec) {
                log::warn!(
                    "Ignoring invalid RUST_LOG value '{}': {}. Falling back to info-level console logging",
                    spec,
                    err
                );
                builder.filter_level(log::LevelFilter::Info);
            }
        }
        _ => {
            builder.filter_level(log::LevelFilter::Info);
        }
    }

    builder.build()
}

fn show_main_window(app: &AppHandle) {
    if let Some(main_window) = app.get_webview_window("main") {
        if let Err(e) = main_window.unminimize() {
            log::error!("Failed to unminimize webview window: {}", e);
        }
        if let Err(e) = main_window.show() {
            log::error!("Failed to show webview window: {}", e);
        }
        if let Err(e) = main_window.set_focus() {
            log::error!("Failed to focus webview window: {}", e);
        }
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
                log::error!("Failed to set activation policy to Regular: {}", e);
            }
        }
        return;
    }

    let webview_labels = app.webview_windows().keys().cloned().collect::<Vec<_>>();
    log::error!(
        "Main window not found. Webview labels: {:?}",
        webview_labels
    );
}

/// Choose the macOS activation policy the process *launches* with.
///
/// Must run between `build()` and `run()`: that is the only point where
/// `App::set_activation_policy` sets tao's initial policy, which
/// `applicationDidFinishLaunching` then applies directly. Calling the
/// `AppHandle` variant from `setup` (which Tauri runs on `RunEvent::Ready`,
/// i.e. after launch) is instead a runtime Regular → Accessory demotion of an
/// already-activated foreground app — the transition Apple documents as
/// unreliable, and what left a Dock icon behind for start-hidden and
/// login-item launches on macOS 26+ (#1787). Launching as Accessory avoids the
/// transition entirely; showing the window later promotes to Regular, which is
/// the supported direction.
///
/// Mirrors the show-window decision in `setup`: the app launches without a
/// Dock icon only when it will start hidden (setting or `--start-hidden`) AND a
/// tray icon is available (setting and not `--no-tray`). With no tray the Dock
/// icon stays as the only way back into the app (#903).
#[cfg(target_os = "macos")]
fn apply_startup_activation_policy(app: &mut tauri::App) {
    let cli_args = app.state::<CliArgs>().inner().clone();
    let settings = settings::get_settings(app.handle());

    let should_hide = settings.start_hidden || cli_args.start_hidden;
    let tray_available = settings.show_tray_icon && !cli_args.no_tray;

    if should_hide && tray_available {
        log::info!("Starting hidden with tray available: launching as Accessory (no Dock icon)");
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    }
}

#[allow(unused_variables)]
fn should_force_show_permissions_window(app: &AppHandle) -> bool {
    #[cfg(target_os = "windows")]
    {
        let model_manager = app.state::<Arc<ModelManager>>();
        let has_downloaded_models = model_manager
            .get_available_models()
            .iter()
            .any(|model| model.is_downloaded);

        if !has_downloaded_models {
            return false;
        }

        let status = commands::audio::get_windows_microphone_permission_status();
        if status.supported && status.overall_access == commands::audio::PermissionAccess::Denied {
            log::info!(
                "Windows microphone permissions are denied; forcing main window visible for onboarding"
            );
            return true;
        }
    }

    false
}

fn initialize_core_logic(app_handle: &AppHandle) {
    // Note: Enigo (keyboard/mouse simulation) is NOT initialized here.
    // The frontend is responsible for calling the `initialize_enigo` command
    // after onboarding completes. This avoids triggering permission dialogs
    // on macOS before the user is ready.

    // Initialize the managers. The audio recorder receives the streaming router
    // explicitly, so always-on microphone startup can wire live-preview frames
    // even before Tauri state is populated.
    let model_manager =
        Arc::new(ModelManager::new(app_handle).expect("Failed to initialize model manager"));
    let transcription_manager = Arc::new(
        TranscriptionManager::new(app_handle, model_manager.clone())
            .expect("Failed to initialize transcription manager"),
    );
    let recording_manager = Arc::new(
        AudioRecordingManager::new(app_handle, transcription_manager.stream_router())
            .expect("Failed to initialize recording manager"),
    );

    // Add managers to Tauri's managed state
    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(tray::TrayState::new());

    // Note: Shortcuts are NOT initialized here.
    // The frontend is responsible for calling the `initialize_shortcuts` command
    // after permissions are confirmed (on macOS) or after onboarding completes.
    // This matches the pattern used for Enigo initialization.

    // The macOS activation policy for a start-hidden launch is applied before
    // the event loop runs (see `apply_startup_activation_policy`), not here:
    // by the time `setup` runs the app has already launched as a Regular
    // (Dock) app, and demoting it at runtime is unreliable (#1787).

    // Get the current theme to set the appropriate initial icon
    let initial_theme = tray::get_current_theme(app_handle);

    // Choose the appropriate initial icon based on theme
    let initial_icon_path = tray::get_icon_path(initial_theme, tray::TrayIconState::Idle, false);

    let mut tray_builder = TrayIconBuilder::new()
        .icon(
            Image::from_path(
                app_handle
                    .path()
                    .resolve(initial_icon_path, tauri::path::BaseDirectory::Resource)
                    .unwrap(),
            )
            .unwrap(),
        )
        .tooltip(tray::tray_tooltip())
        .icon_as_template(true);

    // Windows notification-area convention: left click opens the app, right click
    // shows the menu. Elsewhere (macOS menu bar, Linux) the menu stays on left click.
    #[cfg(target_os = "windows")]
    {
        tray_builder = tray_builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                let opens_window = matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                );
                if opens_window {
                    show_main_window(tray.app_handle());
                }
            });
    }
    #[cfg(not(target_os = "windows"))]
    {
        tray_builder = tray_builder.show_menu_on_left_click(true);
    }

    let tray = tray_builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                show_main_window(app);
            }
            "secure_input_warning" => {
                // Full explanation lives in the settings-window banner
                show_main_window(app);
            }
            "notice" => {
                show_main_window(app);
            }
            "unload_model" => {
                let transcription_manager = app.state::<Arc<TranscriptionManager>>();
                if !transcription_manager.is_model_loaded() {
                    log::warn!("No model is currently loaded.");
                    return;
                }
                if let Err(e) = transcription_manager.ensure_model_unlocked() {
                    log::warn!("{}", e);
                    return;
                }
                transcription_manager.request_unload();
                log::info!("Model unloaded via tray.");
            }
            "cancel" => {
                use crate::utils::cancel_current_operation;

                // Use centralized cancellation that handles all operations
                cancel_current_operation(app);
            }
            "quit" => {
                app.exit(0);
            }
            id if id.starts_with("model_select:") => {
                let model_id = id.strip_prefix("model_select:").unwrap().to_string();
                let current_model = settings::get_settings(app).selected_model;
                if model_id == current_model {
                    return;
                }
                let app_clone = app.clone();
                std::thread::spawn(move || {
                    match commands::models::switch_active_model(&app_clone, &model_id) {
                        Ok(()) => {
                            log::info!("Model switched to {} via tray.", model_id);
                        }
                        Err(e) => {
                            log::error!("Failed to switch model via tray: {}", e);
                        }
                    }
                    tray::update_tray_menu(&app_clone);
                });
            }
            id if id.starts_with("language_select:") => {
                let language = id.strip_prefix("language_select:").unwrap().to_string();
                if let Err(e) =
                    shortcut::change_selected_language_setting(app.clone(), language.clone())
                {
                    log::error!("Failed to switch language via tray: {}", e);
                    return;
                }
                log::info!("Language switched to {} via tray.", language);
                tray::rebuild_tray_menu(app);
            }
            _ => {}
        })
        .build(app_handle)
        .unwrap();
    app_handle.manage(tray);

    // Initialize tray menu with idle state
    tray::update_tray_menu(app_handle);

    // Apply show_tray_icon setting
    let settings = settings::get_settings(app_handle);
    if !settings.show_tray_icon {
        tray::set_tray_visibility(app_handle, false);
    }

    // Refresh tray menu when model state changes
    let app_handle_for_listener = app_handle.clone();
    app_handle.listen("model-state-changed", move |_| {
        tray::update_tray_menu(&app_handle_for_listener);
    });
    // A download or delete can change what the language submenu offers (the
    // language detector arriving makes Auto usable).
    for event in ["model-download-complete", "model-deleted"] {
        let app_handle_for_listener = app_handle.clone();
        app_handle.listen(event, move |_| {
            tray::update_tray_menu(&app_handle_for_listener);
        });
    }

    // Apply the autostart preference (SMAppService login item on macOS 13+,
    // tauri-plugin-autostart elsewhere)
    autostart::apply_autostart(app_handle, settings.autostart_enabled);

    // Create the recording overlay window (hidden by default)
    utils::create_recording_overlay(app_handle);
}

#[tauri::command]
#[specta::specta]
fn show_main_window_command(app: AppHandle) -> Result<(), String> {
    show_main_window(&app);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(cli_args: CliArgs) {
    // Avoid ggml-metal residency-set teardown assertions when a native engine
    // outlives the Tauri shutdown sequence (#1902). This must happen before
    // transcribe-cpp initializes its Metal device. Advanced users can restore
    // upstream residency behavior with HANDY_METAL_RESIDENCY=1.
    #[cfg(target_os = "macos")]
    if std::env::var("HANDY_METAL_RESIDENCY").as_deref() == Ok("1") {
        // ggml treats GGML_METAL_NO_RESIDENCY as presence-based, so remove an
        // inherited value as well when explicitly opting back in.
        std::env::remove_var("GGML_METAL_NO_RESIDENCY");
    } else {
        std::env::set_var("GGML_METAL_NO_RESIDENCY", "1");
    }

    // Pin glibc's dynamic mmap threshold before the first large allocation,
    // so per-dictation transient buffers are returned to the OS on free
    // instead of accumulating in malloc arenas (#1792). No-op off Linux/glibc.
    memory::init_allocator();

    // Detect portable mode before anything else
    portable::init();

    // Parse console logging directives from RUST_LOG, falling back to info-level logging
    // when the variable is unset
    let console_filter = build_console_filter();

    let specta_builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            shortcut::change_binding,
            shortcut::reset_binding,
            shortcut::change_shortcut_activation_setting,
            shortcut::change_hold_threshold_ms_setting,
            shortcut::change_audio_feedback_setting,
            shortcut::change_audio_feedback_volume_setting,
            shortcut::change_sound_theme_setting,
            shortcut::change_theme_setting,
            shortcut::change_start_hidden_setting,
            shortcut::change_autostart_setting,
            shortcut::change_translate_to_english_setting,
            shortcut::change_selected_language_setting,
            shortcut::change_spoken_languages_setting,
            shortcut::change_overlay_position_setting,
            shortcut::change_overlay_style_setting,
            shortcut::change_debug_mode_setting,
            shortcut::change_word_correction_threshold_setting,
            shortcut::change_extra_recording_buffer_setting,
            shortcut::change_language_detection_setting,
            shortcut::change_language_detection_window_setting,
            shortcut::change_paste_delay_ms_setting,
            shortcut::change_paste_delay_after_ms_setting,
            shortcut::change_reliable_paste_setting,
            shortcut::change_paste_method_setting,
            shortcut::get_available_typing_tools,
            shortcut::change_typing_tool_setting,
            shortcut::change_clipboard_handling_setting,
            shortcut::change_auto_submit_setting,
            shortcut::change_auto_submit_key_setting,
            shortcut::change_experimental_enabled_setting,
            shortcut::update_custom_words,
            shortcut::suspend_all_bindings,
            shortcut::resume_all_bindings,
            shortcut::change_mute_while_recording_setting,
            shortcut::change_append_trailing_space_setting,
            shortcut::change_lazy_stream_close_setting,
            shortcut::change_vad_enabled_setting,
            shortcut::change_long_form_chunking_setting,
            shortcut::change_vad_backend_setting,
            shortcut::change_filler_word_removal_enabled_setting,
            shortcut::change_chinese_script_setting,
            shortcut::change_app_language_setting,
            shortcut::change_keyboard_implementation_setting,
            shortcut::get_keyboard_implementation,
            shortcut::change_show_tray_icon_setting,
            shortcut::change_transcribe_accelerator_setting,
            shortcut::change_transcribe_gpu_device,
            shortcut::get_available_accelerators,
            shortcut::handy_keys::start_handy_keys_recording,
            shortcut::handy_keys::stop_handy_keys_recording,
            secure_input::get_secure_input_status,
            secure_input::run_keyboard_diagnostic,
            show_main_window_command,
            commands::cancel_operation,
            commands::is_portable,
            commands::get_app_dir_path,
            commands::get_app_settings,
            commands::get_default_settings,
            commands::get_log_dir_path,
            commands::set_log_level,
            commands::open_log_dir,
            commands::open_app_data_dir,
            commands::initialize_enigo,
            commands::initialize_shortcuts,
            commands::complete_onboarding,
            commands::models::get_available_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::download_language_detector,
            commands::models::is_language_detector_downloaded,
            commands::models::delete_model,
            commands::models::get_retired_models,
            commands::models::delete_retired_model,
            commands::models::cancel_download,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::get_transcription_model_status,
            commands::models::is_model_loading,
            commands::models::rescan_local_models,
            commands::audio::update_microphone_mode,
            commands::audio::get_microphone_mode,
            commands::audio::get_windows_microphone_permission_status,
            commands::audio::open_microphone_privacy_settings,
            commands::audio::get_available_microphones,
            commands::audio::set_selected_microphone,
            commands::audio::get_selected_microphone,
            commands::audio::get_available_output_devices,
            commands::audio::set_selected_output_device,
            commands::audio::get_selected_output_device,
            commands::audio::play_test_sound,
            commands::audio::check_custom_sounds,
            commands::audio::set_clamshell_microphone,
            commands::audio::get_clamshell_microphone,
            commands::audio::is_recording,
            commands::audio::get_microphone_channels,
            commands::audio::set_selected_channel,
            commands::audio::change_airpods_mode_setting,
            commands::transcription::set_model_unload_timeout,
            commands::transcription::get_model_load_status,
            commands::transcription::get_language_status,
            commands::transcription::unload_model_manually,
            helpers::clamshell::is_laptop,
            notices::get_active_notices,
            notices::dismiss_notice,
            notices::dismiss_overlay_notice,
        ])
        .events(collect_events![
            managers::transcription::StreamTextEvent,
            managers::transcription::StreamPhaseEvent,
            notices::NoticesChanged,
        ]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    specta_builder
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    let invoke_handler = specta_builder.invoke_handler();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                // Key path: never above Info, even with --debug / Trace, so no
                // key event detail can reach a log (our shortcut code and the crate).
                .level_for("handy_keys", log::LevelFilter::Info)
                .level_for("handy_app_lib::shortcut::handy_keys", log::LevelFilter::Info)
                .max_file_size(500_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Console output respects RUST_LOG environment variable.
                    Target::new(TargetKind::Stdout)
                    .filter({
                        let console_filter = console_filter.clone();
                        move |metadata| console_filter.enabled(metadata)
                    }),
                    // File logs respect the user's settings (stored in FILE_LOG_LEVEL atomic)
                    Target::new(if let Some(data_dir) = portable::data_dir() {
                        TargetKind::Folder {
                            path: data_dir.join("logs"),
                            file_name: Some("smooth-operator".into()),
                        }
                    } else {
                        TargetKind::LogDir {
                            file_name: Some("smooth-operator".into()),
                        }
                    })
                    .filter(|metadata| {
                        let file_level = FILE_LOG_LEVEL.load(Ordering::Relaxed);
                        metadata.level() <= level_filter_from_u8(file_level)
                    }),
                    // Stream logs to the webview (via the `log://log` event) so the
                    // debug panel's live log viewer can show them in real time. Only
                    // active while debug mode is on (its sole consumer), and shares the
                    // file log level so the "Log Level" setting controls verbosity.
                    Target::new(TargetKind::Webview).filter(|metadata| {
                        WEBVIEW_LOG_STREAMING.load(Ordering::Relaxed)
                            && metadata.level()
                                <= level_filter_from_u8(FILE_LOG_LEVEL.load(Ordering::Relaxed))
                    }),
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    // A second launch only brings the settings window forward. It carries no
    // commands: arguments are ignored. (On macOS, relaunching the bundle from
    // Spotlight/Finder/Dock arrives as RunEvent::Reopen below instead.)
    builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        #[cfg(target_os = "macos")]
        tray::recreate_tray_icon(app);
        show_main_window(app);
    }));

    // The updater exists only in spike builds (cargo feature `spike`).
    #[cfg(feature = "spike")]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    #[allow(unused_mut)]
    let mut app = builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(system_languages::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(cli_args.clone())
        .manage(notices::NoticeStore::default())
        .setup(move |app| {
            #[cfg(target_os = "windows")]
            log::info!(
                "Vulkan layer policy: VK_LOADER_LAYERS_DISABLE={:?}, HANDY_KEEP_VULKAN_IMPLICIT_LAYERS={}",
                std::env::var_os("VK_LOADER_LAYERS_DISABLE"),
                utils::env_flag_enabled("HANDY_KEEP_VULKAN_IMPLICIT_LAYERS"),
            );
            system_languages::log_system_languages();

            specta_builder.mount_events(app);

            // Create main window programmatically so we can set data_directory
            // for portable mode (redirects WebView2 cache to portable Data dir)
            let mut win_builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title("Smooth Operator")
                    .inner_size(utils::MAIN_WINDOW_SIZE.0, utils::MAIN_WINDOW_SIZE.1)
                    .min_inner_size(utils::MAIN_WINDOW_SIZE.0, utils::MAIN_WINDOW_SIZE.1)
                    .resizable(true)
                    .maximizable(true)
                    .visible(false);

            if let Some(data_dir) = portable::data_dir() {
                win_builder = win_builder.data_directory(data_dir.join("webview"));
            }

            // Only used on macOS (to size it) and Windows (to disable WebView2
            // browser accelerators).
            #[cfg_attr(
                not(any(target_os = "macos", target_os = "windows")),
                allow(unused_variables)
            )]
            let main_window = win_builder.build()?;

            // macOS draws the title bar inside the inner size; grow the window to fit it.
            #[cfg(target_os = "macos")]
            {
                let bar = utils::measure_title_bar(&main_window);
                if bar > 0.0 {
                    let (width, height) = utils::MAIN_WINDOW_SIZE;
                    let size = tauri::LogicalSize::new(width, height + bar);
                    let _ = main_window.set_min_size(Some(size));
                    let _ = main_window.set_size(size);
                    let _ = main_window.center();
                }
            }

            // Disable WebView2 browser accelerators (F5, F6, Ctrl+F, F12, ...).
            // A settings window has no use for them, and pressing F6 while
            // recording a shortcut was reported to turn the whole window white
            // (cjpais/Handy#1940), likely by triggering WebView2 focus cycling.
            // DevTools stays enabled; only the F12 accelerator is lost.
            #[cfg(target_os = "windows")]
            {
                let _ = main_window.with_webview(|webview| unsafe {
                    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
                    use windows::core::Interface;

                    let result = webview
                        .controller()
                        .CoreWebView2()
                        .and_then(|core| core.Settings())
                        .and_then(|settings| settings.cast::<ICoreWebView2Settings3>())
                        .and_then(|settings| settings.SetAreBrowserAcceleratorKeysEnabled(false));

                    if let Err(error) = result {
                        log::warn!("Failed to disable WebView2 browser accelerators: {error}");
                    }
                });
            }

            let mut settings = get_settings(app.handle());

            // Apply the persisted appearance theme to the native title bar before
            // the window is shown, so it matches the in-app palette without a flash
            // of the wrong theme. See `apply_window_theme` for what this does per
            // platform.
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            shortcut::apply_window_theme(app.handle(), settings.theme);

            // CLI --debug flag overrides debug_mode and log level (runtime-only, not persisted)
            if cli_args.debug {
                settings.debug_mode = true;
                settings.log_level = settings::LogLevel::Trace;
            }

            let tauri_log_level: tauri_plugin_log::LogLevel = settings.log_level.into();
            let file_log_level: log::Level = tauri_log_level.into();
            // Store the file log level in the atomic for the filter to use
            FILE_LOG_LEVEL.store(file_log_level.to_level_filter() as u8, Ordering::Relaxed);
            // Only forward logs to the webview while debug mode is on (the live log
            // viewer is the sole consumer and only exists in debug mode). This also
            // honors the runtime `--debug` override applied to `settings` above.
            WEBVIEW_LOG_STREAMING.store(settings.debug_mode, Ordering::Relaxed);
            let app_handle = app.handle().clone();

            initialize_core_logic(&app_handle);

            // Secure Input monitor (macOS): detects stuck secure input that
            // silently blocks keyed shortcuts, warns the user, and activates
            // the Carbon fallback. See secure_input.rs and issue #1578.
            secure_input::init(&app_handle);

            // Populate the overlay-enabled cache from initial settings so the
            // audio path (overlay::emit_levels, called ~24 Hz during recording)
            // can do a single atomic load instead of reading the Tauri store.
            // Kept in sync by shortcut::change_overlay_style_setting.
            overlay::update_overlay_enabled_cache(
                settings.overlay_style != settings::OverlayStyle::None,
            );

            // Pre-warm GPU/accelerator enumeration on a background thread. The first
            // device listing opens the GPU, which on macOS loads ggml's Metal library
            // and compiles it when the system shader cache does not have it yet, so it
            // stays off the startup path. get_available_accelerators then enumerates
            // ORT execution providers and transcribe-cpp compute devices; without this
            // the cost is paid synchronously when the user first opens Advanced
            // settings, freezing the UI. The device list is kept by the
            // transcription engine and refreshed by every worker it starts that may
            // use the GPU (CPU-only workers can't see it).
            let devices_app_handle = app_handle.clone();
            std::thread::spawn(move || {
                let tm = devices_app_handle.state::<Arc<TranscriptionManager>>();
                crate::managers::transcription::report_compute_devices(&tm);
                let _ = crate::managers::transcription::get_available_accelerators(&tm);
            });

            // Hide tray icon if --no-tray was passed
            if cli_args.no_tray {
                tray::set_tray_visibility(&app_handle, false);
            }

            // Show main window only if not starting hidden.
            // CLI --start-hidden flag overrides the setting.
            // But if permission onboarding is required, always show the window.
            let should_hide = settings.start_hidden || cli_args.start_hidden;
            let should_force_show = should_force_show_permissions_window(&app_handle);

            // If start_hidden but tray is disabled, we must show the window
            // anyway. Without a tray icon, the dock is the only way back in.
            // Keep in sync with `apply_startup_activation_policy` (macOS).
            let tray_available = settings.show_tray_icon && !cli_args.no_tray;
            if should_force_show || !should_hide || !tray_available {
                show_main_window(&app_handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _res = window.hide();

                #[cfg(target_os = "macos")]
                {
                    let settings = get_settings(window.app_handle());
                    let tray_visible =
                        settings.show_tray_icon && !window.app_handle().state::<CliArgs>().no_tray;
                    if tray_visible {
                        // Tray is available: hide the dock icon, app lives in the tray
                        let res = window
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                        if let Err(e) = res {
                            log::error!("Failed to set activation policy: {}", e);
                        }
                    }
                    // No tray: keep the dock icon visible so the user can reopen
                }
            }
            tauri::WindowEvent::ThemeChanged(theme) => {
                log::info!("Theme changed to: {:?}", theme);
                // Re-apply the current tray state with the new theme's icon set
                utils::refresh_tray_icon(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(invoke_handler)
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Must sit between build() and run(): see the doc comment.
    #[cfg(target_os = "macos")]
    apply_startup_activation_policy(&mut app);

    // `app` is only used by the macOS arm.
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    app.run(|app, event| match &event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            // Fired when the already-running bundle is launched again from
            // Spotlight/Finder or the Dock icon is clicked. If the settings
            // window is hidden, the user is likely looking for a tray icon
            // that vanished (#1948): recreate it. When the window is
            // already visible this is just a focus request and the tray is
            // left alone.
            let window_visible = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            if !window_visible {
                tray::recreate_tray_icon(app);
            }
            show_main_window(app);
        }
        // No transcription teardown on exit: transcribe.cpp runs only in the
        // worker process, which exits by itself as soon as this process's end
        // of its stdin closes, even if it is hung. Waiting on an unload here
        // could block quitting behind a hung model load.
        _ => {}
    });
}

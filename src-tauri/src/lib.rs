mod audio;
mod whisper;

use audio::AudioRecorder;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use whisper::{get_available_models, get_models_directory, ModelInfo, SharedWhisperEngine};

// Application state
pub struct AppState {
    pub recorder: Arc<Mutex<AudioRecorder>>,
    pub whisper: SharedWhisperEngine,
    pub settings: Arc<Mutex<Settings>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub model_filename: String,
    pub language: String,
    pub hotkey: String,
    pub auto_paste: bool,
    pub show_notification: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_filename: "ggml-base.bin".to_string(),
            language: "auto".to_string(),
            hotkey: "Super+Shift+Space".to_string(),
            auto_paste: true,
            show_notification: true,
        }
    }
}

// ===== Tauri Commands =====

#[tauri::command]
async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    let mut recorder = state.recorder.lock().map_err(|e| e.to_string())?;
    recorder.start_recording()
}

#[tauri::command]
async fn stop_recording(state: State<'_, AppState>) -> Result<String, String> {
    let samples = {
        let mut recorder = state.recorder.lock().map_err(|e| e.to_string())?;
        recorder.stop_recording()?
    };

    if samples.is_empty() {
        return Err("No audio recorded".to_string());
    }

    // Get language setting
    let language = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        if settings.language == "auto" {
            None
        } else {
            Some(settings.language.clone())
        }
    };

    // Transcribe
    let whisper = state.whisper.lock().map_err(|e| e.to_string())?;
    if !whisper.is_loaded() {
        return Err("Model not loaded. Please load a model first.".to_string());
    }

    whisper.transcribe(&samples, language.as_deref())
}

#[tauri::command]
async fn get_audio_level(state: State<'_, AppState>) -> Result<f32, String> {
    let recorder = state.recorder.lock().map_err(|e| e.to_string())?;
    Ok(recorder.get_audio_level())
}

#[tauri::command]
async fn is_recording(state: State<'_, AppState>) -> Result<bool, String> {
    let recorder = state.recorder.lock().map_err(|e| e.to_string())?;
    Ok(recorder.is_recording())
}

#[tauri::command]
async fn load_model(state: State<'_, AppState>, filename: String) -> Result<(), String> {
    let model_path = get_models_directory().join(&filename);
    
    let mut whisper = state.whisper.lock().map_err(|e| e.to_string())?;
    whisper.load_model(model_path)?;
    
    // Update settings
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.model_filename = filename;
    
    Ok(())
}

#[tauri::command]
async fn is_model_loaded(state: State<'_, AppState>) -> Result<bool, String> {
    let whisper = state.whisper.lock().map_err(|e| e.to_string())?;
    Ok(whisper.is_loaded())
}

#[tauri::command]
fn get_models() -> Vec<ModelInfo> {
    get_available_models()
}

#[tauri::command]
fn get_models_dir() -> String {
    get_models_directory().to_string_lossy().to_string()
}

#[tauri::command]
fn get_downloaded_models() -> Vec<String> {
    let models_dir = get_models_directory();
    std::fs::read_dir(&models_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.ends_with(".bin") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
async fn download_model(
    app: AppHandle,
    model: ModelInfo,
) -> Result<(), String> {
    use futures_util::StreamExt;

    let models_dir = get_models_directory();
    let model_path = models_dir.join(&model.filename);

    if model_path.exists() {
        return Ok(());
    }

    // Create a temp file for downloading
    let temp_path = model_path.with_extension("bin.part");

    let client = reqwest::Client::new();
    let response = client
        .get(&model.url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        
        use tokio::io::AsyncWriteExt;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;

        // Emit progress event
        let progress = if total_size > 0 {
            (downloaded as f64 / total_size as f64 * 100.0) as u32
        } else {
            0
        };

        app.emit("download-progress", serde_json::json!({
            "filename": model.filename,
            "progress": progress,
            "downloaded": downloaded,
            "total": total_size,
        })).ok();
    }

    file.sync_all()
        .await
        .map_err(|e| format!("Sync error: {}", e))?;

    // Rename temp file to final name
    tokio::fs::rename(&temp_path, &model_path)
        .await
        .map_err(|e| format!("Rename error: {}", e))?;

    app.emit("download-complete", &model.filename).ok();

    Ok(())
}

#[tauri::command]
async fn delete_model(filename: String) -> Result<(), String> {
    let model_path = get_models_directory().join(&filename);
    if model_path.exists() {
        std::fs::remove_file(&model_path)
            .map_err(|e| format!("Failed to delete model: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let mut current = state.settings.lock().map_err(|e| e.to_string())?;
    *current = settings;
    Ok(())
}

#[tauri::command]
fn get_input_devices() -> Vec<String> {
    audio::get_input_devices()
}

#[tauri::command]
async fn type_text(text: String) -> Result<(), String> {
    // For Hyprland/Wayland: Use wl-copy + wtype or ydotool
    // First, copy to clipboard
    let copy_result = Command::new("wl-copy")
        .arg(&text)
        .output();

    if copy_result.is_err() {
        // Fallback to xclip for X11
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(text.as_bytes())?;
                }
                child.wait()
            })
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    // Small delay before pasting
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Try wtype first (Wayland)
    let wtype_result = Command::new("wtype")
        .arg("-M")
        .arg("ctrl")
        .arg("-P")
        .arg("v")
        .arg("-m")
        .arg("ctrl")
        .output();

    if wtype_result.is_err() {
        // Fallback to ydotool
        let ydotool_result = Command::new("ydotool")
            .args(["key", "29:1", "47:1", "47:0", "29:0"]) // Ctrl+V
            .output();

        if ydotool_result.is_err() {
            // Final fallback to xdotool (X11)
            Command::new("xdotool")
                .args(["key", "ctrl+v"])
                .output()
                .map_err(|e| format!("Failed to paste text: {}", e))?;
        }
    }

    Ok(())
}

#[tauri::command]
async fn copy_to_clipboard(text: String) -> Result<(), String> {
    // Try wl-copy first (Wayland)
    let result = Command::new("wl-copy")
        .arg(&text)
        .output();

    if result.is_err() {
        // Fallback to xclip (X11)
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(text.as_bytes())?;
                }
                child.wait()
            })
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    Ok(())
}

fn setup_global_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Default hotkey: Super+Shift+Space
    let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);

    let app_handle = app.clone();
    
    app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            // Emit event to frontend
            app_handle.emit("hotkey-pressed", ()).ok();
        }
    })?;

    app.global_shortcut().register(shortcut)?;

    println!("Global shortcut registered: Super+Shift+Space");
    Ok(())
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &settings, &quit])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("HyprWhisper - Speech to Text")
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        window.show().ok();
                        window.set_focus().ok();
                    }
                }
                "settings" => {
                    app.emit("open-settings", ()).ok();
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            recorder: Arc::new(Mutex::new(AudioRecorder::new())),
            whisper: whisper::create_shared_engine(),
            settings: Arc::new(Mutex::new(Settings::default())),
        })
        .setup(|app| {
            // Setup global shortcut
            if let Err(e) = setup_global_shortcut(app.handle()) {
                eprintln!("Failed to setup global shortcut: {}", e);
            }

            // Setup tray
            if let Err(e) = setup_tray(app.handle()) {
                eprintln!("Failed to setup tray: {}", e);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_audio_level,
            is_recording,
            load_model,
            is_model_loaded,
            get_models,
            get_models_dir,
            get_downloaded_models,
            download_model,
            delete_model,
            get_settings,
            save_settings,
            get_input_devices,
            type_text,
            copy_to_clipboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

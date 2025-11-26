mod audio;
mod whisper;

use audio::AudioRecorder;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};
use whisper::{get_available_models, get_models_directory, ModelInfo, SharedWhisperEngine};

// Application state
pub struct AppState {
    pub recorder: Arc<Mutex<AudioRecorder>>,
    pub whisper: SharedWhisperEngine,
    pub settings: Arc<Mutex<Settings>>,
    pub previous_window: Arc<Mutex<Option<String>>>,
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
            hotkey: "Ctrl+Shift+.".to_string(),
            auto_paste: true,
            show_notification: true,
        }
    }
}

// ===== Helper Functions =====

/// Get the currently focused window address using hyprctl
fn get_active_window_address() -> Option<String> {
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;
    
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("address")?.as_str().map(|s| s.to_string())
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
        return Ok(String::new());
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

/// Stop recording without transcribing - just cleanup
#[tauri::command]
fn stop_recording_silent(state: State<'_, AppState>) -> Result<(), String> {
    let mut recorder = state.recorder.lock().map_err(|e| e.to_string())?;
    let _ = recorder.stop_recording();
    Ok(())
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

/// Transcribe current audio buffer without stopping recording (for real-time preview)
#[tauri::command]
async fn transcribe_current(state: State<'_, AppState>) -> Result<String, String> {
    let samples = {
        let recorder = state.recorder.lock().map_err(|e| e.to_string())?;
        recorder.get_current_samples()
    };

    if samples.is_empty() {
        return Ok(String::new());
    }

    // Need at least 0.5 seconds of audio (8000 samples at 16kHz)
    if samples.len() < 8000 {
        return Ok(String::new());
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
        return Err("Model not loaded".to_string());
    }

    whisper.transcribe_chunk(&samples, language.as_deref())
}

/// Get sample count for tracking transcription progress
#[tauri::command]
async fn get_sample_count(state: State<'_, AppState>) -> Result<usize, String> {
    let recorder = state.recorder.lock().map_err(|e| e.to_string())?;
    Ok(recorder.get_sample_count())
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

/// Called when user finishes dictation - handles everything synchronously before closing
#[tauri::command]
fn finish_and_paste(app: AppHandle, state: State<'_, AppState>, text: String) {
    // Stop recording immediately
    {
        let mut recorder = state.recorder.lock().unwrap();
        let _ = recorder.stop_recording();
    }
    
    // Get the previous window address
    let prev_window = state.previous_window.lock().unwrap().clone();
    println!("Previous window: {:?}", prev_window);
    
    // Hide the window immediately (visually disappears but app keeps running)
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    
    // Skip if no text
    if text.trim().is_empty() {
        println!("No text to paste, exiting");
        app.exit(0);
        return;
    }
    
    let text = text.trim().to_string();
    println!("Text to type: {}", text);
    
    // Small delay for window to fully hide
    thread::sleep(Duration::from_millis(50));
    
    // Also copy to clipboard as backup (user can Ctrl+V if wtype fails)
    println!("Copying to clipboard as backup...");
    let mut child = match Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Failed to spawn wl-copy: {}", e);
            // Continue anyway, wtype might still work
            app.exit(1);
            return;
        }
    };
    
    // Write text to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        if let Err(e) = stdin.write_all(text.as_bytes()) {
            eprintln!("Failed to write to wl-copy stdin: {}", e);
        }
        // stdin is dropped here, closing the pipe
    }
    
    // Wait for wl-copy to fork (it will exit quickly after forking)
    let _ = child.wait();
    
    // Focus previous window if we have one
    if let Some(addr) = prev_window {
        println!("Focusing window: {}", addr);
        let focus_result = Command::new("hyprctl")
            .args(["dispatch", "focuswindow", &format!("address:{}", addr)])
            .output();
        
        match focus_result {
            Ok(output) => {
                println!("Focus result: {}", String::from_utf8_lossy(&output.stdout));
                if !output.status.success() {
                    eprintln!("Focus stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
            Err(e) => {
                eprintln!("Failed to focus window: {}", e);
            }
        }
        
        // Wait for focus to complete
        thread::sleep(Duration::from_millis(150));
    } else {
        println!("No previous window to focus");
    }
    
    // Type text directly using wtype (more reliable than Ctrl+V for terminals/IDEs)
    println!("Typing text with wtype...");
    let type_result = Command::new("wtype")
        .arg("--")
        .arg(&text)
        .output();
    
    match type_result {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("wtype failed: {}", String::from_utf8_lossy(&output.stderr));
            } else {
                println!("Text typed successfully");
            }
        }
        Err(e) => {
            eprintln!("Failed to run wtype: {}", e);
        }
    }
    
    // Now exit the app
    println!("Exiting app");
    app.exit(0);
}

/// Called on cancel - just cleanup and close
#[tauri::command]
fn cancel_recording(app: AppHandle, state: State<'_, AppState>) {
    // Stop recording
    {
        let mut recorder = state.recorder.lock().unwrap();
        let _ = recorder.stop_recording();
    }
    
    // Exit the app
    app.exit(0);
}

fn setup_global_shortcut(_app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Global shortcuts have issues on Wayland/Hyprland
    // For now, users can use the app window and press Space to record
    // TODO: Implement proper Wayland global shortcut support via portal or hyprland IPC
    println!("Note: Global shortcuts disabled on Wayland. Use the app window (Space key) to record.");
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
    // Capture the previous window BEFORE we create our window
    let previous_window = get_active_window_address();
    println!("Captured previous window at startup: {:?}", previous_window);
    
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
            previous_window: Arc::new(Mutex::new(previous_window)),
        })
        .setup(|app| {
            // Set WebView background to transparent on Linux
            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    use webkit2gtk::WebViewExt;
                    
                    let _ = window.with_webview(|webview| {
                        // Get the webkit2gtk WebView and set transparent background
                        let wv = webview.inner();
                        let rgba = gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0);
                        wv.set_background_color(&rgba);
                    });
                }
            }
            
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
            stop_recording_silent,
            get_audio_level,
            is_recording,
            transcribe_current,
            get_sample_count,
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
            finish_and_paste,
            cancel_recording,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

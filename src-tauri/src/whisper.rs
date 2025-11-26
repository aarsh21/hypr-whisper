use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperEngine {
    context: Option<WhisperContext>,
    model_path: Option<PathBuf>,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            context: None,
            model_path: None,
        }
    }

    pub fn load_model(&mut self, model_path: PathBuf) -> Result<(), String> {
        if !model_path.exists() {
            return Err(format!("Model file not found: {:?}", model_path));
        }

        let params = WhisperContextParameters::default();
        
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or("Invalid path")?,
            params,
        )
        .map_err(|e| format!("Failed to load Whisper model: {}", e))?;

        self.context = Some(ctx);
        self.model_path = Some(model_path);

        println!("Whisper model loaded successfully");
        Ok(())
    }

    pub fn transcribe(&self, audio_samples: &[f32], language: Option<&str>) -> Result<String, String> {
        let ctx = self.context.as_ref().ok_or("Model not loaded")?;

        let mut state = ctx
            .create_state()
            .map_err(|e| format!("Failed to create state: {}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        
        // Configure for best results
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_translate(false);
        params.set_single_segment(false);
        params.set_no_context(true);
        
        // Set language if specified
        if let Some(lang) = language {
            params.set_language(Some(lang));
        } else {
            // Auto-detect language
            params.set_language(None);
        }

        // Run inference
        state
            .full(params, audio_samples)
            .map_err(|e| format!("Transcription failed: {}", e))?;

        // Collect results
        let num_segments = state
            .full_n_segments()
            .map_err(|e| format!("Failed to get segments: {}", e))?;

        let mut result = String::new();
        for i in 0..num_segments {
            let segment = state
                .full_get_segment_text(i)
                .map_err(|e| format!("Failed to get segment {}: {}", i, e))?;
            result.push_str(&segment);
        }

        Ok(result.trim().to_string())
    }

    pub fn is_loaded(&self) -> bool {
        self.context.is_some()
    }

    pub fn get_model_path(&self) -> Option<&PathBuf> {
        self.model_path.as_ref()
    }
}

impl Default for WhisperEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Thread-safe wrapper for the engine
pub type SharedWhisperEngine = Arc<Mutex<WhisperEngine>>;

pub fn create_shared_engine() -> SharedWhisperEngine {
    Arc::new(Mutex::new(WhisperEngine::new()))
}

// Model information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub filename: String,
    pub size_mb: u64,
    pub url: String,
    pub description: String,
}

pub fn get_available_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            name: "Tiny".to_string(),
            filename: "ggml-tiny.bin".to_string(),
            size_mb: 75,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin".to_string(),
            description: "Fastest, least accurate. Good for testing.".to_string(),
        },
        ModelInfo {
            name: "Tiny (English)".to_string(),
            filename: "ggml-tiny.en.bin".to_string(),
            size_mb: 75,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin".to_string(),
            description: "Tiny model, English only. Faster than multilingual.".to_string(),
        },
        ModelInfo {
            name: "Base".to_string(),
            filename: "ggml-base.bin".to_string(),
            size_mb: 142,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin".to_string(),
            description: "Good balance of speed and accuracy.".to_string(),
        },
        ModelInfo {
            name: "Base (English)".to_string(),
            filename: "ggml-base.en.bin".to_string(),
            size_mb: 142,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin".to_string(),
            description: "Base model, English only.".to_string(),
        },
        ModelInfo {
            name: "Small".to_string(),
            filename: "ggml-small.bin".to_string(),
            size_mb: 466,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin".to_string(),
            description: "Good accuracy, moderate speed.".to_string(),
        },
        ModelInfo {
            name: "Small (English)".to_string(),
            filename: "ggml-small.en.bin".to_string(),
            size_mb: 466,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin".to_string(),
            description: "Small model, English only.".to_string(),
        },
        ModelInfo {
            name: "Medium".to_string(),
            filename: "ggml-medium.bin".to_string(),
            size_mb: 1500,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin".to_string(),
            description: "High accuracy, slower. Recommended for quality.".to_string(),
        },
        ModelInfo {
            name: "Medium (English)".to_string(),
            filename: "ggml-medium.en.bin".to_string(),
            size_mb: 1500,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin".to_string(),
            description: "Medium model, English only.".to_string(),
        },
        ModelInfo {
            name: "Large-v3".to_string(),
            filename: "ggml-large-v3.bin".to_string(),
            size_mb: 3100,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin".to_string(),
            description: "Best accuracy, slowest. Requires GPU for real-time.".to_string(),
        },
        ModelInfo {
            name: "Large-v3 Turbo".to_string(),
            filename: "ggml-large-v3-turbo.bin".to_string(),
            size_mb: 1600,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin".to_string(),
            description: "Large v3 optimized for speed. Great balance.".to_string(),
        },
    ]
}

pub fn get_models_directory() -> PathBuf {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hyprwhisper")
        .join("models");
    
    // Ensure directory exists
    std::fs::create_dir_all(&data_dir).ok();
    
    data_dir
}

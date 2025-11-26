use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

// Global static for recording management
static RECORDING_FLAG: AtomicBool = AtomicBool::new(false);
static STREAM_READY: AtomicBool = AtomicBool::new(false);
static SAMPLE_RATE: AtomicU32 = AtomicU32::new(16000);
static SAMPLES: once_cell::sync::Lazy<Arc<Mutex<Vec<f32>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

pub struct AudioRecorder {
    sample_rate: u32,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self { sample_rate: 16000 }
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        if RECORDING_FLAG.load(Ordering::SeqCst) {
            return Err("Already recording".to_string());
        }

        // Clear previous samples
        if let Ok(mut samples) = SAMPLES.lock() {
            samples.clear();
        }

        STREAM_READY.store(false, Ordering::SeqCst);
        RECORDING_FLAG.store(true, Ordering::SeqCst);

        // Start recording in a separate thread
        thread::spawn(move || {
            if let Err(e) = start_recording_internal() {
                eprintln!("Recording error: {}", e);
                RECORDING_FLAG.store(false, Ordering::SeqCst);
            }
        });

        // Wait for the stream to be ready
        let start = std::time::Instant::now();
        while !STREAM_READY.load(Ordering::SeqCst) {
            if start.elapsed() > std::time::Duration::from_secs(5) {
                RECORDING_FLAG.store(false, Ordering::SeqCst);
                return Err("Recording failed to start in time".to_string());
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        self.sample_rate = SAMPLE_RATE.load(Ordering::SeqCst);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<Vec<f32>, String> {
        if !RECORDING_FLAG.load(Ordering::SeqCst) {
            return Err("Not recording".to_string());
        }

        RECORDING_FLAG.store(false, Ordering::SeqCst);

        // Give the recording thread time to clean up
        thread::sleep(std::time::Duration::from_millis(100));

        let samples = if let Ok(mut lock) = SAMPLES.lock() {
            std::mem::take(&mut *lock)
        } else {
            Vec::new()
        };

        println!("Recording stopped: {} samples", samples.len());

        // Resample to 16kHz if needed (Whisper requires 16kHz)
        let resampled = if self.sample_rate != 16000 {
            resample(&samples, self.sample_rate, 16000)
        } else {
            samples
        };

        Ok(resampled)
    }

    pub fn is_recording(&self) -> bool {
        RECORDING_FLAG.load(Ordering::SeqCst)
    }

    pub fn get_audio_level(&self) -> f32 {
        if let Ok(lock) = SAMPLES.lock() {
            if lock.is_empty() {
                return 0.0;
            }
            let chunk_size = 1600; // ~100ms at 16kHz
            let start = lock.len().saturating_sub(chunk_size);
            let chunk = &lock[start..];
            if chunk.is_empty() {
                return 0.0;
            }
            let sum_squares: f32 = chunk.iter().map(|s| s * s).sum();
            (sum_squares / chunk.len() as f32).sqrt()
        } else {
            0.0
        }
    }

    /// Get current samples without stopping the recording
    /// Returns samples from the specified position onwards
    pub fn get_samples_from(&self, from_sample: usize) -> Vec<f32> {
        if let Ok(lock) = SAMPLES.lock() {
            if from_sample < lock.len() {
                lock[from_sample..].to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    }

    /// Get total number of samples recorded so far
    pub fn get_sample_count(&self) -> usize {
        if let Ok(lock) = SAMPLES.lock() {
            lock.len()
        } else {
            0
        }
    }

    /// Get all current samples without stopping
    pub fn get_current_samples(&self) -> Vec<f32> {
        if let Ok(lock) = SAMPLES.lock() {
            lock.clone()
        } else {
            Vec::new()
        }
    }
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

fn start_recording_internal() -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input device available")?;

    println!("Using audio device: {:?}", device.name());

    // Get supported config
    let supported_configs = device
        .supported_input_configs()
        .map_err(|e| format!("Failed to get supported configs: {}", e))?;

    // Try to find a config that supports 16kHz
    let target_sample_rate = SampleRate(16000);
    let mut config: Option<StreamConfig> = None;

    for supported_config in supported_configs {
        if supported_config.min_sample_rate() <= target_sample_rate
            && supported_config.max_sample_rate() >= target_sample_rate
        {
            config = Some(StreamConfig {
                channels: 1,
                sample_rate: target_sample_rate,
                buffer_size: cpal::BufferSize::Default,
            });
            break;
        }
    }

    // If no exact match, use default
    let config = config.unwrap_or_else(|| {
        let default = device.default_input_config().unwrap();
        StreamConfig {
            channels: default.channels(),
            sample_rate: default.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        }
    });

    let sample_rate = config.sample_rate.0;
    let channels = config.channels;

    SAMPLE_RATE.store(sample_rate, Ordering::SeqCst);

    let samples_ref = Arc::clone(&SAMPLES);

    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if RECORDING_FLAG.load(Ordering::SeqCst) {
                    if let Ok(mut samples) = samples_ref.lock() {
                        // Convert to mono if stereo
                        if channels > 1 {
                            for chunk in data.chunks(channels as usize) {
                                let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                                samples.push(mono);
                            }
                        } else {
                            samples.extend_from_slice(data);
                        }
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to play stream: {}", e))?;

    println!(
        "Recording started: {}Hz, {} channels",
        sample_rate, channels
    );

    // Signal that we're ready
    STREAM_READY.store(true, Ordering::SeqCst);

    // Keep the stream alive while recording
    while RECORDING_FLAG.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(10));
    }

    // Stream is dropped here when recording stops
    drop(stream);
    println!("Recording stream closed");

    Ok(())
}

// Simple linear resampling
fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = source_rate as f64 / target_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut result = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 * ratio;
        let src_idx_floor = src_idx.floor() as usize;
        let src_idx_ceil = (src_idx_floor + 1).min(samples.len() - 1);
        let frac = src_idx - src_idx_floor as f64;

        let sample =
            samples[src_idx_floor] * (1.0 - frac as f32) + samples[src_idx_ceil] * frac as f32;
        result.push(sample);
    }

    result
}

pub fn get_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

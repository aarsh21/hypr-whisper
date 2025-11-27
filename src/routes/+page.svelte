<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { fly, scale } from "svelte/transition";
  import { backOut, cubicOut } from "svelte/easing";

  let visible = $state(false);
  let recording = $state(false);
  let modelLoaded = $state(false);
  
  // Real-time typing state
  let typedText = "";           // What we've already typed to target
  let previousTranscript = "";  // For delta detection
  let isTyping = false;         // Prevent concurrent wtype calls
  
  let levelInterval: number | null = null;
  let transcribeInterval: number | null = null;
  let unlistenToggle: (() => void) | null = null;
  
  // Smooth audio level for orb animation
  let audioLevel = $state(0);
  let smoothAudioLevel = $state(0);

  function cleanup() {
    if (levelInterval) { clearInterval(levelInterval); levelInterval = null; }
    if (transcribeInterval) { clearInterval(transcribeInterval); transcribeInterval = null; }
  }

  /**
   * Find stable prefix - words that haven't changed between transcriptions
   */
  function findStablePrefix(current: string, previous: string): string {
    if (!previous || !current) return "";
    
    const currentWords = current.trim().split(/\s+/).filter(w => w);
    const previousWords = previous.trim().split(/\s+/).filter(w => w);
    
    const stableWords: string[] = [];
    const minLen = Math.min(currentWords.length, previousWords.length);
    
    for (let i = 0; i < minLen; i++) {
      if (currentWords[i].toLowerCase() === previousWords[i].toLowerCase()) {
        stableWords.push(currentWords[i]);
      } else {
        break;
      }
    }
    
    return stableWords.join(" ");
  }

  /**
   * Real-time transcription with immediate typing to target window
   */
  async function transcriptionLoop() {
    if (!recording || isTyping) return;
    
    try {
      const result = await invoke<string>("transcribe_current");
      if (!result?.trim()) return;
      
      const transcript = result.trim();
      
      // Find stable words that match previous transcription
      const stablePrefix = findStablePrefix(transcript, previousTranscript);
      
      // If we have new stable words beyond what we've typed
      if (stablePrefix.length > typedText.length) {
        const delta = stablePrefix.slice(typedText.length).trim();
        
        if (delta) {
          isTyping = true;
          try {
            // Add space before new words if we have existing text
            const textToType = typedText ? " " + delta : delta;
            await invoke("wtype_text", { text: textToType });
            typedText = stablePrefix;
          } catch (e) {
            console.error("Failed to type:", e);
          } finally {
            isTyping = false;
          }
        }
      }
      
      previousTranscript = transcript;
    } catch (e) {
      // Transcription error, continue
    }
  }

  function start() {
    if (!modelLoaded || recording) return;
    
    // Reset state
    typedText = "";
    previousTranscript = "";
    
    invoke("start_recording").then(() => {
      recording = true;
      
      // Smooth audio level updates
      levelInterval = setInterval(() => {
        invoke<number>("get_audio_level")
          .then(level => {
            audioLevel = Math.min(level * 50, 1);
            smoothAudioLevel = smoothAudioLevel * 0.7 + audioLevel * 0.3;
          })
          .catch(() => {});
      }, 50);

      // Real-time transcription & typing (every 400ms for responsiveness)
      transcribeInterval = setInterval(transcriptionLoop, 400);
    });
  }

  async function finish() {
    if (!recording) return;
    cleanup();
    recording = false;
    visible = false;
    
    // Final transcription to get any remaining words
    try {
      const finalTranscript = await invoke<string>("stop_recording");
      if (finalTranscript?.trim()) {
        // Type any remaining text that wasn't typed yet
        const remaining = finalTranscript.trim();
        if (remaining.length > typedText.length) {
          const delta = remaining.slice(typedText.length).trim();
          if (delta) {
            await new Promise(r => setTimeout(r, 50));
            const textToType = typedText ? " " + delta : delta;
            await invoke("wtype_text", { text: textToType });
          }
        }
      }
    } catch (e) {
      console.error("Final transcription failed:", e);
    }
    
    await invoke("exit_app");
  }

  function cancel() {
    cleanup();
    recording = false;
    visible = false;
    
    // Stop recording but don't type anything
    invoke("stop_recording_silent").then(() => {
      invoke("exit_app");
    });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    } else if ((e.code === "Space" || e.key === "Enter") && recording) {
      e.preventDefault();
      finish();
    }
  }

  onMount(async () => {
    // Listen for toggle-stop event from second instance
    unlistenToggle = await listen("toggle-stop", () => {
      console.log("Received toggle-stop signal");
      finish();
    });
    
    const loaded = await invoke<boolean>("is_model_loaded").then(async isLoaded => {
      if (!isLoaded) {
        const models = await invoke<string[]>("get_downloaded_models");
        if (models.length > 0) {
          await invoke("load_model", { filename: models[0] });
          return true;
        }
        return false;
      }
      return isLoaded;
    });
    
    modelLoaded = loaded;
    visible = true;
    
    if (loaded) {
      setTimeout(start, 300);
    }
  });

  onDestroy(() => {
    cleanup();
    if (unlistenToggle) unlistenToggle();
  });

  // Computed orb scale based on audio
  let orbScale = $derived(1 + smoothAudioLevel * 0.5);
  let glowOpacity = $derived(0.4 + smoothAudioLevel * 0.5);
</script>

<svelte:window onkeydown={onKey} />

<div class="w-screen h-screen flex flex-col items-end justify-end pb-8 pr-8 bg-transparent font-mono">
  {#if visible}
    <!-- Main container - just the orb, no transcript preview -->
    <div
      class="flex flex-col items-center gap-3"
      in:fly={{ y: 30, duration: 400, easing: backOut }}
      out:fly={{ y: 20, duration: 200, easing: cubicOut }}
    >
      <!-- Glassmorphic pill with orb -->
      <div class="dictation-pill">
        {#if !modelLoaded}
          <span class="no-model-text">No model loaded</span>
        {:else}
          <!-- Audio orb -->
          <div class="orb-container">
            <!-- Pulse rings (recording indicator) -->
            {#if recording}
              <div class="pulse-ring"></div>
              <div class="pulse-ring delay-1"></div>
            {/if}
            
            <!-- Glow effect -->
            <div
              class="orb-glow"
              style="opacity: {glowOpacity}; transform: scale({orbScale * 1.3})"
            ></div>
            
            <!-- Main orb -->
            <div
              class="orb"
              style="transform: scale({orbScale})"
            >
              <div class="orb-inner"></div>
            </div>
          </div>

          <!-- Status text -->
          <span class="status-text">
            {#if recording}
              Listening...
            {:else}
              Starting...
            {/if}
          </span>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  /* JetBrains Mono font */
  @import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&display=swap');
  
  :global(*) {
    font-family: 'JetBrains Mono', monospace;
  }

  /* Glassmorphic pill container */
  .dictation-pill {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 20px 10px 14px;
    background: rgba(10, 10, 12, 0.9);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 140, 50, 0.15);
    border-radius: 50px;
    box-shadow:
      0 4px 24px rgba(0, 0, 0, 0.5),
      0 0 40px rgba(255, 100, 0, 0.1),
      inset 0 1px 0 rgba(255, 255, 255, 0.05);
  }

  .no-model-text {
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
    padding: 4px 8px;
  }

  .status-text {
    color: rgba(255, 200, 150, 0.9);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  /* Orb container */
  .orb-container {
    position: relative;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* Pulse rings for recording indicator */
  .pulse-ring {
    position: absolute;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    border: 2px solid rgba(255, 120, 50, 0.5);
    animation: pulse-expand 2s ease-out infinite;
  }

  .pulse-ring.delay-1 {
    animation-delay: 1s;
  }

  @keyframes pulse-expand {
    0% {
      transform: scale(1);
      opacity: 0.6;
    }
    100% {
      transform: scale(2.2);
      opacity: 0;
    }
  }

  /* Orb glow effect */
  .orb-glow {
    position: absolute;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    background: radial-gradient(circle, #ff6a00 0%, #ff4500 50%, #cc3300 100%);
    filter: blur(12px);
    transition: opacity 0.12s ease-out, transform 0.12s ease-out;
  }

  /* Main orb */
  .orb {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    transition: transform 0.08s ease-out;
  }

  .orb-inner {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    background: radial-gradient(circle at 30% 30%, #ff8c42 0%, #ff6b35 40%, #e85d04 70%, #9d4302 100%);
    box-shadow:
      inset 0 0 15px rgba(0, 0, 0, 0.3),
      0 0 8px rgba(255, 100, 50, 0.6);
    animation: orb-pulse 1.5s ease-in-out infinite;
  }

  @keyframes orb-pulse {
    0%, 100% {
      box-shadow:
        inset 0 0 15px rgba(0, 0, 0, 0.3),
        0 0 8px rgba(255, 100, 50, 0.6);
    }
    50% {
      box-shadow:
        inset 0 0 20px rgba(0, 0, 0, 0.2),
        0 0 15px rgba(255, 120, 50, 0.8);
    }
  }

</style>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, onDestroy } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/spinner";
  import Mic from "@lucide/svelte/icons/mic";
  import MicOff from "@lucide/svelte/icons/mic-off";
  import Settings from "@lucide/svelte/icons/settings";
  import X from "@lucide/svelte/icons/x";
  import Minus from "@lucide/svelte/icons/minus";
  import Copy from "@lucide/svelte/icons/copy";
  import ClipboardPaste from "@lucide/svelte/icons/clipboard-paste";
  import Check from "@lucide/svelte/icons/check";
  import SettingsPage from "$lib/components/SettingsPage.svelte";
  import { toast } from "svelte-sonner";

  type AppState = "idle" | "recording" | "processing";

  let appState: AppState = $state("idle");
  let transcript = $state("");
  let showSettings = $state(false);
  let modelLoaded = $state(false);
  let copied = $state(false);
  
  let levelInterval: ReturnType<typeof setInterval> | null = null;
  let audioLevels = $state<number[]>(Array(32).fill(0));
  let unlisteners: UnlistenFn[] = [];

  async function checkModelLoaded() {
    try {
      modelLoaded = await invoke<boolean>("is_model_loaded");
    } catch (e) {
      console.error("Failed to check model status:", e);
    }
  }

  async function startRecording() {
    if (!modelLoaded) {
      toast.error("Please load a model first", {
        description: "Go to Settings to download and select a model."
      });
      showSettings = true;
      return;
    }

    try {
      await invoke("start_recording");
      appState = "recording";
      transcript = "";
      
      // Start monitoring audio levels
      levelInterval = setInterval(async () => {
        try {
          const level = await invoke<number>("get_audio_level");
          
          // Update waveform visualization
          audioLevels = [...audioLevels.slice(1), Math.min(level * 10, 1)];
        } catch {
          // Ignore errors while recording
        }
      }, 50);
    } catch (e) {
      console.error("Failed to start recording:", e);
      toast.error("Failed to start recording", { description: String(e) });
    }
  }

  async function stopRecording() {
    if (appState !== "recording") return;
    
    // Stop level monitoring
    if (levelInterval) {
      clearInterval(levelInterval);
      levelInterval = null;
    }

    appState = "processing";

    try {
      const result = await invoke<string>("stop_recording");
      transcript = result;
      appState = "idle";
      
      // Auto-paste if enabled
      const settings = await invoke<{ auto_paste: boolean }>("get_settings");
      if (settings.auto_paste && result.trim()) {
        await invoke("type_text", { text: result });
        toast.success("Text pasted!");
      }
    } catch (e) {
      console.error("Transcription failed:", e);
      toast.error("Transcription failed", { description: String(e) });
      appState = "idle";
    }
  }

  async function toggleRecording() {
    if (appState === "recording") {
      await stopRecording();
    } else if (appState === "idle") {
      await startRecording();
    }
  }

  async function copyTranscript() {
    if (!transcript) return;
    
    try {
      await invoke("copy_to_clipboard", { text: transcript });
      copied = true;
      setTimeout(() => copied = false, 2000);
      toast.success("Copied to clipboard!");
    } catch (e) {
      toast.error("Failed to copy", { description: String(e) });
    }
  }

  async function pasteTranscript() {
    if (!transcript) return;
    
    try {
      await invoke("type_text", { text: transcript });
      toast.success("Text pasted!");
    } catch (e) {
      toast.error("Failed to paste", { description: String(e) });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (showSettings) {
        showSettings = false;
      } else if (appState === "recording") {
        stopRecording();
      }
    }
    if (e.code === "Space" && !showSettings && appState !== "processing") {
      e.preventDefault();
      toggleRecording();
    }
  }

  onMount(() => {
    // Check if model is loaded
    checkModelLoaded();

    // Listen for hotkey events from backend
    listen("hotkey-pressed", () => {
      toggleRecording();
    }).then(unlisten => unlisteners.push(unlisten));

    // Listen for settings request
    listen("open-settings", () => {
      showSettings = true;
    }).then(unlisten => unlisteners.push(unlisten));

    // Add keyboard listener
    window.addEventListener("keydown", handleKeydown);
  });

  onDestroy(() => {
    if (levelInterval) {
      clearInterval(levelInterval);
    }
    unlisteners.forEach(unlisten => unlisten());
    window.removeEventListener("keydown", handleKeydown);
  });

  async function minimizeWindow() {
    await getCurrentWindow().minimize();
  }

  async function closeWindow() {
    await getCurrentWindow().hide();
  }

  async function startDragging() {
    await getCurrentWindow().startDragging();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="h-screen w-screen select-none overflow-hidden bg-transparent">
  {#if showSettings}
    <SettingsPage 
      onClose={() => { showSettings = false; checkModelLoaded(); }} 
    />
  {:else}
    <!-- Main Recording UI -->
    <div 
      class="flex h-full flex-col rounded-2xl border border-border/50 bg-background/95 backdrop-blur-xl shadow-2xl"
    >
      <!-- Custom Title Bar -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="flex h-10 items-center justify-between px-3 cursor-move"
        onmousedown={startDragging}
      >
        <span class="text-sm font-medium text-foreground/80">HyprWhisper</span>
        <div class="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7 rounded-lg hover:bg-muted"
            onclick={() => showSettings = true}
          >
            <Settings class="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7 rounded-lg hover:bg-muted"
            onclick={minimizeWindow}
          >
            <Minus class="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7 rounded-lg hover:bg-destructive/10 hover:text-destructive"
            onclick={closeWindow}
          >
            <X class="h-4 w-4" />
          </Button>
        </div>
      </div>

      <!-- Content Area -->
      <div class="flex flex-1 flex-col items-center justify-center gap-4 p-6">
        {#if !modelLoaded}
          <!-- No Model Loaded State -->
          <div class="text-center">
            <div class="mb-4 rounded-full bg-muted p-4">
              <MicOff class="h-8 w-8 text-muted-foreground" />
            </div>
            <p class="text-sm text-muted-foreground mb-4">No model loaded</p>
            <Button onclick={() => showSettings = true}>
              <Settings class="mr-2 h-4 w-4" />
              Setup Model
            </Button>
          </div>
        {:else if appState === "processing"}
          <!-- Processing State -->
          <div class="flex flex-col items-center gap-4">
            <div class="relative">
              <Spinner class="h-16 w-16" />
            </div>
            <p class="text-sm text-muted-foreground animate-pulse">
              Transcribing...
            </p>
          </div>
        {:else}
          <!-- Recording Button -->
          <button
            onclick={toggleRecording}
            class="group relative flex h-24 w-24 items-center justify-center rounded-full transition-all duration-300 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2
              {appState === 'recording' 
                ? 'bg-red-500 hover:bg-red-600 scale-110 shadow-lg shadow-red-500/30' 
                : 'bg-primary hover:bg-primary/90 hover:scale-105'}"
          >
            <!-- Pulsing ring for recording state -->
            {#if appState === "recording"}
              <div class="absolute inset-0 animate-ping rounded-full bg-red-500 opacity-30"></div>
              <div class="absolute inset-0 animate-pulse rounded-full bg-red-500 opacity-20"></div>
            {/if}
            
            {#if appState === "recording"}
              <MicOff class="h-10 w-10 text-white relative z-10" />
            {:else}
              <Mic class="h-10 w-10 text-primary-foreground relative z-10 group-hover:scale-110 transition-transform" />
            {/if}
          </button>

          <!-- Audio Waveform Visualization -->
          {#if appState === "recording"}
            <div class="flex h-12 items-center justify-center gap-[2px]">
              {#each audioLevels as level}
                <div
                  class="w-1 rounded-full bg-red-500 transition-all duration-75"
                  style="height: {Math.max(4, level * 48)}px; opacity: {0.3 + level * 0.7}"
                ></div>
              {/each}
            </div>
          {/if}

          <!-- Instructions -->
          <p class="text-center text-sm text-muted-foreground">
            {#if appState === "recording"}
              <span class="text-red-500 font-medium">Recording...</span> Press <kbd class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">Space</kbd> or click to stop
            {:else}
              Press <kbd class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">Space</kbd> or click to record
            {/if}
          </p>

          <!-- Transcript Display -->
          {#if transcript}
            <div class="w-full mt-2">
              <div class="rounded-lg border border-border bg-muted/50 p-3 max-h-24 overflow-y-auto">
                <p class="text-sm text-foreground whitespace-pre-wrap">{transcript}</p>
              </div>
              <div class="flex justify-end gap-2 mt-2">
                <Button variant="outline" size="sm" onclick={copyTranscript}>
                  {#if copied}
                    <Check class="mr-1.5 h-3.5 w-3.5 text-green-500" />
                  {:else}
                    <Copy class="mr-1.5 h-3.5 w-3.5" />
                  {/if}
                  Copy
                </Button>
                <Button variant="default" size="sm" onclick={pasteTranscript}>
                  <ClipboardPaste class="mr-1.5 h-3.5 w-3.5" />
                  Paste
                </Button>
              </div>
            </div>
          {/if}
        {/if}
      </div>

      <!-- Status Bar -->
      <div class="flex h-8 items-center justify-center border-t border-border/50 px-3">
        <div class="flex items-center gap-2">
          <div class="h-2 w-2 rounded-full {modelLoaded ? 'bg-green-500' : 'bg-amber-500'}"></div>
          <span class="text-xs text-muted-foreground">
            {modelLoaded ? 'Ready' : 'No model'} • <kbd class="font-mono">Super+Shift+Space</kbd>
          </span>
        </div>
      </div>
    </div>
  {/if}
</div>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  let recording = $state(false);
  let modelLoaded = $state(false);
  let transcript = $state("");
  
  let levelInterval: number | null = null;
  let transcribeInterval: number | null = null;
  let audioLevels = $state<number[]>(Array(12).fill(0.2));

  function cleanup() {
    if (levelInterval) { clearInterval(levelInterval); levelInterval = null; }
    if (transcribeInterval) { clearInterval(transcribeInterval); transcribeInterval = null; }
  }

  function start() {
    if (!modelLoaded || recording) return;
    
    invoke("start_recording").then(() => {
      recording = true;
      
      levelInterval = setInterval(() => {
        invoke<number>("get_audio_level")
          .then(level => {
            audioLevels = [...audioLevels.slice(1), Math.max(0.2, Math.min(level * 10, 1))];
          })
          .catch(() => {});
      }, 50);

      transcribeInterval = setInterval(() => {
        invoke<string>("transcribe_current")
          .then(result => {
            if (result?.trim()) transcript = result.trim();
          })
          .catch(() => {});
      }, 1000);
    });
  }

  function finish() {
    if (!recording) return;
    cleanup();
    recording = false;
    
    // This handles everything: stop recording, close window, focus prev, paste
    invoke("finish_and_paste", { text: transcript });
  }

  function cancel() {
    cleanup();
    recording = false;
    
    // This handles: stop recording, close window (no paste)
    invoke("cancel_recording");
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

  onMount(() => {
    invoke<boolean>("is_model_loaded").then(async loaded => {
      if (!loaded) {
        const models = await invoke<string[]>("get_downloaded_models");
        if (models.length > 0) {
          await invoke("load_model", { filename: models[0] });
          loaded = true;
        }
      }
      modelLoaded = loaded;
      if (loaded) start();
    });
  });

  onDestroy(cleanup);
</script>

<svelte:window onkeydown={onKey} />

<div class="w-screen h-screen flex flex-col items-center justify-center gap-2">
  <div class="flex items-center justify-center min-w-[120px] h-10 px-5 bg-[#111] border border-[#333] rounded-full">
    {#if !modelLoaded}
      <span class="text-[#666] text-xs">No model</span>
    {:else}
      <div class="flex items-center gap-0.5 h-[18px]">
        {#each audioLevels as level}
          <div 
            class="w-0.5 bg-white rounded-sm"
            style="height:{Math.max(3, level * 18)}px"
          ></div>
        {/each}
      </div>
    {/if}
  </div>
  {#if transcript}
    <div class="max-w-[280px] py-1.5 px-3 bg-[#111] border border-[#333] rounded-[10px] text-[#ddd] text-[11px] text-center">
      {transcript}
    </div>
  {/if}
</div>

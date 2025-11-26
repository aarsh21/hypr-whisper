<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { Progress } from "$lib/components/ui/progress";
  import { Switch } from "$lib/components/ui/switch";
  import { Label } from "$lib/components/ui/label";
  import { Separator } from "$lib/components/ui/separator";
  import { Badge } from "$lib/components/ui/badge";
  import * as Select from "$lib/components/ui/select";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Download from "@lucide/svelte/icons/download";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Check from "@lucide/svelte/icons/check";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import Folder from "@lucide/svelte/icons/folder";
  import Cpu from "@lucide/svelte/icons/cpu";
  import { toast } from "svelte-sonner";

  interface Props {
    onClose: () => void;
  }

  interface ModelInfo {
    name: string;
    filename: string;
    size_mb: number;
    url: string;
    description: string;
  }

  interface Settings {
    model_filename: string;
    language: string;
    hotkey: string;
    auto_paste: boolean;
    show_notification: boolean;
  }

  interface DownloadProgress {
    filename: string;
    progress: number;
    downloaded: number;
    total: number;
  }

  let { onClose }: Props = $props();

  let models = $state<ModelInfo[]>([]);
  let downloadedModels = $state<string[]>([]);
  let settings = $state<Settings>({
    model_filename: "ggml-base.bin",
    language: "auto",
    hotkey: "Super+Shift+Space",
    auto_paste: true,
    show_notification: true,
  });
  let modelsDir = $state("");
  let downloadingFilename = $state<string | null>(null);
  let downloadProgress = $state(0);
  let loadingModel = $state<string | null>(null);
  let unlisteners: UnlistenFn[] = [];

  const languages = [
    { value: "auto", label: "Auto Detect" },
    { value: "en", label: "English" },
    { value: "es", label: "Spanish" },
    { value: "fr", label: "French" },
    { value: "de", label: "German" },
    { value: "it", label: "Italian" },
    { value: "pt", label: "Portuguese" },
    { value: "ru", label: "Russian" },
    { value: "ja", label: "Japanese" },
    { value: "ko", label: "Korean" },
    { value: "zh", label: "Chinese" },
    { value: "ar", label: "Arabic" },
    { value: "hi", label: "Hindi" },
    { value: "nl", label: "Dutch" },
    { value: "pl", label: "Polish" },
    { value: "tr", label: "Turkish" },
    { value: "vi", label: "Vietnamese" },
    { value: "th", label: "Thai" },
    { value: "id", label: "Indonesian" },
    { value: "sv", label: "Swedish" },
    { value: "da", label: "Danish" },
    { value: "no", label: "Norwegian" },
    { value: "fi", label: "Finnish" },
    { value: "uk", label: "Ukrainian" },
    { value: "el", label: "Greek" },
    { value: "he", label: "Hebrew" },
    { value: "cs", label: "Czech" },
    { value: "ro", label: "Romanian" },
    { value: "hu", label: "Hungarian" },
  ];

  async function loadData() {
    try {
      models = await invoke<ModelInfo[]>("get_models");
      downloadedModels = await invoke<string[]>("get_downloaded_models");
      modelsDir = await invoke<string>("get_models_dir");
      settings = await invoke<Settings>("get_settings");
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  async function downloadModel(model: ModelInfo) {
    if (downloadingFilename) return;
    
    downloadingFilename = model.filename;
    downloadProgress = 0;

    try {
      await invoke("download_model", { model });
      downloadedModels = await invoke<string[]>("get_downloaded_models");
      toast.success(`Downloaded ${model.name}`);
    } catch (e) {
      toast.error("Download failed", { description: String(e) });
    } finally {
      downloadingFilename = null;
      downloadProgress = 0;
    }
  }

  async function deleteModel(filename: string) {
    try {
      await invoke("delete_model", { filename });
      downloadedModels = await invoke<string[]>("get_downloaded_models");
      toast.success("Model deleted");
    } catch (e) {
      toast.error("Failed to delete", { description: String(e) });
    }
  }

  async function loadModel(filename: string) {
    loadingModel = filename;
    try {
      await invoke("load_model", { filename });
      settings.model_filename = filename;
      await invoke("save_settings", { settings });
      toast.success("Model loaded");
    } catch (e) {
      toast.error("Failed to load model", { description: String(e) });
    } finally {
      loadingModel = null;
    }
  }

  async function saveSettings() {
    try {
      await invoke("save_settings", { settings });
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  }

  function formatSize(mb: number): string {
    if (mb >= 1000) {
      return `${(mb / 1000).toFixed(1)} GB`;
    }
    return `${mb} MB`;
  }

  onMount(() => {
    loadData();

    // Listen for download progress
    listen<DownloadProgress>("download-progress", (event) => {
      downloadProgress = event.payload.progress;
    }).then(unlisten => unlisteners.push(unlisten));

    listen<string>("download-complete", async () => {
      downloadingFilename = null;
      downloadProgress = 0;
      downloadedModels = await invoke<string[]>("get_downloaded_models");
    }).then(unlisten => unlisteners.push(unlisten));
  });

  onDestroy(() => {
    unlisteners.forEach(unlisten => unlisten());
  });
</script>

<div class="flex h-full flex-col rounded-2xl border border-border/50 bg-background/95 backdrop-blur-xl shadow-2xl">
  <!-- Header -->
  <div class="flex h-12 items-center gap-3 border-b border-border/50 px-4">
    <Button variant="ghost" size="icon" class="h-8 w-8" onclick={onClose}>
      <ArrowLeft class="h-4 w-4" />
    </Button>
    <span class="font-semibold">Settings</span>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto p-4 space-y-6">
    <!-- Models Section -->
    <section>
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-sm font-semibold text-foreground">Whisper Models</h2>
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <Folder class="h-3.5 w-3.5" />
          <span class="truncate max-w-40" title={modelsDir}>{modelsDir}</span>
        </div>
      </div>
      
      <div class="space-y-2">
        {#each models as model}
          {@const isDownloaded = downloadedModels.includes(model.filename)}
          {@const isLoaded = settings.model_filename === model.filename}
          {@const isDownloading = downloadingFilename === model.filename}
          {@const isLoading = loadingModel === model.filename}
          
          <div class="flex items-center gap-3 rounded-lg border border-border/50 bg-card/50 p-3 transition-colors hover:bg-card/80">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="font-medium text-sm">{model.name}</span>
                <Badge variant="secondary" class="text-[10px] px-1.5 py-0">
                  {formatSize(model.size_mb)}
                </Badge>
                {#if isLoaded}
                  <Badge variant="default" class="text-[10px] px-1.5 py-0 bg-green-500/20 text-green-500 border-green-500/30">
                    Active
                  </Badge>
                {/if}
              </div>
              <p class="text-xs text-muted-foreground mt-0.5 truncate">
                {model.description}
              </p>
              
              {#if isDownloading}
                <div class="mt-2">
                  <Progress value={downloadProgress} class="h-1.5" />
                  <span class="text-[10px] text-muted-foreground">{downloadProgress}%</span>
                </div>
              {/if}
            </div>
            
            <div class="flex items-center gap-2">
              {#if isDownloaded}
                {#if !isLoaded}
                  <Button 
                    variant="outline" 
                    size="sm" 
                    class="h-8"
                    onclick={() => loadModel(model.filename)}
                    disabled={isLoading}
                  >
                    {#if isLoading}
                      <Loader2 class="h-3.5 w-3.5 animate-spin" />
                    {:else}
                      <Check class="h-3.5 w-3.5 mr-1" />
                      Load
                    {/if}
                  </Button>
                {/if}
                <Button 
                  variant="ghost" 
                  size="icon" 
                  class="h-8 w-8 text-destructive hover:text-destructive hover:bg-destructive/10"
                  onclick={() => deleteModel(model.filename)}
                  disabled={isLoaded}
                >
                  <Trash2 class="h-4 w-4" />
                </Button>
              {:else}
                <Button 
                  variant="default" 
                  size="sm"
                  class="h-8"
                  onclick={() => downloadModel(model)}
                  disabled={isDownloading}
                >
                  {#if isDownloading}
                    <Loader2 class="h-3.5 w-3.5 animate-spin mr-1" />
                    {downloadProgress}%
                  {:else}
                    <Download class="h-3.5 w-3.5 mr-1" />
                    Download
                  {/if}
                </Button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </section>

    <Separator />

    <!-- Language Section -->
    <section>
      <h2 class="text-sm font-semibold text-foreground mb-3">Language</h2>
      <Select.Root 
        type="single" 
        value={settings.language}
        onValueChange={(value) => {
          if (value) {
            settings.language = value;
            saveSettings();
          }
        }}
      >
        <Select.Trigger class="w-full">
          {languages.find(l => l.value === settings.language)?.label || "Select language"}
        </Select.Trigger>
        <Select.Content>
          {#each languages as lang}
            <Select.Item value={lang.value}>{lang.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
      <p class="text-xs text-muted-foreground mt-1.5">
        Select "Auto Detect" to let Whisper identify the spoken language.
      </p>
    </section>

    <Separator />

    <!-- Behavior Section -->
    <section>
      <h2 class="text-sm font-semibold text-foreground mb-3">Behavior</h2>
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <Label for="auto-paste" class="text-sm">Auto Paste</Label>
            <p class="text-xs text-muted-foreground">Automatically paste transcription to focused input</p>
          </div>
          <Switch 
            id="auto-paste" 
            checked={settings.auto_paste}
            onCheckedChange={(checked) => {
              settings.auto_paste = checked;
              saveSettings();
            }}
          />
        </div>
        
        <div class="flex items-center justify-between">
          <div>
            <Label for="notifications" class="text-sm">Notifications</Label>
            <p class="text-xs text-muted-foreground">Show notification on transcription complete</p>
          </div>
          <Switch 
            id="notifications" 
            checked={settings.show_notification}
            onCheckedChange={(checked) => {
              settings.show_notification = checked;
              saveSettings();
            }}
          />
        </div>
      </div>
    </section>

    <Separator />

    <!-- Hotkey Section -->
    <section>
      <h2 class="text-sm font-semibold text-foreground mb-3">Global Hotkey</h2>
      <div class="flex items-center gap-2 rounded-lg border border-border bg-muted/50 px-3 py-2">
        <Cpu class="h-4 w-4 text-muted-foreground" />
        <kbd class="font-mono text-sm">{settings.hotkey}</kbd>
      </div>
      <p class="text-xs text-muted-foreground mt-1.5">
        Press this shortcut anywhere to start/stop recording.
      </p>
    </section>

    <Separator />

    <!-- Build Info -->
    <section>
      <h2 class="text-sm font-semibold text-foreground mb-3">About</h2>
      <div class="rounded-lg border border-border/50 bg-card/30 p-3 space-y-1">
        <div class="flex justify-between text-xs">
          <span class="text-muted-foreground">Version</span>
          <span>0.1.0</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-muted-foreground">Backend</span>
          <span>whisper.cpp</span>
        </div>
        <div class="flex justify-between text-xs">
          <span class="text-muted-foreground">Inference</span>
          <Badge variant="outline" class="text-[10px] px-1.5 py-0">
            CPU
          </Badge>
        </div>
      </div>
      <p class="text-[10px] text-muted-foreground mt-2 text-center">
        Build with --features cuda or --features rocm for GPU acceleration
      </p>
    </section>
  </div>
</div>

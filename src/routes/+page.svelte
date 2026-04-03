<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { save } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";
  import {
    jobStore, isRunning, isIdle, hasResult, activeTab,
    selectedLanguage, selectedModel, performanceMode,
  } from "$lib/jobStore";
  import { startPipeline, cancelJob, checkModel, exportFile, downloadMedia, auditEnvironment, type EnvironmentAudit } from "$lib/invoke";
  import SyncTab from "$lib/SyncTab.svelte";

  // ── State ────────────────────────────────────────────────────────────────────
  let videoPath = $state<string | null>(null);
  let videoName = $state<string>("");
  let isDragging = $state(false);
  let modelAvailable = $state(true);
  
  // New Source State
  let sourceType = $state<"file" | "url">("file");
  let url = $state("");
  let downloadFormat = $state<"wav" | "mp3" | "mp4">("wav");
  let saveLocal = $state(false);
  
  let unlisten: (() => void) | null = null;
  let unlistenDownload: (() => void) | null = null;

  // Environment Audit State
  let auditResult = $state<EnvironmentAudit | null>(null);

  // ── Lifecycle ─────────────────────────────────────────────────────────────────
  onMount(async () => {
    await performAudit();
    
    // Listen for pipeline progress events from Rust
    unlisten = await listen<{ stage: string; percent: number; segment_count: number }>(
      "pipeline-progress",
      (event) => {
        jobStore.setRunning(event.payload.stage, event.payload.percent);
      }
    );

    unlistenDownload = await listen<{ type: string; data: any }>(
      "download-progress",
      (event) => {
        const { percentage, speed, eta } = event.payload.data;
        jobStore.setDownloading(percentage, speed, eta);
      }
    );
  });

  async function performAudit() {
    try {
      auditResult = await auditEnvironment();
    } catch (e) {
      console.error("Audit failed:", e);
    }
  }

  onDestroy(() => {
    unlisten?.();
    unlistenDownload?.();
  });

  // ── File Handling ─────────────────────────────────────────────────────────────
  async function pickFile() {
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Media Files", extensions: ["mp4", "mov", "mkv", "avi", "m4v", "webm", "mp3", "wav", "m4a", "ogg", "flac", "aac"] },
        { name: "Video", extensions: ["mp4", "mov", "mkv", "avi", "m4v", "webm"] },
        { name: "Audio", extensions: ["mp3", "wav", "m4a", "ogg", "flac", "aac"] }
      ],
    });
    if (selected && typeof selected === "string") {
      setVideoPath(selected);
    }
  }

  function setVideoPath(path: string) {
    videoPath = path;
    videoName = path.split("/").pop() ?? path;
    jobStore.reset();
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    isDragging = false;
    const file = e.dataTransfer?.files[0];
    if (file) setVideoPath((file as any).path ?? file.name);
  }

  // ── Pipeline ──────────────────────────────────────────────────────────────────
  async function startTranscription() {
    let currentPath = videoPath;

    // Verify model exists
    const hasModel = await checkModel($selectedModel);
    if (!hasModel) {
      modelAvailable = false;
      return;
    }
    modelAvailable = true;

    jobStore.reset();

    // If source is URL, download first
    if (sourceType === "url") {
      if (!url) return;
      jobStore.setDownloading(0, "Connecting...", "...");
      try {
        const dlResult = await downloadMedia({
          url,
          format: downloadFormat,
          save_local: saveLocal,
        });
        currentPath = dlResult.file_path;
        videoName = dlResult.title;
      } catch (err: any) {
        if (err?.toString().includes("cancel")) {
          jobStore.setCancelled();
        } else {
          jobStore.setFailed(String(err));
        }
        return;
      }
    }

    if (!currentPath) return;

    jobStore.setRunning("Starting…", 0);

    try {
      const result = await startPipeline({
        video_path: currentPath,
        language: $selectedLanguage,
        model: $selectedModel,
        performance_mode: $performanceMode,
      });
      jobStore.setCompleted(result);
      $activeTab = "review";
    } catch (err: any) {
      if (err?.toString().includes("cancel")) {
        jobStore.setCancelled();
      } else {
        jobStore.setFailed(String(err));
      }
    }
  }

  async function cancel() {
    await cancelJob();
    jobStore.setCancelled();
  }

  // ── Export ────────────────────────────────────────────────────────────────────
  async function exportSRT() {
    const path = await save({
      filters: [{ name: "SRT Subtitle", extensions: ["srt"] }],
      defaultPath: videoName.replace(/\.[^.]+$/, "") + ".srt",
    });
    if (path) await exportFile(path, $jobStore.srtContent);
  }

  async function exportTXT() {
    const path = await save({
      filters: [{ name: "Text", extensions: ["txt"] }],
      defaultPath: videoName.replace(/\.[^.]+$/, "") + ".txt",
    });
    if (path) await exportFile(path, $jobStore.txtContent);
  }

  // ── Helpers ───────────────────────────────────────────────────────────────────
  function formatTime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.round((secs % 1) * 1000);
    return `${String(h).padStart(2,"0")}:${String(m).padStart(2,"0")}:${String(s).padStart(2,"0")},${String(ms).padStart(3,"0")}`;
  }

  function cps(seg: { start: number; end: number; text: string }): number {
    const dur = seg.end - seg.start;
    return dur > 0 ? seg.text.length / dur : 0;
  }
</script>

<!-- ── App Shell ─────────────────────────────────────────────────────────────── -->
<div class="app">
  <!-- Environment Requirements Alert -->
  {#if auditResult && (!auditResult.ffmpeg || !auditResult.whisper || !auditResult.ytdlp)}
    <div class="fixed-overlay">
      <div class="audit-modal">
        <div class="audit-header">
          <div class="audit-icon-bg">
            <svg class="audit-svg" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <h2 class="audit-title">System Requirements</h2>
        </div>

        <p class="audit-desc">
          AutoSub requires some external tools to be installed in your system PATH or application directory.
        </p>

        <ul class="audit-list">
          <li class="audit-item {auditResult.ffmpeg ? 'success' : 'error'}">
            <span class="audit-item-label">FFmpeg</span>
            <span class="audit-status">
              {auditResult.ffmpeg ? '✓ Found' : '✗ Missing'}
            </span>
          </li>
          <li class="audit-item {auditResult.whisper ? 'success' : 'error'}">
            <span class="audit-item-label">Whisper-Main</span>
            <span class="audit-status">
              {auditResult.whisper ? '✓ Found' : '✗ Missing'}
            </span>
          </li>
          <li class="audit-item {auditResult.ytdlp ? 'success' : 'error'}">
            <span class="audit-item-label">YT-DLP</span>
            <span class="audit-status">
              {auditResult.ytdlp ? '✓ Found' : '✗ Missing'}
            </span>
          </li>
        </ul>

        <div class="audit-path-box">
          <p class="audit-path-label">Models directory:</p>
          <p class="audit-path-val">{auditResult.models_dir}</p>
        </div>

        <button 
          onclick={performAudit}
          class="audit-btn"
        >
          Retry Check
        </button>
      </div>
    </div>
  {/if}

  <!-- Header -->
  <header class="header">
    <div class="header-inner">
      <div class="logo">
        <span class="logo-icon">🎬</span>
        <span class="logo-text">AutoSub</span>
        <span class="logo-badge">v0.1</span>
      </div>
      <nav class="tabs">
        <button
          class="tab {$activeTab === 'transcribe' ? 'active' : ''}"
          onclick={() => ($activeTab = "transcribe")}
        >
          Transcribe
        </button>
        <button
          class="tab {$activeTab === 'review' ? 'active' : ''} {!$hasResult ? 'disabled' : ''}"
          onclick={() => $hasResult && ($activeTab = "review")}
        >
          Review
          {#if $jobStore.segments.length > 0}
            <span class="badge">{$jobStore.segments.length}</span>
          {/if}
        </button>
        <button
          class="tab {$activeTab === 'sync' ? 'active' : ''} {!$hasResult ? 'disabled' : ''}"
          onclick={() => $hasResult && ($activeTab = "sync")}
        >
          Sync
        </button>
      </nav>
    </div>
  </header>

  <!-- Main Content -->
  <main class="main">

    <!-- ── TAB 1: TRANSCRIBE ──────────────────────────────────────────────── -->
    {#if $activeTab === "transcribe"}
    <div class="transcribe-layout">
    {/if}
    
    {#if $activeTab === "sync"}
      <SyncTab {videoPath} />
    {/if}

      <!-- Left: File + Settings -->
      <div class="panel settings-panel">
        <!-- Source Selection -->
        <div class="source-tabs">
          <button 
            class="source-tab {sourceType === 'file' ? 'active' : ''}" 
            onclick={() => sourceType = 'file'}
          >
            Local File
          </button>
          <button 
            class="source-tab {sourceType === 'url' ? 'active' : ''}" 
            onclick={() => sourceType = 'url'}
          >
            YouTube / URL
          </button>
        </div>

        {#if sourceType === "file"}
          <!-- Drop Zone -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="dropzone {isDragging ? 'dragging' : ''} {videoPath ? 'has-file' : ''}"
            role="button"
            tabindex="0"
            ondragover={(e) => { e.preventDefault(); isDragging = true; }}
            ondragleave={() => isDragging = false}
            ondrop={handleDrop}
            onclick={pickFile}
            onkeypress={(e) => e.key === "Enter" && pickFile()}
          >
            {#if videoPath}
              <div class="file-info">
                <span class="file-icon">🎞️</span>
                <span class="file-name">{videoName}</span>
                <span class="file-change">Click to change</span>
              </div>
            {:else}
              <div class="drop-prompt">
                <span class="drop-icon">⬆️</span>
                <span class="drop-text">Drop video/audio here</span>
                <span class="drop-sub">or click to browse</span>
                <span class="drop-formats">MP4 · MP3 · WAV · MKV · MOV</span>
              </div>
            {/if}
          </div>
        {:else}
          <div class="url-input-container">
            <label class="field-label" for="url-input">Media URL</label>
            <input
              id="url-input"
              type="text"
              class="input"
              placeholder="https://www.youtube.com/watch?v=..."
              bind:value={url}
              onkeypress={(e) => { if (e.key === "Enter" && url.trim() && !$isRunning) startTranscription(); }}
            />
            
            <div class="url-options">
              <div class="option-item">
                <label class="field-label" for="dl-format">Format</label>
                <select id="dl-format" class="select" bind:value={downloadFormat}>
                  <option value="wav">Transcribe Only (Fastest)</option>
                  <option value="mp3">Transcribe & Save MP3</option>
                  <option value="mp4">Transcribe & Save MP4</option>
                </select>
              </div>
              <div class="option-item checkbox-item">
                <label class="checkbox-label">
                  <input type="checkbox" bind:checked={saveLocal} />
                  <span>Save local copy to Downloads</span>
                </label>
              </div>
            </div>
          </div>
        {/if}

        <!-- Settings -->
        <div class="settings-group">
          <h2 class="panel-title" style="margin-top: 1.5rem">Settings</h2>

          <label class="field-label" for="lang-select">Language</label>
          <select id="lang-select" class="select" bind:value={$selectedLanguage}>
            <option value="auto">Auto Detect</option>
            <option value="zh">Chinese Simplified</option>
            <option value="zh-tw">Chinese Traditional</option>
            <option value="en">English</option>
            <option value="ja">Japanese</option>
            <option value="ko">Korean</option>
          </select>

          <label class="field-label" for="model-select">Model</label>
          <select id="model-select" class="select" bind:value={$selectedModel}>
            <option value="base">Base (Fast · ~141MB)</option>
            <option value="small">Small (~465MB)</option>
            <option value="medium">Medium (~1.5GB)</option>
            <option value="large-v2">Large v2 (Highest Accuracy · ~2.9GB)</option>
            <option value="large-v3-turbo">Large v3 Turbo (Fast & Stable · ~1.6GB)</option>
          </select>

          {#if !modelAvailable}
            <div class="alert alert-error">
              ⚠️ Model not found. Download it to <code>~/.autosub/models/</code>
            </div>
          {/if}

          <label class="field-label" for="perf-select">Performance Mode</label>
          <select id="perf-select" class="select" bind:value={$performanceMode}>
            <option value="Balanced">Balanced (8 threads, quiet)</option>
            <option value="MaxSpeed">Max Speed (12 threads, may heat up)</option>
          </select>
        </div>
      </div>

      <!-- Right: Progress & Controls -->
      <div class="panel progress-panel">
        <h2 class="panel-title">Progress</h2>

        <!-- Start / Cancel button -->
        {#if $isRunning}
          <button id="cancel-btn" class="btn btn-danger" onclick={cancel}>
            ⬛ Cancel
          </button>
        {:else}
          <button
            id="start-btn"
            class="btn btn-primary"
            disabled={(sourceType === 'file' && !videoPath) || (sourceType === 'url' && !url.trim()) || $isRunning}
            onclick={startTranscription}
          >
            {sourceType === "file"
              ? (videoPath ? "▶ Start Transcription" : "Select a file first")
              : "▶ Download & Transcribe"}
          </button>
        {/if}

        <!-- Progress bar -->
        <div class="progress-container">
          <div class="progress-header">
            <span class="progress-stage">
              {$jobStore.status === 'downloading' ? 'Downloading Media...' : ($jobStore.stage || "Ready")}
            </span>
            <span class="progress-pct">{Math.round($jobStore.percent)}%</span>
          </div>
          
          {#if $jobStore.status === 'downloading'}
            <div class="download-meta">
              <span class="dl-speed">🚀 {$jobStore.downloadSpeed}</span>
              <span class="dl-eta">⏳ {$jobStore.downloadEta}</span>
            </div>
          {/if}

          <div class="progress-track">
            <div
              class="progress-fill {$jobStore.status === 'completed' ? 'done' : ''}"
              style="width: {$jobStore.percent}%"
            ></div>
          </div>

          <!-- Stage indicators -->
          <div class="stage-dots">
            {#each ["Downloading", "Extracting", "Transcribing", "Done"] as stageName, i}
              {@const isActive = 
                (stageName === "Downloading" && $jobStore.status === "downloading") ||
                (stageName === "Extracting" && $jobStore.stage?.includes("Extracting")) ||
                (stageName === "Transcribing" && $jobStore.stage?.includes("Transcribing")) ||
                (stageName === "Done" && $jobStore.status === "completed")
              }
              <div class="stage-dot {isActive ? 'active' : ''}">
                <div class="dot"></div>
                <span class="dot-label">{stageName}</span>
              </div>
            {/each}
          </div>
        </div>

        <!-- Status messages -->
        {#if $jobStore.status === "completed"}
          <div class="alert alert-success">
            ✅ Done! {$jobStore.segments.length} segments
            {#if $jobStore.fromCache}(from cache){/if}
          </div>
        {:else if $jobStore.status === "failed"}
          <div class="alert alert-error">
            ❌ {$jobStore.error}
          </div>
        {:else if $jobStore.status === "cancelled"}
          <div class="alert alert-warning">⚠️ Transcription cancelled</div>
        {/if}

        <!-- Live preview of recent segments -->
        {#if $isRunning && $jobStore.segments.length > 0}
          <div class="live-preview">
            <h3 class="preview-title">Live Preview</h3>
            <div class="preview-segments">
              {#each $jobStore.segments.slice(-3) as seg}
                <div class="preview-seg">
                  <span class="preview-time">{formatTime(seg.start)}</span>
                  <span class="preview-text">{seg.text}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
    {/if}

    <!-- ── TAB 2: REVIEW & EXPORT ─────────────────────────────────────────── -->
    {#if $activeTab === "review"}
    <div class="review-layout">

      <!-- Toolbar -->
      <div class="review-toolbar">
        <div class="toolbar-info">
          <span class="seg-count">{$jobStore.segments.length} segments</span>
          {#if $jobStore.durationSecs > 0}
            <span class="duration">· {Math.round($jobStore.durationSecs / 60)}m video</span>
          {/if}
          {#if $jobStore.fromCache}
            <span class="cache-badge">⚡ Cached</span>
          {/if}
        </div>
        <div class="toolbar-actions">
          <button id="export-srt-btn" class="btn btn-secondary" onclick={exportSRT}>
            ⬇ Export SRT
          </button>
          <button id="export-txt-btn" class="btn btn-outline" onclick={exportTXT}>
            ⬇ Export TXT
          </button>
        </div>
      </div>

      <!-- Segment table -->
      <div class="table-container">
        <table class="seg-table">
          <thead>
            <tr>
              <th class="col-idx">#</th>
              <th class="col-time">Start</th>
              <th class="col-time">End</th>
              <th class="col-text">Text</th>
              <th class="col-cps">CPS</th>
            </tr>
          </thead>
          <tbody>
            {#each $jobStore.syncedSegments.length > 0 ? $jobStore.syncedSegments : $jobStore.segments as seg, i}
              {@const segCps = cps(seg)}
              <tr class="seg-row {segCps > 20 ? 'cps-warn' : ''}">
                <td class="col-idx">{i + 1}</td>
                <td class="col-time mono">{formatTime(seg.start)}</td>
                <td class="col-time mono">{formatTime(seg.end)}</td>
                <td class="col-text">
                  <textarea
                    class="seg-edit"
                    value={seg.text}
                    rows={seg.text.split("\n").length}
                    oninput={(e) =>
                      jobStore.updateSegment(i, (e.target as HTMLTextAreaElement).value)
                    }
                  ></textarea>
                </td>
                <td class="col-cps {segCps > 20 ? 'cps-high' : segCps > 15 ? 'cps-mid' : ''}">
                  {segCps.toFixed(1)}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <!-- SRT Preview -->
      <div class="srt-preview-panel">
        <h3 class="preview-title">SRT Preview</h3>
        <pre class="srt-preview">{$jobStore.srtContent.slice(0, 2000)}{$jobStore.srtContent.length > 2000 ? "\n…" : ""}</pre>
      </div>
    </div>
    {/if}

  </main>
</div>

<style>
  /* ── Reset & Base ─────────────────────────────────────────────────────────── */
  :global(*) { box-sizing: border-box; margin: 0; padding: 0; }
  :global(body) {
    font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    background: #0e0f14;
    color: #e2e4ef;
    height: 100vh;
    overflow: hidden;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  /* ── Header ──────────────────────────────────────────────────────────────── */
  .header {
    background: linear-gradient(135deg, #13141c 0%, #1a1b26 100%);
    border-bottom: 1px solid #2a2d3e;
    padding: 0 1.5rem;
    flex-shrink: 0;
  }
  .header-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 56px;
  }
  .logo { display: flex; align-items: center; gap: 0.5rem; }
  .logo-icon { font-size: 1.4rem; }
  .logo-text {
    font-size: 1.1rem;
    font-weight: 700;
    background: linear-gradient(135deg, #7c8cf8, #a78bfa);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    letter-spacing: -0.5px;
  }
  .logo-badge {
    font-size: 0.65rem;
    background: #2d3050;
    color: #7c8cf8;
    padding: 0.15rem 0.4rem;
    border-radius: 999px;
    font-weight: 600;
  }

  /* ── Tabs ────────────────────────────────────────────────────────────────── */
  .tabs { display: flex; gap: 0.25rem; }
  .tab {
    padding: 0.45rem 1.1rem;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: #6b7194;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
    position: relative;
  }
  .tab:hover { color: #c4c8e2; background: #1e2030; }
  .tab.active {
    background: #252840;
    color: #a5b4fc;
    font-weight: 600;
  }
  .tab.disabled { opacity: 0.4; cursor: not-allowed; }

  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: #7c8cf8;
    color: #fff;
    border-radius: 999px;
    font-size: 0.65rem;
    padding: 0.05rem 0.4rem;
    margin-left: 0.35rem;
    font-weight: 700;
  }

  /* ── Main ────────────────────────────────────────────────────────────────── */
  .main {
    flex: 1;
    overflow: auto;
    padding: 1.5rem;
  }

  /* ── Panels ──────────────────────────────────────────────────────────────── */
  .transcribe-layout {
    display: grid;
    grid-template-columns: 380px 1fr;
    gap: 1.5rem;
    height: calc(100vh - 56px - 3rem);
  }

  .panel {
    background: #13141c;
    border: 1px solid #2a2d3e;
    border-radius: 12px;
    padding: 1.5rem;
    overflow-y: auto;
  }

  .panel-title {
    font-size: 0.8rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #5b6080;
    margin-bottom: 1rem;
  }

  /* ── Drop Zone ───────────────────────────────────────────────────────────── */
  .dropzone {
    border: 2px dashed #2a2d3e;
    border-radius: 10px;
    padding: 2rem 1rem;
    text-align: center;
    cursor: pointer;
    transition: all 0.25s;
    background: #0e0f14;
  }
  .dropzone:hover { border-color: #7c8cf8; background: #13152a; }
  .dropzone.dragging { border-color: #7c8cf8; background: #13152a; transform: scale(1.01); }
  .dropzone.has-file { border-color: #4ade80; border-style: solid; background: #0f1a14; }

  .drop-prompt { display: flex; flex-direction: column; gap: 0.25rem; }
  .drop-icon { font-size: 2rem; margin-bottom: 0.5rem; }
  .drop-text { font-size: 1rem; font-weight: 600; color: #c4c8e2; }
  .drop-sub { font-size: 0.85rem; color: #5b6080; }
  .drop-formats { font-size: 0.75rem; color: #404360; margin-top: 0.5rem; }

  .file-info { display: flex; flex-direction: column; align-items: center; gap: 0.4rem; }
  .file-icon { font-size: 2rem; }
  .file-name { font-size: 0.9rem; font-weight: 600; color: #4ade80; word-break: break-all; }
  .file-change { font-size: 0.75rem; color: #5b6080; }

  /* ── Source Selection ────────────────────────────────────────────────────── */
  .source-tabs {
    display: flex;
    background: #0e0f14;
    padding: 0.25rem;
    border-radius: 10px;
    margin-bottom: 1.25rem;
    border: 1px solid #2a2d3e;
  }
  .source-tab {
    flex: 1;
    padding: 0.55rem;
    border: none;
    background: transparent;
    color: #6b7194;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    border-radius: 7px;
    transition: all 0.2s;
  }
  .source-tab.active {
    background: #252840;
    color: #a5b4fc;
    box-shadow: 0 2px 8px rgba(0,0,0,0.2);
  }

  .url-input-container {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    animation: fadeIn 0.3s ease;
  }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

  .input {
    width: 100%;
    background: #1a1b28;
    border: 1px solid #2a2d3e;
    color: #c4c8e2;
    border-radius: 8px;
    padding: 0.65rem 0.85rem;
    font-size: 0.9rem;
    outline: none;
    transition: border-color 0.2s;
  }
  .input:focus { border-color: #7c8cf8; }

  .url-options {
    margin-top: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .checkbox-item {
    padding-top: 0.25rem;
  }
  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: 0.85rem;
    color: #c4c8e2;
    cursor: pointer;
    user-select: none;
  }
  .checkbox-label input {
    accent-color: #7c8cf8;
    width: 16px;
    height: 16px;
  }

  /* ── Settings ────────────────────────────────────────────────────────────── */
  .settings-group { display: flex; flex-direction: column; gap: 0.5rem; }
  .field-label {
    font-size: 0.8rem;
    color: #6b7194;
    font-weight: 600;
    margin-top: 1rem;
    margin-bottom: 0.25rem;
    display: block;
  }
  .select {
    width: 100%;
    background: #1a1b28;
    border: 1px solid #2a2d3e;
    color: #c4c8e2;
    border-radius: 8px;
    padding: 0.55rem 0.75rem;
    font-size: 0.9rem;
    cursor: pointer;
    outline: none;
    transition: border-color 0.2s;
  }
  .select:hover, .select:focus { border-color: #7c8cf8; }

  /* ── Buttons ─────────────────────────────────────────────────────────────── */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 0.6rem 1.2rem;
    border-radius: 8px;
    border: none;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }
  .btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-primary {
    width: 100%;
    padding: 0.75rem;
    background: linear-gradient(135deg, #7c8cf8, #a78bfa);
    color: #fff;
    font-size: 1rem;
    margin-bottom: 1.5rem;
  }
  .btn-primary:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 4px 20px rgba(124, 140, 248, 0.4);
  }

  .btn-danger {
    width: 100%;
    padding: 0.75rem;
    background: #3d1515;
    color: #f87171;
    border: 1px solid #7f1d1d;
    font-size: 1rem;
    margin-bottom: 1.5rem;
  }
  .btn-danger:hover { background: #7f1d1d; }

  .btn-secondary {
    background: #252840;
    color: #a5b4fc;
    border: 1px solid #3a3e5c;
  }
  .btn-secondary:hover { background: #2d3254; }

  .btn-outline {
    background: transparent;
    color: #6b7194;
    border: 1px solid #2a2d3e;
  }
  .btn-outline:hover { border-color: #6b7194; color: #c4c8e2; }

  /* ── Progress ────────────────────────────────────────────────────────────── */
  .progress-container { margin-bottom: 1.5rem; }
  .progress-header {
    display: flex;
    justify-content: space-between;
    font-size: 0.85rem;
    color: #8b92b8;
    margin-bottom: 0.5rem;
  }
  .progress-stage { font-weight: 600; }
  .progress-pct { color: #a5b4fc; }

  .download-meta {
    display: flex;
    gap: 1.5rem;
    font-size: 0.8rem;
    color: #8b92b8;
    margin-bottom: 0.8rem;
    background: #1a1b28;
    padding: 0.4rem 0.75rem;
    border-radius: 6px;
    width: fit-content;
  }
  .dl-speed { color: #4ade80; font-weight: 600; }
  .dl-eta { color: #fbbf24; font-weight: 600; }

  .progress-track {
    width: 100%;
    height: 6px;
    background: #1e2030;
    border-radius: 999px;
    overflow: hidden;
    margin-bottom: 1rem;
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #7c8cf8, #a78bfa);
    border-radius: 999px;
    transition: width 0.5s ease;
  }
  .progress-fill.done { background: linear-gradient(90deg, #34d399, #4ade80); }

  .stage-dots {
    display: flex;
    justify-content: space-between;
    padding: 0 0.25rem;
  }
  .stage-dot {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.3rem;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #2a2d3e;
    transition: background 0.3s;
  }
  .stage-dot.active .dot { background: #7c8cf8; }
  .dot-label { font-size: 0.65rem; color: #404360; }

  /* ── Alerts ──────────────────────────────────────────────────────────────── */
  .alert {
    padding: 0.75rem 1rem;
    border-radius: 8px;
    font-size: 0.85rem;
    margin-top: 1rem;
    line-height: 1.5;
  }
  .alert code {
    background: rgba(255,255,255,0.1);
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
    font-size: 0.8rem;
  }
  .alert-success { background: #0f2b1a; border: 1px solid #166534; color: #86efac; }
  .alert-error { background: #2b0f0f; border: 1px solid #7f1d1d; color: #fca5a5; }
  .alert-warning { background: #2b220f; border: 1px solid #78350f; color: #fcd34d; }

  /* ── Live Preview ────────────────────────────────────────────────────────── */
  .live-preview { margin-top: 1.5rem; }
  .preview-title {
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #5b6080;
    margin-bottom: 0.75rem;
  }
  .preview-segments { display: flex; flex-direction: column; gap: 0.5rem; }
  .preview-seg {
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
    padding: 0.5rem 0.75rem;
    background: #1a1b28;
    border-radius: 6px;
    animation: slide-in 0.2s ease;
  }
  @keyframes slide-in {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .preview-time {
    font-size: 0.7rem;
    color: #5b6080;
    font-family: monospace;
    white-space: nowrap;
    padding-top: 0.1rem;
    min-width: 60px;
  }
  .preview-text { font-size: 0.85rem; color: #c4c8e2; line-height: 1.4; }

  /* ── Review Tab ──────────────────────────────────────────────────────────── */
  .review-layout {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: calc(100vh - 56px - 3rem);
    gap: 1rem;
  }

  .review-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: #13141c;
    border: 1px solid #2a2d3e;
    border-radius: 10px;
    padding: 0.75rem 1.25rem;
  }
  .toolbar-info { display: flex; align-items: center; gap: 0.75rem; font-size: 0.9rem; }
  .seg-count { color: #c4c8e2; font-weight: 600; }
  .duration { color: #6b7194; }
  .cache-badge {
    background: #1a2f1a;
    color: #4ade80;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 700;
  }
  .toolbar-actions { display: flex; gap: 0.75rem; }

  .table-container {
    overflow-y: auto;
    background: #13141c;
    border: 1px solid #2a2d3e;
    border-radius: 10px;
  }

  .seg-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
  }
  .seg-table thead {
    position: sticky;
    top: 0;
    background: #1a1b28;
    z-index: 1;
  }
  .seg-table th {
    padding: 0.65rem 1rem;
    text-align: left;
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #5b6080;
    border-bottom: 1px solid #2a2d3e;
  }

  .col-idx { width: 3.5rem; }
  .col-time { width: 8rem; }
  .col-text { width: auto; }
  .col-cps { width: 4rem; text-align: center; }

  .seg-row {
    border-bottom: 1px solid #1e2030;
    transition: background 0.15s;
  }
  .seg-row:hover { background: #17182a; }
  .seg-row.cps-warn { background: #1f1510; }

  .seg-row td { padding: 0.5rem 1rem; vertical-align: top; }
  .mono { font-family: "Menlo", "Monaco", monospace; font-size: 0.8rem; color: #8b92b8; }

  .seg-edit {
    width: 100%;
    background: transparent;
    border: none;
    color: #c4c8e2;
    font-family: inherit;
    font-size: 0.875rem;
    resize: none;
    outline: none;
    line-height: 1.5;
  }
  .seg-edit:focus {
    background: #1e2030;
    border-radius: 4px;
    padding: 0.25rem;
  }

  .cps-high { color: #f87171; font-weight: 700; }
  .cps-mid { color: #fbbf24; }

  .srt-preview-panel {
    background: #13141c;
    border: 1px solid #2a2d3e;
    border-radius: 10px;
    padding: 1rem 1.25rem;
    max-height: 180px;
    overflow-y: auto;
  }
  .srt-preview {
    font-family: "Menlo", "Monaco", monospace;
    font-size: 0.75rem;
    color: #6b7194;
    line-height: 1.5;
    white-space: pre;
  }
  
  /* ── Audit Modal ─────────────────────────────────────────────────────────── */
  .fixed-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(4px);
    padding: 1rem;
  }
  .audit-modal {
    background: #13141c;
    border: 1px solid #3d1515;
    border-radius: 20px;
    padding: 2rem;
    max-w: 420px;
    width: 100%;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  }
  .audit-header {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }
  .audit-icon-bg {
    padding: 0.75rem;
    background: rgba(239, 68, 68, 0.1);
    border-radius: 12px;
  }
  .audit-svg {
    width: 2rem;
    height: 2rem;
    color: #ef4444;
  }
  .audit-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: #ef4444;
  }
  .audit-desc {
    color: #9ca3af;
    margin-bottom: 1.5rem;
    font-style: italic;
    font-size: 0.9rem;
  }
  .audit-list {
    list-style: none;
    margin-bottom: 2rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .audit-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1rem;
    border-radius: 10px;
    border: 1px solid transparent;
    font-size: 0.95rem;
  }
  .audit-item.success {
    background: rgba(16, 185, 129, 0.05);
    border-color: rgba(16, 185, 129, 0.2);
  }
  .audit-item.error {
    background: rgba(239, 68, 68, 0.05);
    border-color: rgba(239, 68, 68, 0.2);
  }
  .audit-item-label {
    font-weight: 600;
  }
  .audit-status {
    font-weight: 700;
  }
  .audit-item.success .audit-status { color: #10b981; }
  .audit-item.error .audit-status { color: #ef4444; }

  .audit-path-box {
    background: rgba(31, 41, 55, 0.5);
    padding: 1rem;
    border-radius: 12px;
    margin-bottom: 2rem;
    font-family: monospace;
    font-size: 0.7rem;
  }
  .audit-path-label {
    color: #6b7280;
    margin-bottom: 0.25rem;
  }
  .audit-path-val {
    color: #d1d5db;
    word-break: break-all;
  }
  .audit-btn {
    width: 100%;
    padding: 1rem;
    background: linear-gradient(135deg, #2563eb, #4f46e5);
    color: white;
    border-radius: 12px;
    font-weight: 700;
    border: none;
    cursor: pointer;
    transition: all 0.2s;
  }
  .audit-btn:hover {
    transform: translateY(-1px);
    box-shadow: 0 10px 15px -3px rgba(37, 99, 235, 0.4);
  }
</style>

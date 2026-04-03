<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import WaveSurfer from "wavesurfer.js";
  import { jobStore } from "$lib/jobStore";
  import { applySubtitleSync } from "$lib/invoke";

  interface Props {
    videoPath: string | null;
  }
  let { videoPath }: Props = $props();

  let waveformContainer = $state<HTMLElement | null>(null);
  let wavesurfer = $state<WaveSurfer | null>(null);
  let currentTime = $state(0);
  let isPlaying = $state(false);

  // Sync Points State
  let pointAIndex = $state<number | null>(null);
  let pointBIndex = $state<number | null>(null);
  let pointAShift = $state(0);
  let pointBShift = $state(0);
  let isApplyingSync = $state(false);

  onMount(async () => {
    if (waveformContainer && videoPath) {
      initWaveSurfer();
    }
  });

  onDestroy(() => {
    wavesurfer?.destroy();
  });

  function initWaveSurfer() {
    if (!waveformContainer || !videoPath) return;

    wavesurfer = WaveSurfer.create({
      container: waveformContainer,
      waveColor: "#4f46e5",
      progressColor: "#818cf8",
      cursorColor: "#a5b4fc",
      barWidth: 2,
      barGap: 1,
      height: 120,
      autoCenter: true,
      normalize: true,
    });

    // We can load video files directly as audio
    wavesurfer.load(`https://asset.localhost/${videoPath}`);

    wavesurfer.on("timeupdate", (time) => {
      currentTime = time;
    });

    wavesurfer.on("play", () => (isPlaying = true));
    wavesurfer.on("pause", () => (isPlaying = false));
  }

  function togglePlay() {
    wavesurfer?.playPause();
  }

  function seekTo(time: number) {
    wavesurfer?.setTime(time);
  }

  async function applySync() {
    if (pointAIndex === null || pointBIndex === null) return;
    
    isApplyingSync = true;
    try {
      const result = await applySubtitleSync(
        $jobStore.originalSegments,
        pointAIndex,
        pointAShift,
        pointBIndex,
        pointBShift
      );
      jobStore.setSyncedSegments(result);
      alert("Synchronization applied successfully!");
    } catch (e) {
      alert(`Error applying sync: ${e}`);
    } finally {
      isApplyingSync = false;
    }
  }

  function setPointA(index: number) {
    pointAIndex = index;
    seekTo($jobStore.originalSegments[index].start);
  }

  function setPointB(index: number) {
    pointBIndex = index;
    seekTo($jobStore.originalSegments[index].start);
  }

  function formatTime(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.round((secs % 1) * 1000);
    return `${m}:${String(s).padStart(2, "0")}.${String(ms).padStart(3, "0")}`;
  }
</script>

<div class="sync-tab">
  <!-- Waveform Header -->
  <div class="waveform-panel">
    <div bind:this={waveformContainer} class="waveform-container"></div>
    <div class="waveform-controls">
      <button class="play-btn" onclick={togglePlay}>
        {isPlaying ? "⏸ Pause" : "▶ Play"}
      </button>
      <span class="time-display">{formatTime(currentTime)}</span>
    </div>
  </div>

  <div class="sync-grid">
    <!-- Points Control -->
    <div class="panel points-panel">
      <h3 class="panel-subtitle">Point Synchronization</h3>
      
      <div class="points-row">
        <!-- Point A -->
        <div class="point-config {pointAIndex !== null ? 'active' : ''}">
          <div class="point-header">
            <span class="point-label">Point A (Start)</span>
            {#if pointAIndex !== null}
              <span class="point-ref"># {pointAIndex + 1}</span>
            {/if}
          </div>
          
          {#if pointAIndex !== null}
            <div class="point-data">
              <span class="orig-time">Original: {formatTime($jobStore.originalSegments[pointAIndex].start)}s</span>
              <div class="shift-control">
                <label>Shift (sec):</label>
                <div class="shift-inputs">
                  <button onclick={() => pointAShift -= 0.1}>-0.1</button>
                  <input type="number" step="0.1" bind:value={pointAShift} />
                  <button onclick={() => pointAShift += 0.1}>+0.1</button>
                </div>
              </div>
            </div>
          {:else}
            <div class="point-empty">Select a segment below as Point A</div>
          {/if}
        </div>

        <!-- Point B -->
        <div class="point-config {pointBIndex !== null ? 'active' : ''}">
          <div class="point-header">
            <span class="point-label">Point B (End)</span>
            {#if pointBIndex !== null}
              <span class="point-ref"># {pointBIndex + 1}</span>
            {/if}
          </div>
          
          {#if pointBIndex !== null}
            <div class="point-data">
              <span class="orig-time">Original: {formatTime($jobStore.originalSegments[pointBIndex].start)}s</span>
              <div class="shift-control">
                <label>Shift (sec):</label>
                <div class="shift-inputs">
                  <button onclick={() => pointBShift -= 0.1}>-0.1</button>
                  <input type="number" step="0.1" bind:value={pointBShift} />
                  <button onclick={() => pointBShift += 0.1}>+0.1</button>
                </div>
              </div>
            </div>
          {:else}
            <div class="point-empty">Select a segment below as Point B</div>
          {/if}
        </div>
      </div>

      <button 
        class="apply-btn" 
        disabled={pointAIndex === null || pointBIndex === null || isApplyingSync}
        onclick={applySync}
      >
        {isApplyingSync ? "⌛ Processing..." : "⚡ Apply Calibration"}
      </button>

      <div class="sync-tip">
        💡 Tip: For best results, select Point A near the beginning and Point B near the end of the video.
      </div>
    </div>

    <!-- Segment List -->
    <div class="panel segments-panel">
      <h3 class="panel-subtitle">Select Reference Segments</h3>
      <div class="list-container">
        <table class="sync-table">
          <thead>
            <tr>
              <th>#</th>
              <th>Time</th>
              <th>Text</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each $jobStore.originalSegments as seg, i}
              <tr class="sync-row" class:is-a={pointAIndex === i} class:is-b={pointBIndex === i}>
                <td>{i + 1}</td>
                <td class="mono">{formatTime(seg.start)}</td>
                <td class="text-truncate">{seg.text}</td>
                <td class="actions">
                  <button class="btn-set a" onclick={() => setPointA(i)}>Set A</button>
                  <button class="btn-set b" onclick={() => setPointB(i)}>Set B</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </div>
</div>

<style>
  .sync-tab {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    height: calc(100vh - 56px - 3rem);
    animation: fadeIn 0.3s ease;
  }

  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

  .waveform-panel {
    background: #13141c;
    border: 1px solid #2a2d3e;
    border-radius: 12px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .waveform-container {
    background: #0e0f14;
    border-radius: 8px;
    overflow: hidden;
  }

  .waveform-controls {
    display: flex;
    align-items: center;
    gap: 1.5rem;
  }

  .play-btn {
    background: #252840;
    color: #a5b4fc;
    border: 1px solid #3a3e5c;
    padding: 0.4rem 1rem;
    border-radius: 6px;
    font-weight: 600;
    cursor: pointer;
  }
  .play-btn:hover { background: #2d3254; }

  .time-display {
    font-family: monospace;
    font-size: 1.1rem;
    color: #a5b4fc;
    font-weight: 700;
  }

  .sync-grid {
    display: grid;
    grid-template-columns: 380px 1fr;
    gap: 1.5rem;
    flex: 1;
    overflow: hidden;
  }

  .panel {
    background: #13141c;
    border: 1px solid #2a2d3e;
    border-radius: 12px;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
  }

  .panel-subtitle {
    font-size: 0.8rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #5b6080;
    margin-bottom: 1.25rem;
  }

  .points-row {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .point-config {
    background: #1a1b28;
    border: 1px solid #2a2d3e;
    border-radius: 10px;
    padding: 1rem;
    transition: all 0.2s;
  }
  .point-config.active { border-color: #4f46e5; background: #13152a; }
  .point-config.active.is-b { border-color: #ef4444; }

  .point-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 0.75rem;
  }
  .point-label { font-weight: 700; font-size: 0.9rem; color: #8b92b8; }
  .point-ref { background: #4f46e5; color: white; padding: 0.1rem 0.4rem; border-radius: 4px; font-size: 0.7rem; }

  .point-empty { font-size: 0.85rem; color: #4b5563; font-style: italic; }

  .orig-time { display: block; font-size: 0.8rem; color: #6b7280; margin-bottom: 0.75rem; }

  .shift-control {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .shift-control label { font-size: 0.75rem; color: #9ca3af; }

  .shift-inputs {
    display: flex;
    gap: 0.25rem;
  }
  .shift-inputs button {
    background: #252840;
    border: 1px solid #3a3e5c;
    color: white;
    width: 32px;
    height: 32px;
    border-radius: 4px;
    cursor: pointer;
  }
  .shift-inputs input {
    flex: 1;
    background: #0e0f14;
    border: 1px solid #2a2d3e;
    color: white;
    text-align: center;
    border-radius: 4px;
    outline: none;
  }

  .apply-btn {
    background: linear-gradient(135deg, #4f46e5, #7c3aed);
    color: white;
    border: none;
    padding: 0.75rem;
    border-radius: 8px;
    font-weight: 700;
    cursor: pointer;
    transition: transform 0.2s;
  }
  .apply-btn:hover:not(:disabled) { transform: translateY(-1px); }
  .apply-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .sync-tip {
    margin-top: 1rem;
    font-size: 0.75rem;
    color: #6b7280;
    line-height: 1.4;
  }

  .list-container {
    flex: 1;
    overflow-y: auto;
    border-radius: 8px;
    border: 1px solid #2a2d3e;
  }

  .sync-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
  }
  .sync-table th {
    text-align: left;
    padding: 0.6rem 0.75rem;
    background: #1a1b28;
    color: #5b6080;
    font-size: 0.7rem;
    text-transform: uppercase;
    position: sticky;
    top: 0;
  }
  .sync-row { border-bottom: 1px solid #1e2030; }
  .sync-row:hover { background: #17182a; }
  .sync-row.is-a { background: #1e1b4b; }
  .sync-row.is-b { background: #450a0a22; border-left: 3px solid #ef4444; }
  .sync-row.is-a { border-left: 3px solid #4f46e5; }

  .sync-row td { padding: 0.5rem 0.75rem; }
  .mono { font-family: monospace; color: #8b92b8; }
  .text-truncate {
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #c4c8e2;
  }

  .actions { display: flex; gap: 0.4rem; }
  .btn-set {
    padding: 0.2rem 0.5rem;
    border-radius: 4px;
    font-size: 0.7rem;
    font-weight: 700;
    cursor: pointer;
    border: 1px solid transparent;
  }
  .btn-set.a { background: #1e1b4b; color: #a5b4fc; border-color: #4f46e5; }
  .btn-set.b { background: #450a0a; color: #fca5a5; border-color: #991b1b; }
</style>

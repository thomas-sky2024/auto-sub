<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import WaveSurfer from "wavesurfer.js";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { jobStore } from "./jobStore";
  import { applySubtitleSync } from "./invoke";
  
  // Props
  let { videoPath } = $props();
  // Use segments from jobStore reactively
  let segments = $derived($jobStore.segments);

  let waveformContainer: HTMLDivElement | undefined = $state(); // Bind with DOM
  let wavesurfer: WaveSurfer | undefined;
  let isReady = $state(false);
  let errorMessage = $state("");
  let currentTime = $state(0);

  let pointA = $state<{ idx: number | null, shift: number }>({ idx: null, shift: 0 });
  let pointB = $state<{ idx: number | null, shift: number }>({ idx: null, shift: 0 });

  function formatTime(secs: number): string {
    if (isNaN(secs)) return "00:00:00,000";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.round((secs % 1) * 1000);
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")},${String(ms).padStart(3, "0")}`;
  }

  function getSegmentText(idx: number | null): string {
    if (idx === null || !segments[idx]) return "";
    return segments[idx].text;
  }

  function getSegmentStart(idx: number | null): number {
    if (idx === null || !segments[idx]) return 0;
    return segments[idx].start;
  }

  function findCurrentSegmentIdx() {
    return segments.findIndex(s => currentTime >= s.start && currentTime <= s.end);
  }

  function setPointAFromCurrent() {
    const idx = findCurrentSegmentIdx();
    if (idx === -1) {
      // If not exactly on a segment, find the closest one?
      // For now, just return if not on a segment to avoid confusion
      return;
    }
    
    pointA.idx = idx;
    pointA.shift = Math.round((currentTime - segments[idx].start) * 1000);
  }

  function setPointBFromCurrent() {
    const idx = findCurrentSegmentIdx();
    if (idx === -1 || idx === pointA.idx) return;
    
    pointB.idx = idx;
    pointB.shift = Math.round((currentTime - segments[idx].start) * 1000);
  }

  function adjustShift(point: 'A' | 'B', amount: number) {
    if (point === 'A' && pointA.idx !== null) {
      pointA.shift += amount;
    } else if (point === 'B' && pointB.idx !== null) {
      pointB.shift += amount;
    }
  }

  function resetPoints() {
    pointA = { idx: null, shift: 0 };
    pointB = { idx: null, shift: 0 };
  }

  async function applySync() {
    if (pointA.idx === null || pointB.idx === null) return;
    
    try {
      const result = await applySubtitleSync(
        $jobStore.segments,
        pointA.idx,
        pointA.shift / 1000,
        pointB.idx,
        pointB.shift / 1000
      );
      jobStore.setSyncedSegments(result);
    } catch (err) {
      console.error("Apply sync failed:", err);
    }
  }

  onMount(async () => {
    errorMessage = "";
    if (!videoPath) {
      errorMessage = "Không tìm thấy đường dẫn video.";
      return;
    }

    try {
      // Guard for Svelte 5 DOM readiness
      await new Promise(resolve => requestAnimationFrame(resolve));

      if (!waveformContainer) {
        errorMessage = "Lỗi container hiển thị sóng âm.";
        return;
      }

      wavesurfer = WaveSurfer.create({
        container: waveformContainer,
        waveColor: "#4f46e5",
        progressColor: "#818cf8",
        cursorColor: "#ffffff",
        barWidth: 2,
        barRadius: 3,
        height: 120,
        normalize: true,
      });

      const assetUrl = convertFileSrc(videoPath);
      console.log("Loading waveform from:", assetUrl);
      
      wavesurfer.load(assetUrl);

      wavesurfer.on("ready", () => {
        isReady = true;
        errorMessage = "";
        console.log("WaveSurfer is ready");
      });

      wavesurfer.on("error", (e) => {
        console.error("WaveSurfer error:", e);
        errorMessage = "Không thể tải sóng âm. Vui lòng kiểm tra định dạng file.";
        isReady = false;
      });

      wavesurfer.on("audioprocess", () => {
        if (wavesurfer) {
          currentTime = wavesurfer.getCurrentTime();
        }
      });

    } catch (err) {
      console.error("Initialization error:", err);
      errorMessage = "Có lỗi xảy ra khi khởi tạo.";
    }
  });

  onDestroy(() => {
    if (wavesurfer) {
      wavesurfer.destroy();
    }
  });
</script>

<div class="sync-container">
  <div class="waveform-panel panel">
    {#if !isReady}
      <div class="waveform-loader">
        {#if errorMessage}
          <p class="error">{errorMessage}</p>
        {:else}
          <div class="spinner"></div>
          <p>Đang tải sóng âm...</p>
        {/if}
      </div>
    {/if}
    
    <div 
      bind:this={waveformContainer} 
      class="waveform-container" 
      class:hidden={!isReady}
    ></div>
  </div>

  <div class="controls-panel panel">
    <div class="panel-header">
      <h2>Canh đồng bộ (Sync)</h2>
      <p class="help-text">Chọn hai điểm (bắt đầu và kết thúc) để kéo giãn hoặc co lại toàn bộ phụ đề.</p>
    </div>

    <div class="sync-point-group">
      <div class="point-header">
        <h3>Bước 1: Chọn điểm đầu</h3>
        <span class="status-badge" class:set={pointA.idx !== null}>
          {pointA.idx !== null ? 'Đã chọn' : 'Chưa chọn'}
        </span>
      </div>
      
      <div class="point-controls">
        <p class="instruction-text">Nghe video, tìm dòng sub đầu tiên, sau đó bấm nút:</p>
        <button class="btn-action set" onclick={setPointAFromCurrent}>
          Sử dụng thời gian hiện tại ({formatTime(currentTime)}) làm Điểm 1
        </button>
        
        {#if pointA.idx !== null}
          <div class="point-details">
            <div class="detail-row">
              <span>Sub đã chọn:</span>
              <strong>#{pointA.idx + 1}: "{getSegmentText(pointA.idx)}"</strong>
            </div>
            
            <div class="detail-row shift-input-row">
              <label for="shiftA">Độ lệch (ms):</label>
              <div class="shift-numeric-control">
                <button onclick={() => adjustShift('A', -100)} class="btn-minus">−</button>
                <input type="number" id="shiftA" bind:value={pointA.shift} step="10" />
                <button onclick={() => adjustShift('A', 100)} class="btn-plus">+</button>
              </div>
            </div>
            <p class="summary">Sub gốc: {formatTime(getSegmentStart(pointA.idx))} → Sub mới: {formatTime(getSegmentStart(pointA.idx) + pointA.shift/1000)}</p>
          </div>
        {/if}
      </div>
    </div>

    <div class="divider"></div>

    <div class="sync-point-group" class:disabled={pointA.idx === null}>
      <div class="point-header">
        <h3>Bước 2: Chọn điểm cuối</h3>
        <span class="status-badge" class:set={pointB.idx !== null}>
          {pointB.idx !== null ? 'Đã chọn' : 'Chưa chọn'}
        </span>
      </div>

      <div class="point-controls">
        <p class="instruction-text">Nghe video, tìm dòng sub cuối cùng, sau đó bấm nút:</p>
        <button 
          class="btn-action set" 
          onclick={setPointBFromCurrent}
          disabled={pointA.idx === null}
        >
          Sử dụng thời gian hiện tại ({formatTime(currentTime)}) làm Điểm 2
        </button>

        {#if pointB.idx !== null}
          <div class="point-details">
            <div class="detail-row">
              <span>Sub đã chọn:</span>
              <strong>#{pointB.idx + 1}: "{getSegmentText(pointB.idx)}"</strong>
            </div>
            
            <div class="detail-row shift-input-row">
              <label for="shiftB">Độ lệch (ms):</label>
              <div class="shift-numeric-control">
                <button onclick={() => adjustShift('B', -100)} class="btn-minus">−</button>
                <input type="number" id="shiftB" bind:value={pointB.shift} step="10" />
                <button onclick={() => adjustShift('B', 100)} class="btn-plus">+</button>
              </div>
            </div>
            <p class="summary">Sub gốc: {formatTime(getSegmentStart(pointB.idx))} → Sub mới: {formatTime(getSegmentStart(pointB.idx) + pointB.shift/1000)}</p>
          </div>
        {/if}
      </div>
    </div>

    <div class="final-actions">
      <button 
        class="btn-primary" 
        onclick={applySync} 
        disabled={!isReady || pointA.idx === null || pointB.idx === null}
      >
        Áp dụng Đồng bộ
      </button>
      <button class="btn-secondary" onclick={resetPoints}>Xóa các điểm</button>
    </div>
  </div>
</div>

<style>
  /* Container tổng thể */
  .sync-container {
    display: grid;
    grid-template-columns: 1fr 380px; /* Panel phải hẹp hơn */
    gap: 1rem;
    padding: 1rem;
    height: 100%;
    color: #e5e7eb;
    font-family: system-ui, -apple-system, sans-serif;
  }

  .panel {
    background: #1f2937; /* slate-800 */
    border-radius: 0.75rem;
    border: 1px solid #374151;
    display: flex;
    flex-direction: column;
  }

  /* PANEL TRÁI (WAVEFORM) */
  .waveform-panel {
    justify-content: center;
    overflow: hidden;
  }

  .waveform-container {
    width: 100%;
    height: 120px;
    padding: 0 10px;
  }

  .waveform-container.hidden { display: none; }

  .waveform-loader {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    color: #9ca3af;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid #374151;
    border-top-color: #4f46e5;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .error { color: #f87171; }

  @keyframes spin { to { transform: rotate(360deg); } }

  /* PANEL PHẢI (CONTROLS) */
  .controls-panel {
    padding: 1.25rem;
    gap: 1rem;
  }

  .panel-header {
    margin-bottom: 0.5rem;
  }

  h2 { font-size: 1.25rem; font-weight: 600; color: #f3f4f6; margin: 0; }
  h3 { font-size: 1rem; font-weight: 600; color: #f3f4f6; margin: 0; }
  
  .help-text, .instruction-text {
    font-size: 0.875rem; color: #9ca3af; margin: 0.25rem 0 0;
  }

  /* SYNC POINT GROUPS */
  .sync-point-group {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .sync-point-group.disabled { opacity: 0.4; pointer-events: none; }

  .point-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .status-badge {
    font-size: 0.75rem; padding: 2px 8px; border-radius: 99px;
    background: #374151; color: #9ca3af;
  }
  .status-badge.set {
    background: #064e3b; color: #a7f3d0;
  }

  .point-controls {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .point-details {
    background: #111827; /* slate-950 */
    padding: 0.75rem;
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border: 1px solid #1f2937;
  }

  .detail-row {
    font-size: 0.875rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .detail-row strong { color: #e5e7eb; max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;}
  
  /* SHIFT NUMERIC CONTROL */
  .shift-input-row { margin-top: 2px; }

  .shift-numeric-control {
    display: flex;
    align-items: center;
    border-radius: 6px;
    background: #1f2937;
    border: 1px solid #374151;
    overflow: hidden;
  }

  .shift-numeric-control button {
    background: none; border: none; color: #9ca3af;
    padding: 0 10px; height: 32px; font-size: 1.25rem; cursor: pointer;
  }
  .shift-numeric-control button:hover { background: #374151; color: #f3f4f6; }

  .shift-numeric-control input {
    background: #111827; border: none; color: #e5e7eb;
    width: 70px; height: 32px; text-align: center;
    border-left: 1px solid #374151; border-right: 1px solid #374151;
  }

  .summary { font-size: 0.75rem; color: #a1a1aa; font-style: italic; margin: 0; }
  .divider { height: 1px; background: #374151; margin: 0.25rem 0; }

  /* BUTTONS */
  button {
    font-size: 0.875rem; font-weight: 500;
    transition: all 150ms ease;
    cursor: pointer;
  }
  button:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-action.set {
    background: #111827; border: 1px dashed #4f46e5; color: #818cf8;
    padding: 0.5rem 1rem; border-radius: 6px; text-align: center;
  }
  .btn-action.set:not(:disabled):hover { background: #1e1b4b; border-color: #818cf8; }

  .final-actions {
    margin-top: auto; /* Đẩy xuống đáy panel */
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding-top: 1rem;
  }

  .btn-primary {
    background: #4f46e5; color: #ffffff; border: none;
    padding: 0.75rem; border-radius: 8px; font-weight: 600;
  }
  .btn-primary:not(:disabled):hover { background: #4338ca; }

  .btn-secondary {
    background: transparent; color: #9ca3af; border: none;
    padding: 0.5rem; text-decoration: underline;
  }
  .btn-secondary:hover { color: #f3f4f6; }
</style>
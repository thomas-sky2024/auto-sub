<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import WaveSurfer from "wavesurfer.js";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { jobStore } from "$lib/jobStore";
  import { applySubtitleSync } from "$lib/invoke";

  interface Props {
    videoPath: string | null;
  }
  let { videoPath }: Props = $props();

  let waveformContainer = $state<HTMLElement | null>(null);
  let wavesurfer = $state<WaveSurfer | null>(null);
  let isReady = $state(false);
  let errorMessage = $state("");
  let currentTime = $state(0);
  let isApplyingSync = $state(false);

  // Lưu trữ 2 điểm, shift tính bằng milliseconds (ms) cho dễ chỉnh trên UI
  let pointA = $state<{ idx: number | null; shift: number }>({ idx: null, shift: 0 });
  let pointB = $state<{ idx: number | null; shift: number }>({ idx: null, shift: 0 });

  onMount(async () => {
    errorMessage = "";
    if (!videoPath) {
      errorMessage = "Không tìm thấy đường dẫn video.";
      return;
    }

    try {
      // Đợi DOM render xong
      await new Promise((resolve) => requestAnimationFrame(resolve));
      if (!waveformContainer) return;

      wavesurfer = WaveSurfer.create({
        container: waveformContainer,
        waveColor: "#4f46e5",
        progressColor: "#818cf8",
        cursorColor: "#ffffff",
        barWidth: 2,
        barRadius: 3,
        height: 120,
        normalize: true,
        backend: "WebAudio",
      });

      // Dùng hàm chuẩn của Tauri v2 để load file local
      const assetUrl = convertFileSrc(videoPath);
      wavesurfer.load(assetUrl);

      wavesurfer.on("ready", () => {
        isReady = true;
      });

      wavesurfer.on("error", () => {
        errorMessage = "Không thể tải sóng âm. Vui lòng kiểm tra định dạng file.";
        isReady = false;
      });

      wavesurfer.on("audioprocess", () => {
        currentTime = wavesurfer?.getCurrentTime() || 0;
      });

      // Cập nhật thời gian khi người dùng click vào sóng âm
      wavesurfer.on("interaction", () => {
        currentTime = wavesurfer?.getCurrentTime() || 0;
      });
    } catch (err) {
      console.error(err);
      errorMessage = "Có lỗi xảy ra khi khởi tạo sóng âm.";
    }
  });

  onDestroy(() => {
    if (wavesurfer) {
      wavesurfer.destroy();
    }
  });

  // ─── LOGIC XỬ LÝ ĐỒNG BỘ (SYNC) ────────────────────────────────────────────────

  // Lấy danh sách sub gốc
  function getSegments() {
    return $jobStore.originalSegments;
  }

  // Tìm vị trí sub đang phát dựa vào currentTime
  function findCurrentSegmentIdx() {
    const segments = getSegments();
    return segments.findIndex((s) => currentTime >= s.start && currentTime <= s.end);
  }

  function setPointAFromCurrent() {
    const idx = findCurrentSegmentIdx();
    if (idx === -1) {
      alert("Thời gian hiện tại không nằm trong đoạn phụ đề nào!");
      return;
    }
    pointA.idx = idx;
    // Độ lệch = (Thời gian thực tế video - Thời gian sub gốc) * 1000 (ms)
    pointA.shift = Math.round((currentTime - getSegments()[idx].start) * 1000);
  }

  function setPointBFromCurrent() {
    const idx = findCurrentSegmentIdx();
    if (idx === -1) {
      alert("Thời gian hiện tại không nằm trong đoạn phụ đề nào!");
      return;
    }
    if (idx === pointA.idx) {
      alert("Điểm 2 phải khác Điểm 1!");
      return;
    }
    pointB.idx = idx;
    pointB.shift = Math.round((currentTime - getSegments()[idx].start) * 1000);
  }

  function adjustShift(point: "A" | "B", amount: number) {
    if (point === "A" && pointA.idx !== null) {
      pointA.shift += amount;
    } else if (point === "B" && pointB.idx !== null) {
      pointB.shift += amount;
    }
  }

  function getSegmentText(idx: number | null): string {
    if (idx === null) return "";
    return getSegments()[idx]?.text || "";
  }

  function getSegmentStart(idx: number | null): number {
    if (idx === null) return 0;
    return getSegments()[idx]?.start || 0;
  }

  function formatTime(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    const ms = Math.round((secs % 1) * 1000);
    return `${m}:${String(s).padStart(2, "0")}.${String(ms).padStart(3, "0")}`;
  }

  function resetPoints() {
    pointA = { idx: null, shift: 0 };
    pointB = { idx: null, shift: 0 };
  }

  async function applySync() {
    if (pointA.idx === null || pointB.idx === null) return;

    isApplyingSync = true;
    try {
      // Backend Rust yêu cầu thời gian shift tính bằng GIÂY (Seconds)
      const result = await applySubtitleSync(
        getSegments(),
        pointA.idx,
        pointA.shift / 1000,
        pointB.idx,
        pointB.shift / 1000
      );
      jobStore.setSyncedSegments(result);
      alert("✅ Đồng bộ phụ đề thành công! Hãy qua tab Review để kiểm tra.");
    } catch (e) {
      alert(`❌ Lỗi khi đồng bộ: ${e}`);
    } finally {
      isApplyingSync = false;
    }
  }
</script>

<div class="sync-container">
  <div class="waveform-panel panel">
    {#if !isReady}
      <div class="waveform-loader">
        {#if errorMessage}
          <p class="error">{errorMessage}</p>
        {:else}
          <div class="spinner"></div>
          <p>Đang xử lý sóng âm...</p>
        {/if}
      </div>
    {/if}

    <div
      bind:this={waveformContainer}
      class="waveform-container"
      style="display: {isReady ? 'block' : 'none'};"
    ></div>
  </div>

  <div class="controls-panel panel">
    <div class="panel-header">
      <h2>Canh đồng bộ (Sync)</h2>
      <p class="help-text">
        Chọn 2 mốc thời gian trên sóng âm khớp với lúc lời nói phát ra để kéo giãn lại toàn bộ phụ đề.
      </p>
    </div>

    <div class="sync-point-group">
      <div class="point-header">
        <h3>Bước 1: Chọn điểm đầu</h3>
        <span class="status-badge" class:set={pointA.idx !== null}>
          {pointA.idx !== null ? "Đã chọn" : "Chưa chọn"}
        </span>
      </div>

      <div class="point-controls">
        <p class="instruction-text">Nghe đoạn đầu video, tìm dòng sub tương ứng rồi bấm:</p>
        <button class="btn-action set" onclick={setPointAFromCurrent}>
          Dùng thời gian hiện tại ({formatTime(currentTime)})
        </button>

        {#if pointA.idx !== null}
          <div class="point-details">
            <div class="detail-row">
              <span>Sub chọn:</span>
              <strong>#{pointA.idx + 1}: "{getSegmentText(pointA.idx)}"</strong>
            </div>

            <div class="detail-row shift-input-row">
              <label for="shiftA">Độ trễ/sớm (ms):</label>
              <div class="shift-numeric-control">
                <button onclick={() => adjustShift("A", -50)} class="btn-minus">−</button>
                <input type="number" id="shiftA" bind:value={pointA.shift} step="10" />
                <button onclick={() => adjustShift("A", 50)} class="btn-plus">+</button>
              </div>
            </div>
            <p class="summary">
              Kết quả: {formatTime(getSegmentStart(pointA.idx))} → {formatTime(getSegmentStart(pointA.idx) + pointA.shift / 1000)}
            </p>
          </div>
        {/if}
      </div>
    </div>

    <div class="divider"></div>

    <div class="sync-point-group" class:disabled={pointA.idx === null}>
      <div class="point-header">
        <h3>Bước 2: Chọn điểm cuối</h3>
        <span class="status-badge" class:set={pointB.idx !== null}>
          {pointB.idx !== null ? "Đã chọn" : "Chưa chọn"}
        </span>
      </div>

      <div class="point-controls">
        <p class="instruction-text">Nghe đoạn cuối video, tìm dòng sub tương ứng rồi bấm:</p>
        <button
          class="btn-action set"
          onclick={setPointBFromCurrent}
          disabled={pointA.idx === null}
        >
          Dùng thời gian hiện tại ({formatTime(currentTime)})
        </button>

        {#if pointB.idx !== null}
          <div class="point-details">
            <div class="detail-row">
              <span>Sub chọn:</span>
              <strong>#{pointB.idx + 1}: "{getSegmentText(pointB.idx)}"</strong>
            </div>

            <div class="detail-row shift-input-row">
              <label for="shiftB">Độ trễ/sớm (ms):</label>
              <div class="shift-numeric-control">
                <button onclick={() => adjustShift("B", -50)} class="btn-minus">−</button>
                <input type="number" id="shiftB" bind:value={pointB.shift} step="10" />
                <button onclick={() => adjustShift("B", 50)} class="btn-plus">+</button>
              </div>
            </div>
            <p class="summary">
              Kết quả: {formatTime(getSegmentStart(pointB.idx))} → {formatTime(getSegmentStart(pointB.idx) + pointB.shift / 1000)}
            </p>
          </div>
        {/if}
      </div>
    </div>

    <div class="final-actions">
      <button
        class="btn-primary"
        onclick={applySync}
        disabled={!isReady || pointA.idx === null || pointB.idx === null || isApplyingSync}
      >
        {isApplyingSync ? "⌛ Đang xử lý..." : "⚡ Áp dụng Đồng bộ"}
      </button>
      <button class="btn-secondary" onclick={resetPoints}>Làm lại từ đầu</button>
    </div>
  </div>
</div>

<style>
  .sync-container {
    display: grid;
    grid-template-columns: 1fr 380px;
    gap: 1.5rem;
    height: 100%;
    color: #e5e7eb;
  }

  .panel {
    background: #13141c;
    border-radius: 12px;
    border: 1px solid #2a2d3e;
    display: flex;
    flex-direction: column;
  }

  /* WAVEFORM PANEL */
  .waveform-panel {
    justify-content: center;
    align-items: center;
    overflow: hidden;
    padding: 1.5rem;
  }

  .waveform-container {
    width: 100%;
  }

  .waveform-loader {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    color: #8b92b8;
  }

  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid #1e2030;
    border-top-color: #7c8cf8;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .error { color: #f87171; text-align: center; }

  @keyframes spin { to { transform: rotate(360deg); } }

  /* CONTROLS PANEL */
  .controls-panel {
    padding: 1.5rem;
    gap: 1.25rem;
  }

  .panel-header h2 { font-size: 1.1rem; color: #c4c8e2; margin-bottom: 0.25rem; }
  .help-text { font-size: 0.8rem; color: #6b7194; line-height: 1.4; }

  .sync-point-group { display: flex; flex-direction: column; gap: 0.8rem; }
  .sync-point-group.disabled { opacity: 0.4; pointer-events: none; }

  .point-header { display: flex; justify-content: space-between; align-items: center; }
  .point-header h3 { font-size: 0.9rem; color: #a5b4fc; }
  
  .status-badge {
    font-size: 0.7rem; padding: 2px 8px; border-radius: 4px;
    background: #1e2030; color: #6b7194;
  }
  .status-badge.set { background: #0f2b1a; color: #4ade80; border: 1px solid #166534; }

  .point-controls { display: flex; flex-direction: column; gap: 0.6rem; }
  .instruction-text { font-size: 0.8rem; color: #8b92b8; }

  .point-details {
    background: #0e0f14; padding: 0.8rem; border-radius: 8px;
    border: 1px solid #1e2030; display: flex; flex-direction: column; gap: 0.6rem;
  }

  .detail-row { font-size: 0.85rem; display: flex; justify-content: space-between; align-items: center; color: #8b92b8; }
  .detail-row strong { color: #e2e4ef; max-width: 65%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;}
  
  .shift-numeric-control {
    display: flex; align-items: center; border-radius: 6px;
    background: #1a1b28; border: 1px solid #2a2d3e;
  }

  .shift-numeric-control button {
    background: none; border: none; color: #a5b4fc;
    width: 32px; height: 30px; font-size: 1.2rem; cursor: pointer;
  }
  .shift-numeric-control button:hover { background: #252840; color: #fff; }

  .shift-numeric-control input {
    background: #0e0f14; border: none; color: #e2e4ef;
    width: 60px; height: 30px; text-align: center; font-family: monospace;
    border-left: 1px solid #2a2d3e; border-right: 1px solid #2a2d3e;
  }

  .summary { font-size: 0.75rem; color: #6b7194; margin-top: 0.2rem; font-family: monospace; }
  .divider { height: 1px; background: #2a2d3e; margin: 0.5rem 0; }

  /* BUTTONS */
  button { font-family: inherit; transition: all 0.2s; cursor: pointer; border: none; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-action.set {
    background: #1e1b4b; border: 1px dashed #4f46e5; color: #a5b4fc;
    padding: 0.6rem; border-radius: 6px; font-size: 0.85rem; font-weight: 600;
  }
  .btn-action.set:not(:disabled):hover { background: #2e2a6e; border-color: #7c8cf8; color: #fff;}

  .final-actions { margin-top: auto; display: flex; flex-direction: column; gap: 0.5rem; }

  .btn-primary {
    background: linear-gradient(135deg, #4f46e5, #7c3aed); color: white;
    padding: 0.8rem; border-radius: 8px; font-weight: 700; font-size: 0.95rem;
  }
  .btn-primary:not(:disabled):hover { transform: translateY(-1px); box-shadow: 0 4px 12px rgba(79, 70, 229, 0.4); }

  .btn-secondary {
    background: transparent; color: #6b7194; padding: 0.5rem; font-size: 0.85rem;
  }
  .btn-secondary:hover { color: #e2e4ef; text-decoration: underline; }
</style>
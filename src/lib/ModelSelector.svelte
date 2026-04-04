<script lang="ts">
  import { onMount } from "svelte";
  import { listAllModels, type ModelInfo } from "$lib/invoke";
  import { selectedModelId } from "$lib/jobStore";

  let models = $state<ModelInfo[]>([]);
  let loading = $state(true);

  const TIER_LABELS: Record<number, string> = {
    1: "⚡ Nhanh",
    2: "⚖️ Cân bằng",
    3: "💪 Mạnh",
    4: "🏆 Chính xác nhất",
  };

  const LANG_LABELS: Record<string, string> = {
    zh: "🇨🇳 Trung",
    yue: "🇭🇰 Quảng",
    en: "🇺🇸 Anh",
    ja: "🇯🇵 Nhật",
    ko: "🇰🇷 Hàn",
    dialect: "🗣️ Phương ngữ",
  };

  onMount(async () => {
    try {
      models = await listAllModels();
      // Auto-select first downloaded model if current selection not available
      if (!models.find((m) => m.id === $selectedModelId && m.is_downloaded)) {
        const first = models.find((m) => m.is_downloaded);
        if (first) selectedModelId.set(first.id);
      }
    } catch (e) {
      console.error("Không load được danh sách model:", e);
    } finally {
      loading = false;
    }
  });

  function selectModel(id: string, downloaded: boolean) {
    if (!downloaded) return; // Không cho chọn model chưa tải
    selectedModelId.set(id);
  }
</script>

<div class="model-selector">
  <label class="section-label">Model AI</label>

  {#if loading}
    <div class="loading-row">
      <div class="spinner-sm"></div>
      <span>Đang tải danh sách model...</span>
    </div>
  {:else if models.length === 0}
    <div class="no-models">
      ⚠️ Chưa có model nào. Chạy <code>setup-models.sh --vad --sense-voice-2024</code>
    </div>
  {:else}
    <div class="model-list">
      {#each models as model}
        {@const isSelected = $selectedModelId === model.id}
        {@const canSelect = model.is_downloaded}

        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="model-card"
          class:selected={isSelected}
          class:not-downloaded={!canSelect}
          onclick={() => selectModel(model.id, canSelect)}
          title={canSelect ? "Chọn model này" : "Chưa tải — chạy setup-models.sh"}
        >
          <!-- Left: radio + info -->
          <div class="card-left">
            <div class="radio-dot" class:active={isSelected}></div>
            <div class="model-info">
              <div class="model-name">{model.display_name}</div>
              <div class="model-desc">{model.description}</div>
              <div class="model-langs">
                {#each model.languages as lang}
                  <span class="lang-badge">{LANG_LABELS[lang] ?? lang}</span>
                {/each}
              </div>
            </div>
          </div>

          <!-- Right: tier + status -->
          <div class="card-right">
            <span class="tier-badge tier-{model.tier}">
              {TIER_LABELS[model.tier] ?? ""}
            </span>
            <span class="status-badge" class:ready={model.is_downloaded}>
              {model.is_downloaded ? "✓ Sẵn sàng" : `~${model.size_mb}MB`}
            </span>
          </div>
        </div>
      {/each}
    </div>

    <!-- Hướng dẫn tải model chưa có -->
    {#if models.some((m) => !m.is_downloaded)}
      <div class="download-hint">
        💡 Để tải model, chạy trong Terminal:
        <div class="code-block">
          <code>chmod +x build-scripts/setup-models.sh</code>
          <br />
          {#each models.filter((m) => !m.is_downloaded) as m}
            <code># {m.display_name} ({m.size_mb}MB)</code>
            <br />
            <code>./build-scripts/setup-models.sh --vad --{m.id}</code>
            <br />
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .model-selector {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .section-label {
    font-size: 0.8rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #5b6080;
    margin-top: 1rem;
    display: block;
  }

  .loading-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: #6b7194;
    font-size: 0.85rem;
    padding: 0.5rem 0;
  }

  .spinner-sm {
    width: 14px;
    height: 14px;
    border: 2px solid #2a2d3e;
    border-top-color: #7c8cf8;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .no-models {
    font-size: 0.8rem;
    color: #fbbf24;
    background: #2b220f;
    border: 1px solid #78350f;
    border-radius: 8px;
    padding: 0.75rem;
  }

  .no-models code {
    font-family: monospace;
    background: rgba(255,255,255,0.1);
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    font-size: 0.75rem;
  }

  /* ── Model Cards ────────────────────────────────────────────────────── */
  .model-list {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .model-card {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.65rem 0.85rem;
    border-radius: 8px;
    border: 1px solid #2a2d3e;
    background: #0e0f14;
    cursor: pointer;
    transition: all 0.2s;
    gap: 0.75rem;
  }

  .model-card:hover:not(.not-downloaded) {
    border-color: #4f46e5;
    background: #12132a;
  }

  .model-card.selected {
    border-color: #7c8cf8;
    background: #13152a;
    box-shadow: 0 0 0 1px #7c8cf826 inset;
  }

  .model-card.not-downloaded {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .card-left {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
    flex: 1;
    min-width: 0;
  }

  .radio-dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid #3a3e5c;
    flex-shrink: 0;
    margin-top: 0.15rem;
    transition: all 0.2s;
  }

  .radio-dot.active {
    border-color: #7c8cf8;
    background: #7c8cf8;
    box-shadow: 0 0 0 3px #7c8cf820;
  }

  .model-info {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }

  .model-name {
    font-size: 0.875rem;
    font-weight: 600;
    color: #c4c8e2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .model-desc {
    font-size: 0.75rem;
    color: #6b7194;
    line-height: 1.3;
  }

  .model-langs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    margin-top: 0.15rem;
  }

  .lang-badge {
    font-size: 0.65rem;
    background: #1e2030;
    color: #8b92b8;
    padding: 0.1rem 0.35rem;
    border-radius: 4px;
    border: 1px solid #2a2d3e;
  }

  .card-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.3rem;
    flex-shrink: 0;
  }

  .tier-badge {
    font-size: 0.7rem;
    font-weight: 700;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    white-space: nowrap;
  }

  .tier-1 { background: #0f2020; color: #34d399; border: 1px solid #065f46; }
  .tier-2 { background: #1e1b0a; color: #fbbf24; border: 1px solid #78350f; }
  .tier-3 { background: #1a0f2e; color: #a78bfa; border: 1px solid #5b21b6; }
  .tier-4 { background: #2d0f0f; color: #f87171; border: 1px solid #991b1b; }

  .status-badge {
    font-size: 0.7rem;
    padding: 0.1rem 0.5rem;
    border-radius: 4px;
    background: #1e2030;
    color: #6b7194;
    border: 1px solid #2a2d3e;
    white-space: nowrap;
  }

  .status-badge.ready {
    background: #0f2b1a;
    color: #4ade80;
    border-color: #166534;
  }

  /* ── Download Hint ──────────────────────────────────────────────────── */
  .download-hint {
    font-size: 0.78rem;
    color: #6b7194;
    background: #0e0f14;
    border: 1px dashed #2a2d3e;
    border-radius: 8px;
    padding: 0.75rem;
    margin-top: 0.25rem;
  }

  .code-block {
    margin-top: 0.4rem;
    background: #13141c;
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
    font-family: "Menlo", "Monaco", monospace;
    font-size: 0.72rem;
    color: #a5b4fc;
    line-height: 1.8;
  }

  .code-block code {
    color: #a5b4fc;
    display: block;
  }
</style>

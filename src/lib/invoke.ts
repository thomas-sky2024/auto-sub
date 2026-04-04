import { invoke } from "@tauri-apps/api/core";

// ── Segment & Pipeline ────────────────────────────────────────────────────────

export interface Segment {
  start: number;
  end: number;
  text: string;
}

export interface PipelineOptions {
  video_path: string;
  language: string;
  model_id: string;              // "sense-voice-2024" | "paraformer-zh" | "fire-red-v2" | ...
  performance_mode: "Balanced" | "MaxSpeed";
}

export interface PipelineResult {
  segments: Segment[];
  srt_content: string;
  txt_content: string;
  duration_secs: number;
  from_cache: boolean;
}

// ── Model Info ────────────────────────────────────────────────────────────────

export interface ModelInfo {
  id: string;
  display_name: string;
  description: string;
  languages: string[];
  size_mb: number;
  is_downloaded: boolean;
  tier: 1 | 2 | 3 | 4; // 1=Fast, 2=Balanced, 3=Strong, 4=Best
}

// ── Job State ─────────────────────────────────────────────────────────────────

export type JobState =
  | { type: "idle" }
  | { type: "running"; stage: string; percent: number }
  | { type: "completed" }
  | { type: "failed"; error: string }
  | { type: "cancelled" };

// ── Download ──────────────────────────────────────────────────────────────────

export interface DownloadOptions {
  url: string;
  format: "wav" | "mp3" | "mp4";
  save_local: boolean;
  output_dir?: string;
}

export interface DownloadResult {
  file_path: string;
  title: string;
}

// ── Environment Audit ─────────────────────────────────────────────────────────

export interface EnvironmentAudit {
  ffmpeg: boolean;
  sherpa_onnx: boolean;  // đổi từ "whisper"
  ytdlp: boolean;
  vad_ready: boolean;    // thêm mới
  models_dir: string;
  ready_models: string[]; // thêm mới
}

// ── Invoke Functions ──────────────────────────────────────────────────────────

export const startPipeline = (opts: PipelineOptions): Promise<PipelineResult> =>
  invoke("start_pipeline", { opts });

export const downloadMedia = (opts: DownloadOptions): Promise<DownloadResult> =>
  invoke("download_media", { opts });

export const cancelJob = (): Promise<void> =>
  invoke("cancel_job");

export const getJobState = (): Promise<JobState> =>
  invoke("get_job_state");

// Model management
export const checkModel = (modelId: string): Promise<boolean> =>
  invoke("check_model", { modelId });

export const listAllModels = (): Promise<ModelInfo[]> =>
  invoke("list_all_models");

export const listReadyModels = (): Promise<string[]> =>
  invoke("list_ready_models");

export const exportFile = (path: string, content: string): Promise<void> =>
  invoke("export_file", { path, content });

export const auditEnvironment = (): Promise<EnvironmentAudit> =>
  invoke("audit_environment");

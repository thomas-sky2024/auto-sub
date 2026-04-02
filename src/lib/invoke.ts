import { invoke } from "@tauri-apps/api/core";

export interface Segment {
  start: number;
  end: number;
  text: string;
}

export interface PipelineOptions {
  video_path: string;
  language: string;
  model: string;
  performance_mode: "Balanced" | "MaxSpeed";
}

export interface PipelineResult {
  segments: Segment[];
  srt_content: string;
  txt_content: string;
  duration_secs: number;
  from_cache: boolean;
}

export type JobState =
  | { type: "idle" }
  | { type: "running"; stage: string; percent: number }
  | { type: "completed" }
  | { type: "failed"; error: string }
  | { type: "cancelled" };

export const startPipeline = (opts: PipelineOptions): Promise<PipelineResult> =>
  invoke("start_pipeline", { opts });

export const cancelJob = (): Promise<void> =>
  invoke("cancel_job");

export const getJobState = (): Promise<JobState> =>
  invoke("get_job_state");

export const checkModel = (modelName: string): Promise<boolean> =>
  invoke("check_model", { modelName });

export const listModels = (): Promise<string[]> =>
  invoke("list_models");

export const exportFile = (path: string, content: string): Promise<void> =>
  invoke("export_file", { path, content });

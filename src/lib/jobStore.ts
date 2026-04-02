import { writable, derived, get } from "svelte/store";
import type { Segment, PipelineResult } from "./invoke";

// ── Job Status ────────────────────────────────────────────────────────────────
export type JobStatus = "idle" | "running" | "completed" | "failed" | "cancelled";

interface JobStore {
  status: JobStatus;
  stage: string;
  percent: number;
  error: string | null;
  segments: Segment[];
  srtContent: string;
  txtContent: string;
  fromCache: boolean;
  durationSecs: number;
  downloadSpeed: string;
  downloadEta: string;
}

const initialState: JobStore = {
  status: "idle",
  stage: "",
  percent: 0,
  error: null,
  segments: [],
  srtContent: "",
  txtContent: "",
  fromCache: false,
  durationSecs: 0,
  downloadSpeed: "",
  downloadEta: "",
};

function createJobStore() {
  const { subscribe, set, update } = writable<JobStore>(initialState);

  return {
    subscribe,

    setRunning: (stage: string, percent: number) =>
      update((s) => ({ ...s, status: "running", stage, percent, error: null })),

    setDownloading: (percent: number, speed: string, eta: string) =>
      update((s) => ({ ...s, status: "running", stage: "Downloading", percent, downloadSpeed: speed, downloadEta: eta })),

    setCompleted: (result: PipelineResult) =>
      update((s) => ({
        ...s,
        status: "completed",
        stage: "Done",
        percent: 100,
        segments: result.segments,
        srtContent: result.srt_content,
        txtContent: result.txt_content,
        fromCache: result.from_cache,
        durationSecs: result.duration_secs,
      })),

    setFailed: (error: string) =>
      update((s) => ({ ...s, status: "failed", error, stage: "Failed" })),

    setCancelled: () =>
      update((s) => ({ ...s, status: "cancelled", stage: "Cancelled", percent: 0 })),

    reset: () => set(initialState),

    updateSegment: (index: number, text: string) =>
      update((s) => {
        const segments = [...s.segments];
        if (segments[index]) {
          segments[index] = { ...segments[index], text };
        }
        return { ...s, segments };
      }),

    setSegments: (segments: Segment[]) =>
      update((s) => ({ ...s, segments })),
  };
}

export const jobStore = createJobStore();

// Derived helpers
export const isRunning = derived(jobStore, ($j) => $j.status === "running");
export const isIdle = derived(jobStore, ($j) => $j.status === "idle");
export const hasResult = derived(jobStore, ($j) =>
  $j.status === "completed" && $j.segments.length > 0
);
export const segmentCount = derived(jobStore, ($j) => $j.segments.length);

// ── UI Settings ───────────────────────────────────────────────────────────────
export const selectedLanguage = writable<string>("auto");
export const selectedModel = writable<string>("large-v2");
export const performanceMode = writable<"Balanced" | "MaxSpeed">("Balanced");
export const activeTab = writable<"transcribe" | "review">("transcribe");

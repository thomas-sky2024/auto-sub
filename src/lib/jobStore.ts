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
  originalSegments: Segment[];
  syncedSegments: Segment[];
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
  originalSegments: [],
  syncedSegments: [],
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
        originalSegments: [...result.segments],
        syncedSegments: [],
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

    setSyncedSegments: (segments: Segment[]) =>
      update((s) => ({ ...s, syncedSegments: segments, segments: [...segments] })),
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

// Helper function to regenerate SRT content from segments
export function generateSRTContent(segments: Segment[]): string {
  if (segments.length === 0) return "";
  
  return segments
    .map((seg, i) => {
      const formatTime = (secs: number): string => {
        const h = Math.floor(secs / 3600);
        const m = Math.floor((secs % 3600) / 60);
        const s = Math.floor(secs % 60);
        const ms = Math.round((secs % 1) * 1000);
        return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")},${String(ms).padStart(3, "0")}`;
      };
      
      return `${i + 1}\n${formatTime(seg.start)} --> ${formatTime(seg.end)}\n${seg.text}\n`;
    })
    .join("\n");
}

// Helper function to regenerate TXT content from segments
export function generateTXTContent(segments: Segment[]): string {
  return segments.map((seg) => seg.text).join("\n");
}

// ── UI Settings ───────────────────────────────────────────────────────────────
export const selectedLanguage = writable<string>("auto");
export const selectedModel = writable<string>("large-v3-turbo");
export const performanceMode = writable<"Balanced" | "MaxSpeed">("Balanced");
export const activeTab = writable<"transcribe" | "sync" | "review">("transcribe");

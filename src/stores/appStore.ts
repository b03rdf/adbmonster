import { create } from "zustand";

interface AppState {
  statusText: string;
  screenshotPath: string;
  recordOutputDir: string;
  setScreenshotPath: (path: string) => void;
  setRecordOutputDir: (dir: string) => void;
  setStatusText: (text: string) => void;
}

export const useAppStore = create<AppState>((set) => ({
  statusText: "Ready",
  screenshotPath: "",
  recordOutputDir: "",
  setScreenshotPath: (screenshotPath) => set({ screenshotPath }),
  setRecordOutputDir: (recordOutputDir) => set({ recordOutputDir }),
  setStatusText: (text) => set({ statusText: text }),
}));

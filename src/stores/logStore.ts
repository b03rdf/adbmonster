import { create } from "zustand";

interface LogState {
  lines: string[];
  isRunning: boolean;
  isPaused: boolean;
  filterText: string;
  maxLines: number;
  errorCount: number;
  crashCount: number;
  anrCount: number;
  addLines: (lines: string[]) => void;
  setLines: (lines: string[]) => void;
  setIsRunning: (running: boolean) => void;
  setIsPaused: (paused: boolean) => void;
  setFilterText: (text: string) => void;
  clearLogs: () => void;
}

export const useLogStore = create<LogState>((set) => ({
  lines: [],
  isRunning: false,
  isPaused: false,
  filterText: "",
  maxLines: 10000,
  errorCount: 0,
  crashCount: 0,
  anrCount: 0,
  addLines: (lines) =>
    set((state) => {
      if (lines.length === 0) return state;
      const nextLines = [...state.lines, ...lines];
      let errorCount = state.errorCount;
      let crashCount = state.crashCount;
      let anrCount = state.anrCount;
      for (const line of lines) {
        if (/\s[EF]\s+[^:]+:|^[EF]\//.test(line)) errorCount += 1;
        if (line.includes("FATAL EXCEPTION") || line.includes("Fatal signal")) crashCount += 1;
        if (line.includes("ANR in ")) anrCount += 1;
      }
      return {
        lines:
          nextLines.length > state.maxLines
            ? nextLines.slice(-state.maxLines)
            : nextLines,
        errorCount,
        crashCount,
        anrCount,
      };
    }),
  setLines: (lines) => set({ lines }),
  setIsRunning: (isRunning) => set({ isRunning }),
  setIsPaused: (isPaused) => set({ isPaused }),
  setFilterText: (filterText) => set({ filterText }),
  clearLogs: () => set({ lines: [], errorCount: 0, crashCount: 0, anrCount: 0 }),
}));

import { create } from "zustand";

interface LogState {
  lines: string[];
  isRunning: boolean;
  isPaused: boolean;
  filterText: string;
  maxLines: number;
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
  addLines: (lines) =>
    set((state) => {
      if (lines.length === 0) return state;
      const nextLines = [...state.lines, ...lines];
      return {
        lines:
          nextLines.length > state.maxLines
            ? nextLines.slice(-state.maxLines)
            : nextLines,
      };
    }),
  setLines: (lines) => set({ lines }),
  setIsRunning: (isRunning) => set({ isRunning }),
  setIsPaused: (isPaused) => set({ isPaused }),
  setFilterText: (filterText) => set({ filterText }),
  clearLogs: () => set({ lines: [] }),
}));

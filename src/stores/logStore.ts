import { create } from "zustand";

interface LogState {
  lines: string[];
  isRunning: boolean;
  isPaused: boolean;
  filterText: string;
  maxLines: number;
  addLine: (line: string) => void;
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
  addLine: (line) =>
    set((state) => {
      const newLines = [...state.lines, line];
      if (newLines.length > state.maxLines) {
        return { lines: newLines.slice(-state.maxLines) };
      }
      return { lines: newLines };
    }),
  setLines: (lines) => set({ lines }),
  setIsRunning: (isRunning) => set({ isRunning }),
  setIsPaused: (isPaused) => set({ isPaused }),
  setFilterText: (filterText) => set({ filterText }),
  clearLogs: () => set({ lines: [] }),
}));

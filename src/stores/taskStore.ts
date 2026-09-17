import { create } from "zustand";
import { reconcileTasks, type TaskSnapshot } from "@/lib/taskState";

export const useTaskStore = create<{
  snapshot: TaskSnapshot;
  receive: (snapshot: TaskSnapshot) => void;
}>((set) => ({
  snapshot: { revision: -1, tasks: [], shuttingDown: false, storageError: null },
  receive: (incoming) => set((state) => ({ snapshot: reconcileTasks(state.snapshot, incoming) })),
}));

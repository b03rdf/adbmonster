export type TaskKind = "logcat" | "recording" | "recordingSave" | "scrcpy" | "diagnostic" | "weakNetwork";
export type TaskStatus = "starting" | "running" | "cancelling" | "completed" | "cancelled" | "failed" | "interrupted";

export interface TaskRecord {
  id: string;
  kind: TaskKind;
  deviceId: string;
  targetPackage: string | null;
  status: TaskStatus;
  progress: number | null;
  message: string;
  cleanup: string;
  outputPath: string | null;
  startedAt: string;
  endedAt: string | null;
}

export interface TaskSnapshot {
  revision: number;
  tasks: TaskRecord[];
  shuttingDown: boolean;
  storageError: string | null;
}

export function isTaskActive(task: Pick<TaskRecord, "status">) {
  return task.status === "starting" || task.status === "running" || task.status === "cancelling";
}

// Event delivery and polling may complete out of order.
export function reconcileTasks(current: TaskSnapshot, incoming: TaskSnapshot) {
  return incoming.revision > current.revision ? incoming : current;
}

export function visibleTasks(tasks: TaskRecord[], deviceId?: string, activeOnly = false) {
  return tasks.filter((task) => (!deviceId || task.deviceId === deviceId) && (!activeOnly || isTaskActive(task)));
}

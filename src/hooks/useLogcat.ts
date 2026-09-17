import { useEffect, useCallback, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useLogStore } from "@/stores/logStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { useAppStore } from "@/stores/appStore";
import { isLogcatRunning, startLogcat, stopLogcat, cancelTask, listTasks } from "@/lib/tauri";
import { isTaskActive } from "@/lib/taskState";
import { useTaskStore } from "@/stores/taskStore";

export function useLogcat() {
  const { lines, isRunning, isPaused, errorCount, crashCount, anrCount, addLines, setIsRunning, setIsPaused, clearLogs: clearStoreLogs, setFilterText, filterText } =
    useLogStore();
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const setStatusText = useAppStore((s) => s.setStatusText);
  const unlistenRef = useRef<UnlistenFn[]>([]);
  const isPausedRef = useRef(isPaused);
  const pendingLinesRef = useRef<string[]>([]);
  const activeDeviceIdRef = useRef<string | null>(null);
  const activeTaskIdRef = useRef<string | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    isPausedRef.current = isPaused;
  }, [isPaused]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!isPausedRef.current && pendingLinesRef.current.length > 0) {
        const batch = pendingLinesRef.current;
        pendingLinesRef.current = [];
        addLines(batch);
      }
    }, 50);

    return () => window.clearInterval(timer);
  }, [addLines]);

  useEffect(() => {
    let cancelled = false;

    const setup = async () => {
      const listeners = await Promise.all([
        listen<{ taskId: string; deviceId: string; lines: string[] }>("logcat:batch", (event) => {
          if (cancelled || event.payload.deviceId !== useDeviceStore.getState().currentDevice?.id) return;
          if (!activeDeviceIdRef.current) return;
          const task = useTaskStore.getState().snapshot.tasks.find((task) => task.id === event.payload.taskId);
          if (task && !isTaskActive(task)) return;
          if (activeTaskIdRef.current && event.payload.taskId !== activeTaskIdRef.current) return;
          pendingLinesRef.current.push(...event.payload.lines);
          if (pendingLinesRef.current.length > 10000) {
            pendingLinesRef.current = pendingLinesRef.current.slice(-10000);
          }
        }),
        listen<string>("logcat:stopped", () => {
          window.setTimeout(() => {
            if (cancelled) return;
            void isLogcatRunning()
              .then((running) => {
                if (cancelled) return;
                setIsRunning(running);
                if (!running) {
                  activeDeviceIdRef.current = null;
                  activeTaskIdRef.current = null;
                  setIsPaused(false);
                }
              })
              .catch((err) => setStatusText(`Failed to query logcat status: ${err}`));
          }, 100);
        }),
        listen<string>("logcat:error", (event) => {
          setStatusText(`Logcat error: ${event.payload}`);
        }),
      ]);
      if (cancelled) { listeners.forEach((unlisten) => unlisten()); return; }
      unlistenRef.current = listeners;
      const snapshot = await listTasks();
      const task = snapshot.tasks.find((task) => task.kind === "logcat" && isTaskActive(task));
      const running = Boolean(task);
      if (!cancelled) {
        setIsRunning(running);
        if (requestRef.current === 0) {
          activeDeviceIdRef.current = task?.deviceId ?? null;
          activeTaskIdRef.current = task?.id ?? null;
        }
      }
      if (cancelled) {
        listeners.forEach((unlisten) => unlisten());
      } else {
        unlistenRef.current = listeners;
      }
    };

    setup().catch((err) => setStatusText(`Failed to listen for logcat events: ${err}`));

    return () => {
      cancelled = true;
      unlistenRef.current.forEach((unlisten) => unlisten());
      unlistenRef.current = [];
    };
  }, [setIsPaused, setIsRunning, setStatusText]);

  const clearLogs = useCallback(() => {
    pendingLinesRef.current = [];
    clearStoreLogs();
  }, [clearStoreLogs]);

  const start = useCallback(async (buffer = "main") => {
    if (!currentDevice) return;
    const deviceId = currentDevice.id;
    const requestId = ++requestRef.current;
    activeDeviceIdRef.current = deviceId;
    activeTaskIdRef.current = null;
    setIsPaused(false);
    try {
      pendingLinesRef.current = [];
      clearLogs();
      const taskId = await startLogcat(deviceId, buffer);
      if (requestId !== requestRef.current) { await cancelTask(taskId); return; }
      activeTaskIdRef.current = taskId;
      if (useDeviceStore.getState().currentDevice?.id !== deviceId) {
        await cancelTask(taskId);
        activeDeviceIdRef.current = null;
        activeTaskIdRef.current = null;
        setIsRunning(false);
        setStatusText("设备已切换，Logcat 已停止");
        return;
      }
      setIsRunning(true);
    } catch (err) {
      if (requestId !== requestRef.current) return;
      activeDeviceIdRef.current = null;
      activeTaskIdRef.current = null;
      setIsRunning(false);
      console.error("Failed to start logcat:", err);
      setStatusText(`Failed to start logcat: ${err}`);
    }
  }, [currentDevice, setIsPaused, setIsRunning, clearLogs, setStatusText]);

  const stop = useCallback(async () => {
    const requestId = ++requestRef.current;
    try {
      if (activeTaskIdRef.current) await cancelTask(activeTaskIdRef.current);
      else await stopLogcat();
      if (requestId !== requestRef.current) return;
      activeDeviceIdRef.current = null;
      activeTaskIdRef.current = null;
      setIsRunning(false);
      setIsPaused(false);
    } catch (err) {
      if (requestId !== requestRef.current) return;
      console.error("Failed to stop logcat:", err);
      setStatusText(`Failed to stop logcat: ${err}`);
    }
  }, [setIsPaused, setIsRunning, setStatusText]);

  const togglePause = useCallback(() => {
    setIsPaused(!isPaused);
  }, [isPaused, setIsPaused]);

  useEffect(() => {
    if (isRunning && activeDeviceIdRef.current !== currentDevice?.id) {
      void stop();
    }
  }, [currentDevice?.id, isRunning, stop]);

  return {
    lines,
    isRunning,
    isPaused,
    filterText,
    errorCount,
    crashCount,
    anrCount,
    start,
    stop,
    togglePause,
    clearLogs,
    setFilterText,
  };
}

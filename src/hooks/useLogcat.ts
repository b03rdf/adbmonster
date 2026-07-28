import { useEffect, useCallback, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useLogStore } from "@/stores/logStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { useAppStore } from "@/stores/appStore";
import { startLogcat, stopLogcat } from "@/lib/tauri";

export function useLogcat() {
  const { lines, isRunning, isPaused, errorCount, crashCount, anrCount, addLines, setIsRunning, setIsPaused, clearLogs: clearStoreLogs, setFilterText, filterText } =
    useLogStore();
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const setStatusText = useAppStore((s) => s.setStatusText);
  const unlistenRef = useRef<UnlistenFn[]>([]);
  const isPausedRef = useRef(isPaused);
  const pendingLinesRef = useRef<string[]>([]);

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
        listen<string[]>("logcat:batch", (event) => {
          pendingLinesRef.current.push(...event.payload);
          if (pendingLinesRef.current.length > 10000) {
            pendingLinesRef.current = pendingLinesRef.current.slice(-10000);
          }
        }),
        listen<string>("logcat:stopped", () => setIsRunning(false)),
        listen<string>("logcat:error", (event) => {
          setStatusText(`Logcat error: ${event.payload}`);
        }),
      ]);
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
  }, [setIsRunning, setStatusText]);

  const clearLogs = useCallback(() => {
    pendingLinesRef.current = [];
    clearStoreLogs();
  }, [clearStoreLogs]);

  const start = useCallback(async (buffer = "main") => {
    if (!currentDevice) return;
    try {
      pendingLinesRef.current = [];
      clearLogs();
      await startLogcat(currentDevice.id, buffer);
      setIsRunning(true);
    } catch (err) {
      console.error("Failed to start logcat:", err);
      setStatusText(`Failed to start logcat: ${err}`);
    }
  }, [currentDevice, setIsRunning, clearLogs, setStatusText]);

  const stop = useCallback(async () => {
    try {
      await stopLogcat();
      setIsRunning(false);
    } catch (err) {
      console.error("Failed to stop logcat:", err);
      setStatusText(`Failed to stop logcat: ${err}`);
    }
  }, [setIsRunning, setStatusText]);

  const togglePause = useCallback(() => {
    setIsPaused(!isPaused);
  }, [isPaused, setIsPaused]);

  useEffect(() => {
    if (!currentDevice && isRunning) {
      stop();
    }
  }, [currentDevice, isRunning, stop]);

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

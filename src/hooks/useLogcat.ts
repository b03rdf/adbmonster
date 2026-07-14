import { useEffect, useCallback, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useLogStore } from "@/stores/logStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { useAppStore } from "@/stores/appStore";
import { startLogcat, stopLogcat } from "@/lib/tauri";

export function useLogcat() {
  const { lines, isRunning, isPaused, addLines, setIsRunning, setIsPaused, clearLogs, setFilterText, filterText } =
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
      if (pendingLinesRef.current.length > 0) {
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
        listen<string>("logcat:line", (event) => {
          if (!isPausedRef.current) {
            pendingLinesRef.current.push(event.payload);
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

  const start = useCallback(async () => {
    if (!currentDevice) return;
    try {
      pendingLinesRef.current = [];
      clearLogs();
      await startLogcat(currentDevice.id, filterText || undefined);
      setIsRunning(true);
    } catch (err) {
      console.error("Failed to start logcat:", err);
      setStatusText(`Failed to start logcat: ${err}`);
    }
  }, [currentDevice, filterText, setIsRunning, clearLogs, setStatusText]);

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
    start,
    stop,
    togglePause,
    clearLogs,
    setFilterText,
  };
}

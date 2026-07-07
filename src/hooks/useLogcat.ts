import { useEffect, useCallback, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useLogStore } from "@/stores/logStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { startLogcat, stopLogcat } from "@/lib/tauri";

export function useLogcat() {
  const { lines, isRunning, isPaused, addLine, setIsRunning, setIsPaused, clearLogs, setFilterText, filterText } =
    useLogStore();
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const isPausedRef = useRef(isPaused);

  useEffect(() => {
    isPausedRef.current = isPaused;
  }, [isPaused]);

  useEffect(() => {
    let cancelled = false;

    const setup = async () => {
      const unlisten = await listen<string>("logcat:line", (event) => {
        if (!isPausedRef.current) {
          addLine(event.payload);
        }
      });
      if (cancelled) {
        unlisten();
      } else {
        unlistenRef.current = unlisten;
      }
    };

    setup();

    return () => {
      cancelled = true;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, [addLine]);

  const start = useCallback(async () => {
    if (!currentDevice) return;
    try {
      clearLogs();
      await startLogcat(currentDevice.id, filterText || undefined);
      setIsRunning(true);
    } catch (err) {
      console.error("Failed to start logcat:", err);
    }
  }, [currentDevice, filterText, setIsRunning, clearLogs]);

  const stop = useCallback(async () => {
    try {
      await stopLogcat();
      setIsRunning(false);
    } catch (err) {
      console.error("Failed to stop logcat:", err);
    }
  }, [setIsRunning]);

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

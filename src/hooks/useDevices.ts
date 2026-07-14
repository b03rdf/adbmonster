import { useEffect, useCallback, useRef } from "react";
import { useDeviceStore } from "@/stores/deviceStore";
import { getDevices, updateTrayMenu } from "@/lib/tauri";

export function useDevices() {
  const { devices, currentDevice, isRefreshing, setDevices, setCurrentDevice, setIsRefreshing } =
    useDeviceStore();
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    setIsRefreshing(true);
    try {
      const list = await getDevices();
      setDevices(list);
      updateTrayMenu(
        list.length > 0 ? `已连接 ${list.length} 台设备` : "未连接设备",
      ).catch(() => {});
      const selectedDevice = useDeviceStore.getState().currentDevice;
      if (selectedDevice) {
        const found = list.find((d) => d.id === selectedDevice.id);
        if (!found) {
          setCurrentDevice(null);
        } else if (
          found.status !== selectedDevice.status ||
          found.model !== selectedDevice.model ||
          found.androidVersion !== selectedDevice.androidVersion
        ) {
          setCurrentDevice(found);
        }
      }
    } catch (err) {
      console.error("Failed to refresh devices:", err);
    } finally {
      setIsRefreshing(false);
      inFlightRef.current = false;
    }
  }, [setDevices, setCurrentDevice, setIsRefreshing]);

  useEffect(() => {
    let cancelled = false;
    const poll = async () => {
      await refresh();
      if (!cancelled) {
        timeoutRef.current = setTimeout(poll, 5000);
      }
    };
    poll();
    return () => {
      cancelled = true;
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, [refresh]);

  return { devices, currentDevice, isRefreshing, setCurrentDevice, refresh };
}

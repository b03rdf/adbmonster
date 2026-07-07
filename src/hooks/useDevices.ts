import { useEffect, useCallback, useRef } from "react";
import { useDeviceStore } from "@/stores/deviceStore";
import { getDevices, updateTrayMenu } from "@/lib/tauri";

export function useDevices() {
  const { devices, currentDevice, isRefreshing, setDevices, setCurrentDevice, setIsRefreshing } =
    useDeviceStore();
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    setIsRefreshing(true);
    try {
      const list = await getDevices();
      setDevices(list);
      updateTrayMenu(
        list.length > 0 ? `已连接 ${list.length} 台设备` : "未连接设备",
      ).catch(() => {});
      if (currentDevice) {
        const found = list.find((d) => d.id === currentDevice.id);
        if (!found) {
          setCurrentDevice(null);
        } else if (found.status !== currentDevice.status) {
          setCurrentDevice(found);
        }
      }
    } catch (err) {
      console.error("Failed to refresh devices:", err);
    } finally {
      setIsRefreshing(false);
    }
  }, [currentDevice, setDevices, setCurrentDevice, setIsRefreshing]);

  useEffect(() => {
    refresh();
    intervalRef.current = setInterval(refresh, 5000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [refresh]);

  return { devices, currentDevice, isRefreshing, setCurrentDevice, refresh };
}

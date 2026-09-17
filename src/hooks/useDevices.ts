import { useEffect, useCallback, useRef, useState } from "react";
import { useDeviceStore } from "@/stores/deviceStore";
import { autoConnectLocalEmulator, getDevices, updateTrayMenu } from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";

const AUTO_CONNECT_KEY = "adb-monster.auto-connect-enabled";
const EMULATOR_ADDRESS_KEY = "adb-monster.emulator-address";
const DEFAULT_EMULATOR_ADDRESS = "127.0.0.1:7555";

export function useDevices() {
  const { devices, currentDevice, isRefreshing, setDevices, setCurrentDevice, setIsRefreshing } =
    useDeviceStore();
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);
  const lastEmulatorAttemptRef = useRef(0);
  const setStatusText = useAppStore((state) => state.setStatusText);
  const [autoConnectEnabled, setAutoConnectEnabled] = useState(
    () => localStorage.getItem(AUTO_CONNECT_KEY) !== "false",
  );
  const [emulatorAddress, setEmulatorAddress] = useState(
    () => localStorage.getItem(EMULATOR_ADDRESS_KEY) || DEFAULT_EMULATOR_ADDRESS,
  );
  const [autoConnectStatus, setAutoConnectStatus] = useState("等待检测设备");

  useEffect(() => {
    localStorage.setItem(AUTO_CONNECT_KEY, String(autoConnectEnabled));
  }, [autoConnectEnabled]);

  useEffect(() => {
    localStorage.setItem(EMULATOR_ADDRESS_KEY, emulatorAddress);
  }, [emulatorAddress]);

  const refresh = useCallback(async () => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    setIsRefreshing(true);
    try {
      let list = await getDevices();
      const readyBeforeConnect = list.filter((device) => device.status === "device");
      if (
        autoConnectEnabled &&
        readyBeforeConnect.length === 0 &&
        Date.now() - lastEmulatorAttemptRef.current >= 10_000
      ) {
        lastEmulatorAttemptRef.current = Date.now();
        setAutoConnectStatus(`正在检测 ${emulatorAddress}`);
        try {
          const result = await autoConnectLocalEmulator(emulatorAddress.trim());
          setAutoConnectStatus(result.message);
          if (result.connected) {
            list = await getDevices();
            setStatusText(`已自动连接模拟器：${result.address}`);
          }
        } catch (reason) {
          setAutoConnectStatus(String(reason));
        }
      }
      setDevices(list);
      const readyCount = list.filter((device) => device.status === "device").length;
      updateTrayMenu(
        readyCount > 0 ? `已连接 ${readyCount} 台设备` : "未连接设备",
      ).catch(() => {});
      const selectedDevice = useDeviceStore.getState().currentDevice;
      const found = selectedDevice
        ? list.find((device) => device.id === selectedDevice.id)
        : undefined;
      const readyUsb = list.find(
        (device) => device.status === "device" && device.connectionType === "usb",
      );
      const firstReady = list.find((device) => device.status === "device");

      if (autoConnectEnabled) {
        const selectedUsb = found?.status === "device" && found.connectionType === "usb"
          ? found
          : undefined;
        const nextDevice = selectedUsb || readyUsb || (found?.status === "device" ? found : firstReady) || null;
        if (nextDevice?.id !== selectedDevice?.id) {
          setCurrentDevice(nextDevice);
          if (nextDevice) {
            const label = nextDevice.connectionType === "usb"
              ? "USB设备"
              : nextDevice.connectionType === "emulator"
                ? "模拟器"
                : "无线设备";
            setStatusText(`已自动选择${label}：${nextDevice.model || nextDevice.id}`);
          }
        } else if (nextDevice && found && selectedDevice && (
          found.status !== selectedDevice.status ||
          found.model !== selectedDevice.model ||
          found.androidVersion !== selectedDevice.androidVersion
        )) {
          setCurrentDevice(found);
        }
      } else if (selectedDevice) {
        if (!found || found.status !== "device") setCurrentDevice(null);
        else if (
          found.status !== selectedDevice.status ||
          found.model !== selectedDevice.model ||
          found.androidVersion !== selectedDevice.androidVersion
        ) setCurrentDevice(found);
      }
    } catch (err) {
      const message = `刷新设备失败: ${err}`;
      console.error(message);
      setStatusText(message);
      setAutoConnectStatus(message);
    } finally {
      setIsRefreshing(false);
      inFlightRef.current = false;
    }
  }, [autoConnectEnabled, emulatorAddress, setDevices, setCurrentDevice, setIsRefreshing, setStatusText]);

  const testEmulatorConnection = useCallback(async () => {
    setAutoConnectStatus(`正在检测 ${emulatorAddress}`);
    try {
      const result = await autoConnectLocalEmulator(emulatorAddress.trim());
      setAutoConnectStatus(result.message);
      if (result.connected) {
        setStatusText(`已连接模拟器：${result.address}`);
        await refresh();
      }
    } catch (reason) {
      setAutoConnectStatus(String(reason));
    }
  }, [emulatorAddress, refresh, setStatusText]);

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

  return {
    devices,
    currentDevice,
    isRefreshing,
    setCurrentDevice,
    refresh,
    autoConnectEnabled,
    setAutoConnectEnabled,
    emulatorAddress,
    setEmulatorAddress,
    autoConnectStatus,
    testEmulatorConnection,
  };
}

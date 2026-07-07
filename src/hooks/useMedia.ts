import { useState, useCallback } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useDeviceStore } from "@/stores/deviceStore";
import { useAppStore } from "@/stores/appStore";
import {
  takeScreenshot,
  startRecord,
  stopRecord,
  pullClog,
  installApk,
  uninstallApk,
  clearApp,
  getPackageInfo,
  getDeviceIp,
  pullApk,
} from "@/lib/tauri";

export function useMedia() {
  const [isScreenshotting, setIsScreenshotting] = useState(false);
  const [isRecordingState, setIsRecordingState] = useState(false);
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const { setStatusText, setScreenshotPath } = useAppStore();

  const captureScreenshot = useCallback(async () => {
    if (!currentDevice) return;
    setIsScreenshotting(true);
    try {
      const filePath = await save({
        defaultPath: `screenshot_${Date.now()}.png`,
        filters: [{ name: "PNG Image", extensions: ["png"] }],
      });
      if (!filePath) {
        setIsScreenshotting(false);
        return;
      }
      const result = await takeScreenshot(currentDevice.id, filePath);
      setScreenshotPath(result);
      setStatusText(`Screenshot saved: ${result}`);
    } catch (err) {
      setStatusText(`Screenshot failed: ${err}`);
    } finally {
      setIsScreenshotting(false);
    }
  }, [currentDevice, setScreenshotPath, setStatusText]);

  const toggleRecord = useCallback(async () => {
    if (!currentDevice) return;

    if (isRecordingState) {
      try {
        const filePath = await save({
          defaultPath: `recording_${Date.now()}.mp4`,
          filters: [{ name: "MP4 Video", extensions: ["mp4"] }],
        });
        if (!filePath) return;

        const result = await stopRecord(currentDevice.id, filePath);
        setIsRecordingState(false);
        setStatusText(`Recording saved: ${result}`);
      } catch (err) {
        setIsRecordingState(false);
        setStatusText(`Record failed: ${err}`);
      }
    } else {
      try {
        const remotePath = await startRecord(currentDevice.id);
        setIsRecordingState(true);
        setStatusText(`Recording started on device: ${remotePath}`);
      } catch (err) {
        setStatusText(`Failed to start recording: ${err}`);
      }
    }
  }, [currentDevice, isRecordingState, setStatusText]);

  return { isScreenshotting, isRecordingState, captureScreenshot, toggleRecord };
}

export function useApk() {
  const [isInstalling, setIsInstalling] = useState(false);
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const { setStatusText } = useAppStore();

  const install = useCallback(
    async (apkPath: string) => {
      if (!currentDevice) return;
      setIsInstalling(true);
      try {
        const result = await installApk(currentDevice.id, apkPath);
        setStatusText(result);
      } catch (err) {
        setStatusText(`Install failed: ${err}`);
      } finally {
        setIsInstalling(false);
      }
    },
    [currentDevice, setStatusText],
  );

  const uninstall = useCallback(
    async (packageName: string) => {
      if (!currentDevice) return;
      try {
        const result = await uninstallApk(currentDevice.id, packageName);
        setStatusText(result);
      } catch (err) {
        setStatusText(`Uninstall failed: ${err}`);
      }
    },
    [currentDevice, setStatusText],
  );

  const clear = useCallback(
    async (packageName: string) => {
      if (!currentDevice) return;
      try {
        await clearApp(currentDevice.id, packageName);
        setStatusText(`Cleared data for ${packageName}`);
      } catch (err) {
        setStatusText(`Clear failed: ${err}`);
      }
    },
    [currentDevice, setStatusText],
  );

  const getInfo = useCallback(
    async (packageName: string) => {
      if (!currentDevice) return "";
      try {
        return await getPackageInfo(currentDevice.id, packageName);
      } catch (err) {
        setStatusText(`Failed to get package info: ${err}`);
        return "";
      }
    },
    [currentDevice, setStatusText],
  );

  const exportClog = useCallback(
    async (packageName: string, localDir: string) => {
      if (!currentDevice) return;
      try {
        const result = await pullClog(currentDevice.id, packageName, localDir);
        setStatusText(`CLog exported: ${result}`);
      } catch (err) {
        setStatusText(`CLog export failed: ${err}`);
      }
    },
    [currentDevice, setStatusText],
  );

  const getIp = useCallback(async () => {
    if (!currentDevice) return "";
    try {
      return await getDeviceIp(currentDevice.id);
    } catch {
      return "";
    }
  }, [currentDevice]);

  const extractApk = useCallback(
    async (packageName: string) => {
      if (!currentDevice) return;
      try {
        const filePath = await save({
          defaultPath: `${packageName}.apk`,
          filters: [{ name: "APK", extensions: ["apk"] }],
        });
        if (!filePath) return;
        const result = await pullApk(currentDevice.id, packageName, filePath);
        setStatusText(`APK 已提取: ${result}`);
      } catch (err) {
        setStatusText(`提取失败: ${err}`);
      }
    },
    [currentDevice, setStatusText],
  );

  return { isInstalling, install, uninstall, clear, getInfo, exportClog, getIp, extractApk };
}

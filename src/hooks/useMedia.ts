import { useState, useCallback, useEffect } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useDeviceStore } from "@/stores/deviceStore";
import { useAppStore } from "@/stores/appStore";
import type { RecordingStatus } from "@/types/adb";
import {
  takeScreenshot,
  startRecord,
  stopRecord,
  getRecordingStatus,
  releaseRecording,
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
  const [recordingStatus, setRecordingStatus] = useState<RecordingStatus>({
    taskId: null,
    running: false,
    hasRecording: false,
    deviceId: null,
  });
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const { setStatusText, setScreenshotPath } = useAppStore();

  const syncRecordingStatus = useCallback(async () => {
    try {
      setRecordingStatus(await getRecordingStatus());
    } catch (err) {
      setStatusText(`查询录屏状态失败: ${err}`);
    }
  }, [setStatusText]);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const sync = async () => {
      try {
        const next = await getRecordingStatus();
        if (!cancelled) setRecordingStatus(next);
      } catch (err) {
        if (!cancelled) setStatusText(`查询录屏状态失败: ${err}`);
      } finally {
        if (!cancelled) timer = setTimeout(sync, 1_000);
      }
    };
    void sync();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [setStatusText]);

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
    if (!currentDevice && !recordingStatus.hasRecording) return;

    if (recordingStatus.hasRecording) {
      try {
        const filePath = await save({
          defaultPath: `recording_${Date.now()}.mp4`,
          filters: [{ name: "MP4 Video", extensions: ["mp4"] }],
        });
        if (!filePath) return;

        const result = await stopRecord(filePath);
        setStatusText(`Recording saved: ${result}`);
        await syncRecordingStatus();
      } catch (err) {
        setStatusText(`Record failed: ${err}`);
        await syncRecordingStatus();
      }
    } else {
      if (!currentDevice) return;
      try {
        const remotePath = await startRecord(currentDevice.id);
        await syncRecordingStatus();
        setStatusText(`Recording started on device: ${remotePath}`);
      } catch (err) {
        setStatusText(`Failed to start recording: ${err}`);
        await syncRecordingStatus();
      }
    }
  }, [currentDevice, recordingStatus.hasRecording, setStatusText, syncRecordingStatus]);

  const release = useCallback(async () => {
    if (!recordingStatus.taskId || recordingStatus.running) return;
    if (!confirm("释放这段录屏的占用以便开始新录屏？设备文件不会删除，但需稍后根据任务历史中的路径手动拉取。")) return;
    try {
      setStatusText(await releaseRecording(recordingStatus.taskId));
      await syncRecordingStatus();
    } catch (error) { setStatusText(`释放录屏失败：${error}`); }
  }, [recordingStatus.taskId, recordingStatus.running, setStatusText, syncRecordingStatus]);

  return {
    isScreenshotting,
    releaseRecordingState: release,
    isRecordingState: recordingStatus.hasRecording,
    isRecordingActive: recordingStatus.running,
    recordingDeviceId: recordingStatus.deviceId,
    captureScreenshot,
    toggleRecord,
  };
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
    } catch (err) {
      setStatusText(`获取 Wi-Fi IP 失败: ${err}`);
      return "";
    }
  }, [currentDevice, setStatusText]);

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

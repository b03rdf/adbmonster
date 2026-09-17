import { useState } from "react";
import { useDeviceStore } from "@/stores/deviceStore";
import { useMedia, useApk } from "@/hooks/useMedia";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Separator } from "@/components/ui/separator";
import { open } from "@tauri-apps/plugin-dialog";
import { PackageList } from "@/components/PackageList";
import { FileManager } from "@/components/FileManager";
import { DeviceDashboard } from "@/components/DeviceDashboard";
import { WeakNetworkPanel } from "@/components/WeakNetworkPanel";
import {
  Camera,
  Monitor,
  Square,
  Download,
  Upload,
  Trash2,
  Info,
  Clipboard,
  Wifi,
  Gauge,
  FolderOpen,
} from "lucide-react";

export function ToolPanel() {
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const {
    isScreenshotting,
    isRecordingState,
    isRecordingActive,
    recordingDeviceId,
    captureScreenshot,
    toggleRecord,
    releaseRecordingState,
  } = useMedia();
  const { isInstalling, install, uninstall, clear, getInfo, exportClog, getIp, extractApk } = useApk();

  const [packageName, setPackageName] = useState("");
  const [apkPath, setApkPath] = useState("");
  const [packageInfo, setPackageInfo] = useState("");
  const [deviceIp, setDeviceIp] = useState("");
  const [clogDir, setClogDir] = useState("");

  const handleGetIp = async () => {
    const ip = await getIp();
    setDeviceIp(ip);
  };

  const handleGetInfo = async () => {
    if (!packageName) return;
    const info = await getInfo(packageName);
    setPackageInfo(info);
  };

  const handleUninstall = async () => {
    if (!packageName || !confirm(`确定卸载 ${packageName} 吗？`)) return;
    await uninstall(packageName);
  };

  const handleClear = async () => {
    if (!packageName || !confirm(`确定清除 ${packageName} 的全部应用数据吗？此操作不可撤销。`)) {
      return;
    }
    await clear(packageName);
  };

  const handleClogDirSelect = async () => {
    const selected = await open({
      multiple: false,
      directory: true,
    });
    if (selected) setClogDir(selected);
  };

  const handleCopyIp = () => {
    if (deviceIp) {
      navigator.clipboard.writeText(deviceIp);
    }
  };

  const handleFileSelect = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Android Package", extensions: ["apk"] }],
    });
    if (!selected) return;
    setApkPath(selected);
    await install(selected);
  };

  return (
    <Card className="p-3">
      <Tabs defaultValue="dashboard" className="w-full">
        <TabsList className="w-full">
          <TabsTrigger value="dashboard" className="flex-1 text-xs"><Gauge className="h-3 w-3 mr-1" />概览</TabsTrigger>
          <TabsTrigger value="media" className="flex-1 text-xs">多媒体</TabsTrigger>
          <TabsTrigger value="files" className="flex-1 text-xs">文件</TabsTrigger>
          <TabsTrigger value="apps" className="flex-1 text-xs">应用</TabsTrigger>
          <TabsTrigger value="tools" className="flex-1 text-xs">工具</TabsTrigger>
        </TabsList>

        <TabsContent value="dashboard" className="mt-2 space-y-2">
          <DeviceDashboard />
        </TabsContent>

        <TabsContent value="media" className="mt-2 space-y-2">
          <div className="grid grid-cols-2 gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={captureScreenshot}
              disabled={!currentDevice || isScreenshotting}
              className="h-16 flex-col gap-1"
            >
              <Camera className="h-5 w-5" />
              <span className="text-[10px]">截图</span>
            </Button>

            <Button
              variant={isRecordingState ? "destructive" : "outline"}
              size="sm"
              onClick={toggleRecord}
              disabled={!currentDevice && !isRecordingState}
              className="h-16 flex-col gap-1"
            >
              {isRecordingState ? (
                <>
                  <Square className="h-5 w-5" />
                  <span className="text-[10px]">{isRecordingActive ? "停止录屏" : "保存录屏"}</span>
                </>
              ) : (
                <>
                  <Monitor className="h-5 w-5" />
                  <span className="text-[10px]">录屏</span>
                </>
              )}
            </Button>
          </div>

          {isRecordingState && (
            <p className="text-[10px] text-center text-destructive animate-pulse">
              {isRecordingActive ? "录制中..." : "录制已结束，等待保存"}
              {recordingDeviceId && recordingDeviceId !== currentDevice?.id
                ? `（设备 ${recordingDeviceId}）`
                : ""}
            </p>
          )}
          {isRecordingState && !isRecordingActive && (
            <Button variant="outline" size="sm" className="w-full text-xs" onClick={() => void releaseRecordingState()}>保留设备文件并释放录屏占用</Button>
          )}
        </TabsContent>

        <TabsContent value="files" className="mt-2 space-y-2 h-[420px]">
          <FileManager />
        </TabsContent>

        <TabsContent value="apps" className="mt-2 space-y-2">
          <div className="space-y-2">
            <div className="flex gap-2">
              <Input
                className="h-7 text-xs flex-1"
                placeholder="选择 APK..."
                value={apkPath}
                readOnly
              />
              <Button
                variant="outline"
                size="sm"
                onClick={handleFileSelect}
                disabled={!currentDevice || isInstalling}
                className="h-7"
              >
                <Upload className="h-3 w-3 mr-1" />
                安装
              </Button>
            </div>

            <Separator />

            <div className="flex gap-2">
              <Input
                className="h-7 text-xs flex-1"
                placeholder="包名 (如 com.example)"
                value={packageName}
                onChange={(e) => setPackageName(e.target.value)}
              />
              <Button
                variant="outline"
                size="sm"
                onClick={handleUninstall}
                disabled={!currentDevice || !packageName}
                className="h-7"
              >
                <Trash2 className="h-3 w-3 mr-1" />
                卸载
              </Button>
            </div>

            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleClear}
                disabled={!currentDevice || !packageName}
                className="h-7 flex-1"
              >
                清除数据
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={handleGetInfo}
                disabled={!currentDevice || !packageName}
                className="h-7 flex-1"
              >
                <Info className="h-3 w-3 mr-1" />
                信息
              </Button>
            </div>

            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => extractApk(packageName)}
                disabled={!currentDevice || !packageName}
                className="h-7 flex-1"
              >
                <Download className="h-3 w-3 mr-1" />
                提取 APK
              </Button>
            </div>

            <PackageList onSelect={(pkg) => setPackageName(pkg)} />
          </div>

          {packageInfo && (
            <pre className="text-[10px] max-h-32 overflow-auto bg-muted rounded p-2 text-muted-foreground">
              {packageInfo}
            </pre>
          )}
        </TabsContent>

        <TabsContent value="tools" className="mt-2 space-y-2">
          <div className="flex gap-2 items-center">
            <Button
              variant="outline"
              size="sm"
              onClick={handleGetIp}
              disabled={!currentDevice}
              className="h-7"
            >
              <Wifi className="h-3 w-3 mr-1" />
              获取 IP
            </Button>
            {deviceIp && (
              <span className="text-xs font-mono flex items-center gap-1">
                {deviceIp}
                <button onClick={handleCopyIp} className="hover:text-foreground text-muted-foreground">
                  <Clipboard className="h-3 w-3" />
                </button>
              </span>
            )}
          </div>

          <div className="flex gap-2">
            <Input
              className="h-7 text-xs flex-1"
              placeholder="CLog 包名..."
              value={packageName}
              onChange={(e) => setPackageName(e.target.value)}
            />
            <Input
              className="h-7 text-xs w-28"
              placeholder="输出目录..."
              value={clogDir}
              readOnly
            />
            <Button
              variant="outline"
              size="sm"
              onClick={handleClogDirSelect}
              disabled={!currentDevice}
              className="h-7 px-2"
              title="选择 CLog 输出目录"
            >
              <FolderOpen className="h-3 w-3" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => exportClog(packageName, clogDir)}
              disabled={!currentDevice || !packageName || !clogDir}
              className="h-7"
            >
              <Download className="h-3 w-3 mr-1" />
              CLog
            </Button>
          </div>

          <Separator />
          <WeakNetworkPanel />
        </TabsContent>
      </Tabs>
    </Card>
  );
}

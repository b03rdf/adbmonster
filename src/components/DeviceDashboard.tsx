import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  BatteryCharging,
  BatteryMedium,
  Clock3,
  Cpu,
  Download,
  HardDrive,
  MemoryStick,
  RefreshCw,
  Smartphone,
  Thermometer,
} from "lucide-react";

import { createDiagnosticPackage, getDeviceMetrics, getDiagnosticStatus } from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { useDeviceStore } from "@/stores/deviceStore";
import type { DeviceMetrics, DiagnosticProgress } from "@/types/adb";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";

function percent(used: number, total: number) {
  return total > 0 ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
}

function formatSize(kilobytes: number) {
  if (kilobytes <= 0) return "--";
  const gigabytes = kilobytes / 1024 / 1024;
  return gigabytes >= 1 ? `${gigabytes.toFixed(1)} GB` : `${(kilobytes / 1024).toFixed(0)} MB`;
}

function formatUptime(seconds: number) {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${days > 0 ? `${days}天 ` : ""}${hours}小时 ${minutes}分`;
}

function MetricCard({
  icon: Icon,
  label,
  value,
  detail,
  progress,
}: {
  icon: typeof Cpu;
  label: string;
  value: string;
  detail?: string;
  progress?: number;
}) {
  return (
    <div className="rounded-md border bg-muted/20 p-2 space-y-1.5 min-w-0">
      <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
        {label}
      </div>
      <div className="text-sm font-semibold truncate">{value}</div>
      {progress !== undefined && <Progress value={progress} className="h-1" />}
      {detail && <div className="text-[9px] text-muted-foreground truncate">{detail}</div>}
    </div>
  );
}

export function DeviceDashboard() {
  const currentDevice = useDeviceStore((state) => state.currentDevice);
  const setStatusText = useAppStore((state) => state.setStatusText);
  const [metrics, setMetrics] = useState<DeviceMetrics | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [packageName, setPackageName] = useState("");
  const [includeBugreport, setIncludeBugreport] = useState(true);
  const [diagnosing, setDiagnosing] = useState(false);
  const [diagnosticProgress, setDiagnosticProgress] = useState<DiagnosticProgress | null>(null);

  const refresh = useCallback(async () => {
    if (!currentDevice) return;
    setLoading(true);
    try {
      const nextMetrics = await getDeviceMetrics(currentDevice.id);
      setMetrics(nextMetrics);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, [currentDevice]);

  useEffect(() => {
    if (!currentDevice) {
      setMetrics(null);
      return;
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      await refresh();
      if (!cancelled) timer = setTimeout(poll, 3000);
    };
    poll();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [currentDevice, refresh]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const setup = async () => {
      const stopListening = await listen<DiagnosticProgress>("diagnostic:progress", (event) => {
        setDiagnosticProgress(event.payload);
        setDiagnosing(event.payload.stage !== "complete" && event.payload.stage !== "error");
      });
      if (cancelled) {
        stopListening();
        return;
      }
      unlisten = stopListening;
      const status = await getDiagnosticStatus();
      if (!cancelled) {
        setDiagnosing(status.running);
        setDiagnosticProgress(status.progress);
      }
    };
    setup().catch((reason) => setStatusText(`诊断状态初始化失败：${reason}`));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setStatusText]);

  const createPackage = async () => {
    if (!currentDevice) return;
    const path = await save({
      defaultPath: `diagnostic-${currentDevice.id}-${Date.now()}.zip`,
      filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
    });
    if (!path) return;
    setDiagnosing(true);
    setDiagnosticProgress({ stage: "prepare", message: "正在启动诊断", percent: 0 });
    try {
      const result = await createDiagnosticPackage(
        currentDevice.id,
        packageName.trim() || null,
        path,
        includeBugreport,
      );
      setStatusText(`诊断包已保存：${result}`);
    } catch (reason) {
      setStatusText(`诊断包生成失败：${reason}`);
      setDiagnosticProgress({ stage: "error", message: String(reason), percent: 0 });
    } finally {
      setDiagnosing(false);
    }
  };

  if (!currentDevice) {
    return (
      <div className="h-64 flex flex-col items-center justify-center text-xs text-muted-foreground gap-2">
        <Smartphone className="h-8 w-8 opacity-50" />
        请先选择一台设备
      </div>
    );
  }

  const memoryUsed = metrics ? metrics.memoryTotalKb - metrics.memoryAvailableKb : 0;
  const storageUsed = metrics ? metrics.storageTotalKb - metrics.storageAvailableKb : 0;

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="text-sm font-semibold truncate">{currentDevice.model || currentDevice.id}</span>
            <Badge variant="secondary" className="text-[9px] px-1.5 py-0">Android {currentDevice.androidVersion || "--"}</Badge>
          </div>
          <div className="text-[9px] text-muted-foreground font-mono truncate mt-0.5">{currentDevice.id}</div>
        </div>
        <Button variant="ghost" size="sm" className="h-7 w-7 p-0" onClick={refresh} disabled={loading}>
          <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
        </Button>
      </div>

      {error && <div className="text-[10px] text-destructive rounded border border-destructive/30 p-2">{error}</div>}

      <div className="grid grid-cols-2 gap-2">
        <MetricCard icon={Cpu} label="CPU 使用率" value={metrics?.cpuUsagePercent != null ? `${metrics.cpuUsagePercent.toFixed(1)}%` : "--"} progress={metrics?.cpuUsagePercent ?? 0} />
        <MetricCard icon={MemoryStick} label="内存" value={metrics ? `${formatSize(memoryUsed)} / ${formatSize(metrics.memoryTotalKb)}` : "--"} progress={metrics ? percent(memoryUsed, metrics.memoryTotalKb) : 0} detail={metrics ? `可用 ${formatSize(metrics.memoryAvailableKb)}` : undefined} />
        <MetricCard icon={HardDrive} label="内部存储" value={metrics ? `${formatSize(storageUsed)} / ${formatSize(metrics.storageTotalKb)}` : "--"} progress={metrics ? percent(storageUsed, metrics.storageTotalKb) : 0} detail={metrics ? `可用 ${formatSize(metrics.storageAvailableKb)}` : undefined} />
        <MetricCard icon={metrics?.charging ? BatteryCharging : BatteryMedium} label={metrics?.charging ? "电池（充电中）" : "电池"} value={metrics?.batteryLevel != null ? `${metrics.batteryLevel}%` : "--"} progress={metrics?.batteryLevel ?? 0} />
        <MetricCard icon={Thermometer} label="电池温度" value={metrics?.batteryTemperatureC != null ? `${metrics.batteryTemperatureC.toFixed(1)} °C` : "--"} />
        <MetricCard icon={Clock3} label="运行时间" value={metrics ? formatUptime(metrics.uptimeSeconds) : "--"} />
      </div>

      <div className="rounded-md border p-2 space-y-1">
        <div className="flex items-center gap-1 text-[10px] text-muted-foreground"><Activity className="h-3.5 w-3.5" />当前前台 Activity</div>
        <div className="text-[10px] font-mono break-all">{metrics?.foregroundActivity || "--"}</div>
      </div>

      <Card className="p-2.5 space-y-2">
        <div className="flex items-center justify-between">
          <div><div className="text-xs font-medium">一键诊断包</div><div className="text-[9px] text-muted-foreground">日志、截图、设备与应用性能数据</div></div>
          <Download className="h-4 w-4 text-primary" />
        </div>
        <Input className="h-7 text-xs" placeholder="应用包名（可选，如 com.example.app）" value={packageName} onChange={(event) => setPackageName(event.target.value)} disabled={diagnosing} />
        <label className="flex items-center gap-2 text-[10px] text-muted-foreground cursor-pointer">
          <input type="checkbox" checked={includeBugreport} onChange={(event) => setIncludeBugreport(event.target.checked)} disabled={diagnosing} />
          包含完整 Bugreport（更全面，可能需要数分钟）
        </label>
        {diagnosticProgress && (
          <div className="space-y-1">
            <div className="flex justify-between text-[9px] text-muted-foreground gap-2"><span className="truncate">{diagnosticProgress.message}</span><span>{diagnosticProgress.percent}%</span></div>
            <Progress value={diagnosticProgress.percent} className="h-1.5" />
          </div>
        )}
        <Button className="w-full h-8 text-xs" onClick={createPackage} disabled={diagnosing}>
          {diagnosing ? <RefreshCw className="h-3.5 w-3.5 mr-1.5 animate-spin" /> : <Download className="h-3.5 w-3.5 mr-1.5" />}
          {diagnosing ? "正在生成诊断包" : "生成诊断包"}
        </Button>
      </Card>
    </div>
  );
}

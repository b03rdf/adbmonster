import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Network, Play, RefreshCw, RotateCcw, ShieldAlert, Square } from "lucide-react";

import {
  applyWeakNetwork,
  detectWeakNetworkCapabilities,
  forceRestoreWeakNetwork,
  getWeakNetworkStatus,
  stopWeakNetwork,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { useDeviceStore } from "@/stores/deviceStore";
import type {
  WeakNetworkCapabilities,
  WeakNetworkConfig,
  WeakNetworkStatus,
} from "@/types/adb";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type PresetKey = "mild" | "3g" | "severe" | "custom";

const PRESETS: Record<Exclude<PresetKey, "custom">, WeakNetworkConfig> = {
  mild: {
    uploadKbps: 3_000,
    downloadKbps: 10_000,
    latencyMs: 80,
    jitterMs: 20,
    lossPercent: 0.5,
    duplicatePercent: 0,
    reorderPercent: 0,
    durationSeconds: 300,
  },
  "3g": {
    uploadKbps: 768,
    downloadKbps: 1_600,
    latencyMs: 200,
    jitterMs: 60,
    lossPercent: 2,
    duplicatePercent: 0,
    reorderPercent: 0.5,
    durationSeconds: 300,
  },
  severe: {
    uploadKbps: 128,
    downloadKbps: 512,
    latencyMs: 500,
    jitterMs: 200,
    lossPercent: 10,
    duplicatePercent: 1,
    reorderPercent: 2,
    durationSeconds: 180,
  },
};

const EMPTY_STATUS: WeakNetworkStatus = {
  active: false,
  deviceId: null,
  mode: null,
  interfaceName: null,
  expiresAt: null,
  message: "当前未启用弱网",
};

function NumericField({
  label,
  unit,
  value,
  onChange,
  disabled,
  min = 0,
  max,
  step = 1,
}: {
  label: string;
  unit: string;
  value: number;
  onChange: (value: number) => void;
  disabled?: boolean;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <div className="space-y-1 min-w-0">
      <Label className="flex items-center justify-between gap-1 text-[10px] text-muted-foreground">
        <span>{label}</span>
        <span className="text-[9px]">{unit}</span>
      </Label>
      <Input
        type="number"
        className="h-7 px-2 text-xs"
        value={value}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={(event) => onChange(Math.max(0, Number(event.target.value) || 0))}
      />
    </div>
  );
}

function formatRemaining(expiresAt: string | null, now: number) {
  if (!expiresAt) return "--";
  const seconds = Math.max(0, Math.ceil((Date.parse(expiresAt) - now) / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes.toString().padStart(2, "0")}:${(seconds % 60)
    .toString()
    .padStart(2, "0")}`;
}

export function WeakNetworkPanel() {
  const currentDevice = useDeviceStore((state) => state.currentDevice);
  const setStatusText = useAppStore((state) => state.setStatusText);
  const [preset, setPreset] = useState<PresetKey>("3g");
  const [config, setConfig] = useState<WeakNetworkConfig>({ ...PRESETS["3g"] });
  const [capabilities, setCapabilities] = useState<WeakNetworkCapabilities | null>(null);
  const [status, setStatus] = useState<WeakNetworkStatus>(EMPTY_STATUS);
  const [detecting, setDetecting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());

  const detect = useCallback(async () => {
    if (!currentDevice) {
      setCapabilities(null);
      return;
    }
    setDetecting(true);
    setError(null);
    try {
      const nextCapabilities = await detectWeakNetworkCapabilities(currentDevice.id);
      setCapabilities(nextCapabilities);
    } catch (reason) {
      setCapabilities(null);
      setError(`能力检测失败：${reason}`);
    } finally {
      setDetecting(false);
    }
  }, [currentDevice]);

  useEffect(() => {
    let cancelled = false;
    setCapabilities(null);
    if (!currentDevice) return;

    setDetecting(true);
    setError(null);
    detectWeakNetworkCapabilities(currentDevice.id)
      .then((nextCapabilities) => {
        if (!cancelled) setCapabilities(nextCapabilities);
      })
      .catch((reason) => {
        if (!cancelled) setError(`能力检测失败：${reason}`);
      })
      .finally(() => {
        if (!cancelled) setDetecting(false);
      });

    return () => {
      cancelled = true;
    };
  }, [currentDevice]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    Promise.all([
      getWeakNetworkStatus().then((nextStatus) => {
        if (!cancelled) setStatus(nextStatus);
      }),
      listen<WeakNetworkStatus>("weak-network:status", (event) => {
        setStatus(event.payload);
        setStatusText(event.payload.message);
      }).then((stopListening) => {
        if (cancelled) stopListening();
        else unlisten = stopListening;
      }),
    ]).catch((reason) => {
      if (!cancelled) setError(`弱网状态初始化失败：${reason}`);
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setStatusText]);

  useEffect(() => {
    if (!status.active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [status.active, status.expiresAt]);

  const modeLabel = useMemo(() => {
    if (detecting) return "检测中";
    if (!capabilities) return "未检测";
    if (capabilities.mode === "android_emulator") return "Android 模拟器";
    if (capabilities.mode === "root_netem") return "Root NetEm";
    return "不支持";
  }, [capabilities, detecting]);

  const updateConfig = <Key extends keyof WeakNetworkConfig>(
    key: Key,
    value: WeakNetworkConfig[Key],
  ) => {
    setPreset("custom");
    setConfig((current) => ({ ...current, [key]: value }));
  };

  const selectPreset = (value: PresetKey) => {
    setPreset(value);
    if (value !== "custom") setConfig({ ...PRESETS[value] });
  };

  const apply = async () => {
    if (!currentDevice || !capabilities?.supported) return;
    setBusy(true);
    setError(null);
    try {
      const effectiveConfig = capabilities.supportsPacketEffects
        ? config
        : {
            ...config,
            lossPercent: 0,
            duplicatePercent: 0,
            reorderPercent: 0,
          };
      const nextStatus = await applyWeakNetwork(currentDevice.id, effectiveConfig);
      setStatus(nextStatus);
      setStatusText(nextStatus.message);
    } catch (reason) {
      const message = `应用弱网失败：${reason}`;
      setError(message);
      setStatusText(message);
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    setError(null);
    try {
      const nextStatus = await stopWeakNetwork();
      setStatus(nextStatus);
      setStatusText(nextStatus.message);
    } catch (reason) {
      const message = `恢复网络失败：${reason}`;
      setError(message);
      setStatusText(message);
    } finally {
      setBusy(false);
    }
  };

  const forceRestore = async () => {
    if (!currentDevice) return;
    setBusy(true);
    setError(null);
    try {
      const nextStatus = await forceRestoreWeakNetwork(currentDevice.id);
      setStatus(nextStatus);
      setStatusText(nextStatus.message);
      await detect();
    } catch (reason) {
      const message = `强制恢复失败：${reason}`;
      setError(message);
      setStatusText(message);
    } finally {
      setBusy(false);
    }
  };

  if (!currentDevice) {
    return (
      <Card className="p-3 text-center text-xs text-muted-foreground">
        <Network className="h-5 w-5 mx-auto mb-1.5 opacity-50" />
        选择设备后可配置弱网测试
      </Card>
    );
  }

  const activeOnCurrentDevice = status.active && status.deviceId === currentDevice.id;
  const packetEffectsDisabled = !capabilities?.supportsPacketEffects || busy;

  return (
    <Card className="p-2.5 space-y-2.5">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 text-xs font-medium">
            <Network className="h-3.5 w-3.5 text-primary" />
            手游弱网测试
          </div>
          <div className="text-[9px] text-muted-foreground mt-0.5">
            带宽、延迟、抖动与丢包模拟
          </div>
        </div>
        <div className="flex items-center gap-1">
          <Badge
            variant={capabilities?.supported ? "secondary" : "outline"}
            className="text-[9px] px-1.5 py-0 whitespace-nowrap"
          >
            {modeLabel}
          </Badge>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0"
            onClick={detect}
            disabled={detecting || busy}
            title="重新检测弱网能力"
          >
            <RefreshCw className={`h-3 w-3 ${detecting ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </div>

      {capabilities && (
        <div className="rounded border bg-muted/20 p-2 text-[9px] text-muted-foreground leading-relaxed">
          {capabilities.message}
          {capabilities.interfaceName && (
            <span className="ml-1 font-mono">接口：{capabilities.interfaceName}</span>
          )}
        </div>
      )}

      {currentDevice.connectionType === "network" && (
        <div className="flex gap-1.5 rounded border border-amber-500/30 bg-amber-500/5 p-2 text-[9px] text-amber-600 dark:text-amber-400">
          <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
          弱网可能让无线 ADB 断开，建议使用 USB 调试。
        </div>
      )}

      <Select value={preset} onValueChange={(value) => selectPreset(value as PresetKey)}>
        <SelectTrigger className="h-7 text-xs px-2" disabled={!capabilities?.supported || busy}>
          <SelectValue placeholder="选择弱网预设" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="mild">轻度弱网（弱 4G）</SelectItem>
          <SelectItem value="3g">3G 网络</SelectItem>
          <SelectItem value="severe">极差网络</SelectItem>
          <SelectItem value="custom">自定义</SelectItem>
        </SelectContent>
      </Select>

      <div className="grid grid-cols-2 gap-2">
        <NumericField
          label="下行带宽"
          unit="Kbps"
          value={config.downloadKbps}
          max={1_000_000}
          disabled={!capabilities?.supportsDownlink || busy}
          onChange={(value) => updateConfig("downloadKbps", value)}
        />
        <NumericField
          label="上行带宽"
          unit="Kbps"
          value={config.uploadKbps}
          max={1_000_000}
          disabled={!capabilities?.supportsBandwidth || busy}
          onChange={(value) => updateConfig("uploadKbps", value)}
        />
        <NumericField
          label="基础延迟"
          unit="ms"
          value={config.latencyMs}
          max={5_000}
          disabled={!capabilities?.supported || busy}
          onChange={(value) => updateConfig("latencyMs", value)}
        />
        <NumericField
          label="延迟抖动"
          unit="ms"
          value={config.jitterMs}
          max={5_000}
          disabled={!capabilities?.supported || busy}
          onChange={(value) => updateConfig("jitterMs", value)}
        />
        <NumericField
          label="丢包率"
          unit="%"
          value={config.lossPercent}
          max={100}
          step={0.1}
          disabled={packetEffectsDisabled}
          onChange={(value) => updateConfig("lossPercent", value)}
        />
        <NumericField
          label="重复包"
          unit="%"
          value={config.duplicatePercent}
          max={100}
          step={0.1}
          disabled={packetEffectsDisabled}
          onChange={(value) => updateConfig("duplicatePercent", value)}
        />
        <NumericField
          label="乱序率"
          unit="%"
          value={config.reorderPercent}
          max={100}
          step={0.1}
          disabled={packetEffectsDisabled}
          onChange={(value) => updateConfig("reorderPercent", value)}
        />
        <NumericField
          label="测试时长"
          unit="秒"
          value={config.durationSeconds}
          min={10}
          max={3_600}
          disabled={!capabilities?.supported || busy}
          onChange={(value) => updateConfig("durationSeconds", value)}
        />
      </div>

      {capabilities?.mode === "android_emulator" && (
        <div className="text-[9px] text-muted-foreground">
          官方模拟器控制台不支持丢包、重复包和乱序，因此这些参数已停用。
        </div>
      )}
      {capabilities?.mode === "root_netem" && (
        <div className="text-[9px] text-muted-foreground">
          Root NetEm 当前只限制设备出口（上行）；下行限速需 VPN/IFB，暂不生效。
        </div>
      )}

      {status.active && (
        <div className="flex items-center justify-between gap-2 rounded border border-primary/30 bg-primary/5 p-2">
          <div className="min-w-0">
            <div className="text-[10px] font-medium text-primary">
              {activeOnCurrentDevice ? "弱网运行中" : "其他设备正在弱网测试"}
            </div>
            <div className="text-[9px] text-muted-foreground truncate" title={status.deviceId ?? ""}>
              {status.deviceId ?? "--"}
            </div>
          </div>
          <div className="font-mono text-xs font-semibold tabular-nums">
            {formatRemaining(status.expiresAt, now)}
          </div>
        </div>
      )}

      {error && (
        <div className="rounded border border-destructive/30 p-2 text-[9px] text-destructive break-all">
          {error}
        </div>
      )}

      <div className="grid grid-cols-2 gap-2">
        <Button
          size="sm"
          className="h-8 text-xs"
          onClick={apply}
          disabled={!capabilities?.supported || busy}
        >
          {busy ? (
            <RefreshCw className="h-3.5 w-3.5 mr-1 animate-spin" />
          ) : (
            <Play className="h-3.5 w-3.5 mr-1" />
          )}
          应用弱网
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="h-8 text-xs"
          onClick={stop}
          disabled={!status.active || busy}
        >
          <Square className="h-3.5 w-3.5 mr-1" />
          停止并恢复
        </Button>
      </div>

      <Button
        variant="outline"
        size="sm"
        className="h-7 w-full text-[10px]"
        onClick={forceRestore}
        disabled={busy}
        title="应用重启或状态异常时，重新探测并清理当前设备的弱网规则"
      >
        <RotateCcw className="h-3 w-3 mr-1" />
        强制恢复当前设备网络
      </Button>
    </Card>
  );
}

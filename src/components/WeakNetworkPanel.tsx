import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Download,
  KeyRound,
  Network,
  Play,
  RefreshCw,
  ShieldAlert,
  Square,
} from "lucide-react";

import {
  applyWeakNetwork,
  authorizeWeakNetworkHelper,
  detectWeakNetworkCapabilities,
  getWeakNetworkStatus,
  installWeakNetworkHelper,
  listPackages,
  stopWeakNetwork,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { reconcileWeakNetworkStatus } from "@/lib/weakNetworkState";
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

const WeakNetworkScenarios = lazy(() => import("./WeakNetworkScenarios").then(module => ({ default: module.WeakNetworkScenarios })));

type PresetKey = "mild" | "3g" | "severe" | "custom";
type NetworkProfile = Omit<WeakNetworkConfig, "targetPackage">;

const PRESETS: Record<Exclude<PresetKey, "custom">, NetworkProfile> = {
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
  targetPackage: null,
  expiresAt: null,
  message: "当前未启用 VPN 弱网",
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
        onChange={(event) => {
          const parsed = Number(event.target.value);
          const normalized = Number.isFinite(parsed) ? parsed : min;
          onChange(Math.min(max ?? Number.POSITIVE_INFINITY, Math.max(min, normalized)));
        }}
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

function delay(milliseconds: number) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

export function WeakNetworkPanel() {
  const currentDevice = useDeviceStore((state) => state.currentDevice);
  const setStatusText = useAppStore((state) => state.setStatusText);
  const deviceId = currentDevice?.id;
  const [preset, setPreset] = useState<PresetKey>("3g");
  const [config, setConfig] = useState<WeakNetworkConfig>({
    targetPackage: "",
    ...PRESETS["3g"],
  });
  const [packages, setPackages] = useState<string[]>([]);
  const [capabilities, setCapabilities] = useState<WeakNetworkCapabilities | null>(null);
  const [status, setStatus] = useState<WeakNetworkStatus>(EMPTY_STATUS);
  const [detecting, setDetecting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());

  const detect = useCallback(async () => {
    if (!deviceId) {
      setCapabilities(null);
      return null;
    }
    setDetecting(true);
    setError(null);
    try {
      const [next, nextStatus] = await Promise.all([
        detectWeakNetworkCapabilities(deviceId),
        getWeakNetworkStatus(deviceId),
      ]);
      setCapabilities(reconcileWeakNetworkStatus(next, nextStatus, deviceId));
      setStatus(nextStatus);
      return next;
    } catch (reason) {
      setCapabilities(null);
      setError(`能力检测失败：${reason}`);
      return null;
    } finally {
      setDetecting(false);
    }
  }, [deviceId]);

  useEffect(() => {
    let cancelled = false;
    setCapabilities(null);
    setPackages([]);
    setStatus(EMPTY_STATUS);
    if (!deviceId) return;

    setDetecting(true);
    setError(null);
    Promise.all([
      detectWeakNetworkCapabilities(deviceId),
      getWeakNetworkStatus(deviceId),
      listPackages(deviceId),
    ])
      .then(([nextCapabilities, nextStatus, nextPackages]) => {
        if (cancelled) return;
        setCapabilities(reconcileWeakNetworkStatus(nextCapabilities, nextStatus, deviceId));
        setStatus(nextStatus);
        setPackages(nextPackages);
      })
      .catch((reason) => {
        if (!cancelled) setError(`弱网状态初始化失败：${reason}`);
      })
      .finally(() => {
        if (!cancelled) setDetecting(false);
      });

    return () => {
      cancelled = true;
    };
  }, [deviceId]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<WeakNetworkStatus>("weak-network:status", (event) => {
      if (cancelled) return;
      setStatus(event.payload);
      setCapabilities((current) => reconcileWeakNetworkStatus(current, event.payload, deviceId));
      setStatusText(event.payload.message);
    }).then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [deviceId, setStatusText]);

  useEffect(() => {
    if (!status.active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [status.active, status.expiresAt]);

  const modeLabel = useMemo(() => {
    if (detecting) return "检测中";
    if (!capabilities) return "未检测";
    if (!capabilities.helperInstalled) return "未安装";
    if (capabilities.helperUpdateRequired) return "需更新";
    if (!capabilities.vpnAuthorized) return "需授权";
    if (status.active && status.deviceId === deviceId) return "运行中";
    return "VPN 就绪";
  }, [capabilities, detecting, status, deviceId]);

  const updateConfig = <Key extends keyof WeakNetworkConfig>(
    key: Key,
    value: WeakNetworkConfig[Key],
  ) => {
    if (key !== "targetPackage") setPreset("custom");
    setConfig((current) => ({ ...current, [key]: value }));
  };

  const selectPreset = (value: PresetKey) => {
    setPreset(value);
    if (value !== "custom") {
      setConfig((current) => ({ targetPackage: current.targetPackage, ...PRESETS[value] }));
    }
  };

  const installHelper = async () => {
    if (!deviceId) return;
    setBusy(true);
    setError(null);
    try {
      const next = await installWeakNetworkHelper(deviceId);
      setCapabilities(next);
      setStatusText("VPN 弱网助手安装成功，请继续授权");
    } catch (reason) {
      const message = `安装弱网助手失败：${reason}`;
      setError(message);
      setStatusText(message);
    } finally {
      setBusy(false);
    }
  };

  const authorizeHelper = async () => {
    if (!deviceId) return;
    setBusy(true);
    setError(null);
    try {
      const message = await authorizeWeakNetworkHelper(deviceId);
      setStatusText(message);
      for (let attempt = 0; attempt < 30; attempt += 1) {
        await delay(500);
        const next = await detectWeakNetworkCapabilities(deviceId);
        setCapabilities(next);
        if (next.vpnAuthorized) {
          setStatusText("VPN 授权成功，可以开始弱网测试");
          return;
        }
      }
      setError("未检测到 VPN 授权，请确认设备上的系统弹窗");
    } catch (reason) {
      const message = `VPN 授权失败：${reason}`;
      setError(message);
      setStatusText(message);
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!deviceId || !capabilities?.supported || !config.targetPackage) return;
    setBusy(true);
    setError(null);
    try {
      const next = await applyWeakNetwork(deviceId, config);
      setStatus(next);
      setStatusText(next.message);
      await detect();
    } catch (reason) {
      const message = `应用弱网失败：${reason}`;
      setError(message);
      setStatusText(message);
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    if (!deviceId) return;
    setBusy(true);
    setError(null);
    try {
      const next = await stopWeakNetwork(deviceId);
      setStatus(next);
      setStatusText(next.message);
      await detect();
    } catch (reason) {
      const message = `恢复网络失败：${reason}`;
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
        选择设备后可配置非 Root VPN 弱网
      </Card>
    );
  }

  const activeOnCurrentDevice = status.active && status.deviceId === deviceId;
  const setupRequired =
    !capabilities?.helperInstalled || capabilities.helperUpdateRequired;
  const authorizationRequired =
    capabilities?.helperInstalled &&
    !capabilities.helperUpdateRequired &&
    !capabilities.vpnAuthorized;
  const controlsDisabled = !capabilities?.supported || busy;

  return (
    <Card className="p-2.5 space-y-2.5">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 text-xs font-medium">
            <Network className="h-3.5 w-3.5 text-primary" />
            手游弱网测试
          </div>
          <div className="text-[9px] text-muted-foreground mt-0.5">
            非 Root · 仅代理指定应用
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
            title="重新检测弱网助手状态"
          >
            <RefreshCw className={`h-3 w-3 ${detecting ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </div>

      {capabilities && (
        <div className="rounded border bg-muted/20 p-2 text-[9px] text-muted-foreground leading-relaxed">
          <div>{capabilities.message}</div>
          {capabilities.helperVersion && (
            <div className="mt-0.5 font-mono">助手版本：{capabilities.helperVersion}</div>
          )}
        </div>
      )}

      {capabilities && setupRequired && (
        <Button
          variant="outline"
          size="sm"
          className="h-8 w-full text-xs"
          onClick={installHelper}
          disabled={busy || detecting}
        >
          {busy ? (
            <RefreshCw className="h-3.5 w-3.5 mr-1 animate-spin" />
          ) : (
            <Download className="h-3.5 w-3.5 mr-1" />
          )}
          {capabilities.helperUpdateRequired ? "更新 VPN 弱网助手" : "安装 VPN 弱网助手"}
        </Button>
      )}

      {authorizationRequired && (
        <Button
          variant="outline"
          size="sm"
          className="h-8 w-full text-xs"
          onClick={authorizeHelper}
          disabled={busy || detecting}
        >
          {busy ? (
            <RefreshCw className="h-3.5 w-3.5 mr-1 animate-spin" />
          ) : (
            <KeyRound className="h-3.5 w-3.5 mr-1" />
          )}
          在设备上授权 VPN
        </Button>
      )}

      <div className="space-y-1">
        <Label className="text-[10px] text-muted-foreground">目标应用包名</Label>
        <Input
          list="weak-network-packages"
          className="h-7 px-2 font-mono text-xs"
          value={config.targetPackage}
          placeholder="com.example.game"
          disabled={controlsDisabled}
          onChange={(event) => updateConfig("targetPackage", event.target.value.trim())}
        />
        <datalist id="weak-network-packages">
          {packages.map((packageName) => (
            <option value={packageName} key={packageName} />
          ))}
        </datalist>
      </div>

      <Select value={preset} onValueChange={(value) => selectPreset(value as PresetKey)}>
        <SelectTrigger className="h-7 text-xs px-2" disabled={controlsDisabled}>
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
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("downloadKbps", value)}
        />
        <NumericField
          label="上行带宽"
          unit="Kbps"
          value={config.uploadKbps}
          max={1_000_000}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("uploadKbps", value)}
        />
        <NumericField
          label="基础延迟"
          unit="ms"
          value={config.latencyMs}
          max={5_000}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("latencyMs", value)}
        />
        <NumericField
          label="延迟抖动"
          unit="ms"
          value={config.jitterMs}
          max={5_000}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("jitterMs", value)}
        />
        <NumericField
          label="丢包率"
          unit="%"
          value={config.lossPercent}
          max={100}
          step={0.1}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("lossPercent", value)}
        />
        <NumericField
          label="重复包"
          unit="%"
          value={config.duplicatePercent}
          max={100}
          step={0.1}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("duplicatePercent", value)}
        />
        <NumericField
          label="乱序率"
          unit="%"
          value={config.reorderPercent}
          max={100}
          step={0.1}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("reorderPercent", value)}
        />
        <NumericField
          label="测试时长"
          unit="秒"
          value={config.durationSeconds}
          min={10}
          max={3_600}
          disabled={controlsDisabled}
          onChange={(value) => updateConfig("durationSeconds", value)}
        />
      </div>

      <div className="flex gap-1.5 rounded border border-amber-500/30 bg-amber-500/5 p-2 text-[9px] text-amber-600 dark:text-amber-400">
        <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
        <span>
          只有所选应用经过 VPN。带宽、延迟和抖动对 TCP/UDP 生效；丢包、重复包和乱序仅对 UDP 生效。
          Android 同时只能启用一个 VPN，测试前请先关闭设备上的其他 VPN。
        </span>
      </div>

      {status.active && (
        <div className="flex items-center justify-between gap-2 rounded border border-primary/30 bg-primary/5 p-2">
          <div className="min-w-0">
            <div className="text-[10px] font-medium text-primary">
              {activeOnCurrentDevice ? "VPN 弱网运行中" : "其他设备正在弱网测试"}
            </div>
            <div
              className="text-[9px] text-muted-foreground truncate font-mono"
              title={status.targetPackage ?? capabilities?.activeTargetPackage ?? ""}
            >
              {status.targetPackage ?? capabilities?.activeTargetPackage ?? "--"}
            </div>
          </div>
          <div className="font-mono text-xs font-semibold tabular-nums">
            {formatRemaining(status.expiresAt ?? capabilities?.expiresAt ?? null, now)}
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
          disabled={controlsDisabled || !config.targetPackage || setupRequired || authorizationRequired}
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
          disabled={!activeOnCurrentDevice || busy}
        >
          <Square className="h-3.5 w-3.5 mr-1" />
          停止并恢复
        </Button>
      </div>
      <Suspense fallback={<div className="text-xs text-muted-foreground">加载场景与观测面板…</div>}>
      <WeakNetworkScenarios
        key={currentDevice.id}
        deviceId={currentDevice.id}
        currentConfig={config}
        ready={Boolean(capabilities?.supported)}
        busy={busy}
        setBusy={setBusy}
        active={activeOnCurrentDevice}
        onStop={stop}
        onStatus={(next) => { setStatus(next); setStatusText(next.message); }}
      />
      </Suspense>
    </Card>
  );
}

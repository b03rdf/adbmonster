import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { useDevices } from "@/hooks/useDevices";
import { Cable, MonitorSmartphone, RefreshCw, Settings2, Wifi, WifiOff } from "lucide-react";

function statusColor(status: string) {
  switch (status) {
    case "device":
      return "bg-green-500/10 text-green-600 border-green-200 dark:border-green-800" as const;
    case "offline":
      return "bg-yellow-500/10 text-yellow-600 border-yellow-200 dark:border-yellow-800" as const;
    case "unauthorized":
      return "bg-red-500/10 text-red-600 border-red-200 dark:border-red-800" as const;
    default:
      return "bg-muted text-muted-foreground" as const;
  }
}

function DeviceStatus({ status }: { status: string }) {
  return (
    <span
      className={
        "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium " +
        statusColor(status)
      }
    >
      {status === "device" ? <Wifi className="h-3 w-3" /> : <WifiOff className="h-3 w-3" />}
      {status}
    </span>
  );
}

export function DeviceSelector() {
  const {
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
  } = useDevices();

  const handleValueChange = (value: string) => {
    const device = devices.find((item) => item.id === value && item.status === "device") || null;
    setCurrentDevice(device);
  };

  return (
    <Card className="p-3 flex items-center gap-3">
      <div className="flex-1 flex items-center gap-2">
        <Select value={currentDevice?.id || ""} onValueChange={handleValueChange}>
          <SelectTrigger className="w-[280px]">
            <SelectValue placeholder="选择设备..." />
          </SelectTrigger>
          <SelectContent>
            {devices.length === 0 && (
              <div className="px-2 py-4 text-center text-sm text-muted-foreground">
                未发现设备
              </div>
            )}
            {devices.map((device) => (
              <SelectItem
                key={device.id}
                value={device.id}
                disabled={device.status !== "device"}
              >
                <span className="flex items-center gap-2">
                  {device.connectionType === "usb" ? (
                    <Cable className="h-3 w-3" />
                  ) : (
                    <MonitorSmartphone className="h-3 w-3" />
                  )}
                  <span>{device.model || device.id}</span>
                  <DeviceStatus status={device.status} />
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {currentDevice && (
          <span className="text-xs text-muted-foreground ml-1">
            {currentDevice.androidVersion && `Android ${currentDevice.androidVersion}`}
          </span>
        )}
      </div>

      <Button
        variant="outline"
        size="icon"
        onClick={refresh}
        disabled={isRefreshing}
        title="刷新设备列表"
      >
        <RefreshCw className={`h-4 w-4 ${isRefreshing ? "animate-spin" : ""}`} />
      </Button>

      <Dialog>
        <DialogTrigger asChild>
          <Button variant="outline" size="icon" title="自动连接设置">
            <Settings2 className="h-4 w-4" />
          </Button>
        </DialogTrigger>
        <DialogContent className="sm:max-w-[430px]">
          <DialogHeader>
            <DialogTitle>自动连接设备</DialogTitle>
            <DialogDescription>
              优先选择USB调试设备；没有可用设备时检测本地模拟器端口。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2">
            <label className="flex items-center justify-between gap-3 text-sm">
              <span>
                <span className="block font-medium">启用自动连接</span>
                <span className="block text-xs text-muted-foreground mt-0.5">插入USB后自动选中，断开后自动切换模拟器</span>
              </span>
              <input
                type="checkbox"
                checked={autoConnectEnabled}
                onChange={(event) => setAutoConnectEnabled(event.target.checked)}
              />
            </label>
            <div className="space-y-1.5">
              <label className="text-xs font-medium">本地模拟器地址</label>
              <Input
                value={emulatorAddress}
                onChange={(event) => setEmulatorAddress(event.target.value)}
                placeholder="127.0.0.1:7555"
                disabled={!autoConnectEnabled}
              />
              <p className="text-[10px] text-muted-foreground">出于安全考虑，只允许127.0.0.1或::1回环地址。</p>
            </div>
            <div className="rounded-md border bg-muted/30 px-3 py-2 text-xs break-all">
              {autoConnectStatus}
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={testEmulatorConnection}
              disabled={!autoConnectEnabled || !emulatorAddress.trim()}
            >
              立即检测并连接
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Card } from "@/components/ui/card";
import { useDevices } from "@/hooks/useDevices";
import { RefreshCw, Wifi, WifiOff } from "lucide-react";

function statusColor(status: string) {
  switch (status) {
    case "device":
      return "bg-green-500/10 text-green-600 border-green-200 dark:border-green-800" as const;
    case "offline":
      return "bg-yellow-500/10 text-yellow-600 border-yellow-200 dark:border-yellow-800" as const;
    case "unauthorized":
      return "bg-red-500/10 text-red-600 border-red-200 dark:border-red-800" as const;
    default:
      return undefined;
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
  const { devices, currentDevice, isRefreshing, setCurrentDevice, refresh } = useDevices();

  const handleValueChange = (value: string) => {
    const device = devices.find((d) => d.id === value) || null;
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
              <SelectItem key={device.id} value={device.id}>
                <span className="flex items-center gap-2">
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
    </Card>
  );
}

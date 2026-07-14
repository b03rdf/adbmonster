import { useEffect, useState, useCallback } from "react";
import { DeviceSelector } from "@/components/DeviceSelector";
import { LogViewer } from "@/components/LogViewer";
import { ToolPanel } from "@/components/ToolPanel";
import { StatusBar } from "@/components/StatusBar";
import { Button } from "@/components/ui/button";
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
import { connectDevice, tcpipConnect, startScrcpy, stopScrcpy, isScrcpyRunning } from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { Plus, Monitor, MonitorOff } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

function ConnectDialog() {
  const [ip, setIp] = useState("");
  const [port, setPort] = useState("5555");
  const [open, setOpen] = useState(false);
  const { setStatusText } = useAppStore();

  const handleConnect = async () => {
    if (!ip) return;
    try {
      const addr = `${ip}:${port}`;
      const result = await connectDevice(addr);
      setStatusText(`已连接: ${result}`);
      setOpen(false);
    } catch (err) {
      setStatusText(`连接失败: ${err}`);
    }
  };

  const handleTcpip = async () => {
    try {
      const result = await tcpipConnect(parseInt(port));
      setStatusText(result);
    } catch (err) {
      setStatusText(`tcpip 设置失败: ${err}`);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="h-7 text-xs">
          <Plus className="h-3 w-3 mr-1" />
          连接
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle>无线连接设备</DialogTitle>
          <DialogDescription>
            请先在设备上开启无线调试 (adb tcpip 5555)
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 py-2">
          <div className="flex gap-2 items-center">
            <Input
              className="h-8 text-sm flex-1"
              placeholder="设备 IP 地址"
              value={ip}
              onChange={(e) => setIp(e.target.value)}
            />
            <span className="text-xs text-muted-foreground">:</span>
            <Input
              className="h-8 text-sm w-20"
              placeholder="端口"
              value={port}
              onChange={(e) => setPort(e.target.value)}
            />
          </div>
          <Button variant="outline" size="sm" onClick={handleTcpip} className="h-7 text-xs">
            设置设备 tcpip 模式
          </Button>
        </div>
        <DialogFooter>
          <Button size="sm" onClick={handleConnect}>
            连接
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ScrcpyButton() {
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const { setStatusText } = useAppStore();
  const [running, setRunning] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;

    const syncState = async () => {
      try {
        const nextRunning = await isScrcpyRunning();
        if (!cancelled) setRunning(nextRunning);
      } catch (err) {
        if (!cancelled) setStatusText(`Failed to query scrcpy status: ${err}`);
      } finally {
        if (!cancelled) timer = setTimeout(syncState, 2000);
      }
    };

    syncState();
    listen("scrcpy:stopped", () => setRunning(false)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    }).catch((err) => {
      if (!cancelled) setStatusText(`Failed to listen for scrcpy events: ${err}`);
    });

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      unlisten?.();
    };
  }, [setStatusText]);

  const toggle = useCallback(async () => {
    if (!currentDevice) return;
    try {
      if (running) {
        await stopScrcpy();
        setRunning(false);
        setStatusText("屏幕投影已停止");
      } else {
        const result = await startScrcpy(currentDevice.id);
        setRunning(true);
        setStatusText(result);
      }
    } catch (err) {
      setStatusText(`scrcpy 错误: ${err}`);
    }
  }, [currentDevice, running, setStatusText]);

  return (
    <Button
      variant={running ? "default" : "outline"}
      size="sm"
      className="h-7 text-xs"
      disabled={!currentDevice}
      onClick={toggle}
      title={running ? "停止屏幕投影" : "启动 scrcpy 实时投屏"}
    >
      {running ? <MonitorOff className="h-3 w-3 mr-1" /> : <Monitor className="h-3 w-3 mr-1" />}
      {running ? "停止投屏" : "屏幕投影"}
    </Button>
  );
}

const MIN_SIDEBAR_WIDTH = 260;
const MAX_SIDEBAR_WIDTH = 700;
const SIDEBAR_WIDTH_KEY = "adb-monster.sidebar-width";

function useSidebarWidth() {
  const [width, setWidth] = useState(() => {
    const saved = localStorage.getItem(SIDEBAR_WIDTH_KEY);
    const parsed = saved ? parseInt(saved, 10) : 320;
    return Math.max(MIN_SIDEBAR_WIDTH, Math.min(MAX_SIDEBAR_WIDTH, parsed));
  });
  const [isResizing, setIsResizing] = useState(false);

  useEffect(() => {
    localStorage.setItem(SIDEBAR_WIDTH_KEY, width.toString());
  }, [width]);

  useEffect(() => {
    if (!isResizing) {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      return;
    }
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    const handleMouseMove = (e: MouseEvent) => {
      const newWidth = window.innerWidth - e.clientX - 8;
      setWidth(Math.max(MIN_SIDEBAR_WIDTH, Math.min(MAX_SIDEBAR_WIDTH, newWidth)));
    };
    const handleMouseUp = () => setIsResizing(false);

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isResizing]);

  return { width, isResizing, startResize: () => setIsResizing(true) };
}

function App() {
  const { width: sidebarWidth, isResizing, startResize } = useSidebarWidth();

  return (
    <div className="h-screen w-screen flex flex-col bg-background text-foreground overflow-hidden">
      <header className="flex items-center justify-between px-4 py-2 border-b shrink-0 bg-card">
        <div className="flex items-center gap-2">
          <Monitor className="h-5 w-5 text-primary" />
          <h1 className="text-sm font-semibold">adb缝合怪</h1>
        </div>
        <div className="flex items-center gap-2">
          <DeviceSelector />
          <ScrcpyButton />
          <ConnectDialog />
        </div>
      </header>

      <div className="flex-1 flex gap-2 p-2 overflow-hidden min-h-0">
        <div className="flex-1 flex flex-col rounded-lg border overflow-hidden bg-card min-w-0">
          <div className="flex-1 overflow-hidden">
            <LogViewer />
          </div>
        </div>

        <div
          className="flex flex-row items-stretch shrink-0 h-full"
          style={{ width: sidebarWidth }}
        >
          <div
            className={`w-1.5 cursor-col-resize shrink-0 rounded-full hover:bg-primary/40 active:bg-primary/70 transition-colors ${
              isResizing ? "bg-primary/70" : ""
            }`}
            onMouseDown={startResize}
            title="拖动调整宽度"
          />
          <div className="flex-1 overflow-y-auto min-w-0">
            <ToolPanel />
          </div>
        </div>
      </div>

      <StatusBar />
    </div>
  );
}

export default App;

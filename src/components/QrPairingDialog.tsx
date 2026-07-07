import { useState } from "react";
import { QRCodeCanvas } from "qrcode.react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { pairDevice, connectDevice } from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { QrCode, Check, Loader2 } from "lucide-react";

export function QrPairingDialog() {
  const { setStatusText } = useAppStore();
  const [open, setOpen] = useState(false);
  const [ip, setIp] = useState("");
  const [pairPort, setPairPort] = useState("37279");
  const [connectPort, setConnectPort] = useState("5555");
  const [code, setCode] = useState("");
  const [pairing, setPairing] = useState(false);
  const [paired, setPaired] = useState(false);

  const handlePair = async () => {
    if (!ip || !code) return;
    setPaired(false);
    setPairing(true);
    try {
      const pairResult = await pairDevice(ip, parseInt(pairPort), code);
      const connectResult = await connectDevice(`${ip}:${connectPort}`);
      setStatusText(`配对成功: ${pairResult}; ${connectResult}`);
      setPaired(true);
    } catch (err) {
      setStatusText(`配对失败: ${err}`);
    } finally {
      setPairing(false);
    }
  };

  const qrValue = `adb pair ${ip}:${pairPort} ${code}`;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="h-7 text-xs">
          <QrCode className="h-3 w-3 mr-1" />
          二维码配对
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle>二维码配对</DialogTitle>
          <DialogDescription>
            在手机「无线调试」中查看配对码，填入下方后自动配对连接
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 py-2">
          <div className="flex gap-2 items-center">
            <Input
              className="h-8 text-sm flex-1"
              placeholder="设备 IP"
              value={ip}
              onChange={(e) => setIp(e.target.value)}
            />
            <span className="text-xs text-muted-foreground shrink-0">配对端口</span>
            <Input
              className="h-8 text-sm w-20"
              placeholder="37279"
              value={pairPort}
              onChange={(e) => setPairPort(e.target.value)}
            />
          </div>
          <div className="flex gap-2 items-center">
            <span className="text-xs text-muted-foreground shrink-0">连接端口</span>
            <Input
              className="h-8 text-sm w-20"
              placeholder="5555"
              value={connectPort}
              onChange={(e) => setConnectPort(e.target.value)}
            />
            <span className="text-xs text-muted-foreground shrink-0">配对码</span>
            <Input
              className="h-8 text-sm flex-1 font-mono tracking-widest"
              placeholder="6 位配对码"
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
            />
          </div>

          {ip && code.length === 6 && (
            <Card className="p-4 flex flex-col items-center gap-2">
              <QRCodeCanvas value={qrValue} size={200} level="M" />
              <span className="text-[10px] text-muted-foreground break-all text-center select-all">
                {qrValue}
              </span>
            </Card>
          )}

          {paired && (
            <div className="flex items-center gap-2 text-xs text-green-500 justify-center">
              <Check className="h-3.5 w-3.5" />
              配对成功，设备已连接
            </div>
          )}
        </div>
        <DialogFooter>
          <Button
            size="sm"
            onClick={handlePair}
            disabled={!ip || code.length !== 6 || pairing}
          >
            {pairing ? <Loader2 className="h-3 w-3 mr-1 animate-spin" /> : <QrCode className="h-3 w-3 mr-1" />}
            {pairing ? "配对中..." : "开始配对"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

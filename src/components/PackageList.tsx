import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { listPackages } from "@/lib/tauri";
import { useDeviceStore } from "@/stores/deviceStore";
import { Package, Search, Loader2 } from "lucide-react";

export function PackageList({ onSelect }: { onSelect: (pkg: string) => void }) {
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const [open, setOpen] = useState(false);
  const [packages, setPackages] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    if (open && currentDevice && packages.length === 0 && !loading) {
      setLoading(true);
      listPackages(currentDevice.id)
        .then(setPackages)
        .finally(() => setLoading(false));
    }
  }, [open, currentDevice]);

  const filtered = filter
    ? packages.filter((p) => p.toLowerCase().includes(filter.toLowerCase()))
    : packages;

  const handleSelect = (pkg: string) => {
    onSelect(pkg);
    setOpen(false);
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="h-7 text-xs flex-1">
          <Package className="h-3 w-3 mr-1" />
          第三方包列表
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-[500px] max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>第三方应用包名</DialogTitle>
        </DialogHeader>
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            className="h-8 text-xs pl-7"
            placeholder="搜索包名..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <div className="flex-1 overflow-y-auto mt-2 space-y-0.5 min-h-0">
          {loading ? (
            <div className="flex items-center justify-center py-8 text-xs text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" />
              加载中...
            </div>
          ) : filtered.length === 0 ? (
            <div className="py-8 text-center text-xs text-muted-foreground">
              {filter ? "无匹配结果" : "未找到第三方应用"}
            </div>
          ) : (
            filtered.map((pkg) => (
              <div
                key={pkg}
                className="flex items-center px-2 py-1.5 rounded hover:bg-muted cursor-pointer text-xs"
                onClick={() => handleSelect(pkg)}
              >
                <span className="font-mono text-[11px]">{pkg}</span>
              </div>
            ))
          )}
        </div>
        <div className="text-[10px] text-muted-foreground text-center pt-2 border-t mt-2">
          共 {packages.length} 个第三方应用
        </div>
      </DialogContent>
    </Dialog>
  );
}

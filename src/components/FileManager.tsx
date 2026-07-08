import { useEffect, useState, useCallback } from "react";
import { useDeviceStore } from "@/stores/deviceStore";
import { useAppStore } from "@/stores/appStore";
import {
  listRemoteFiles,
  pushFileToRemote,
  deleteRemoteFile,
  createRemoteDirectory,
  pullFile,
} from "@/lib/tauri";
import type { RemoteFile } from "@/types/adb";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Folder,
  File,
  ChevronRight,
  Home,
  RefreshCw,
  Upload,
  Download,
  Trash2,
  FolderPlus,
  ArrowUp,
} from "lucide-react";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "-";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

export function FileManager() {
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const { setStatusText } = useAppStore();
  const [currentPath, setCurrentPath] = useState("/sdcard");
  const [files, setFiles] = useState<RemoteFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [newDirName, setNewDirName] = useState("");
  const [showNewDir, setShowNewDir] = useState(false);

  const loadFiles = useCallback(async () => {
    if (!currentDevice) return;
    setLoading(true);
    try {
      const list = await listRemoteFiles(currentDevice.id, currentPath);
      setFiles(list);
      setError(null);
      setStatusText(`已加载 ${currentPath}，共 ${list.length} 项`);
    } catch (err) {
      const msg = String(err);
      setError(msg);
      setStatusText(`读取目录失败: ${msg}`);
      setFiles([]);
    } finally {
      setLoading(false);
    }
  }, [currentDevice, currentPath, setStatusText]);

  useEffect(() => {
    loadFiles();
  }, [loadFiles]);

  const navigateUp = () => {
    if (currentPath === "/") return;
    const parent = currentPath.replace(/\/[^/]*\/?$/, "") || "/";
    setCurrentPath(parent);
  };

  const navigateTo = (path: string) => {
    setCurrentPath(path);
  };

  const breadcrumbs = currentPath.split("/").filter(Boolean);

  const handlePush = async (localPath: string, fileName: string) => {
    if (!currentDevice) return;
    const remotePath = currentPath === "/" ? `/${fileName}` : `${currentPath}/${fileName}`;
    try {
      await pushFileToRemote(currentDevice.id, localPath, remotePath);
      setStatusText(`上传完成: ${fileName}`);
      await loadFiles();
    } catch (err) {
      setStatusText(`上传失败: ${err}`);
    }
  };

  const handleSelectFiles = async () => {
    const selected = await open({
      multiple: true,
      directory: false,
    });
    if (!selected || !Array.isArray(selected)) return;
    for (const path of selected) {
      const name = path.replace(/\\/g, "/").split("/").pop() || path;
      await handlePush(path, name);
    }
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    if (!currentDevice) return;

    const droppedFiles = e.dataTransfer.files;
    for (let i = 0; i < droppedFiles.length; i++) {
      const file = droppedFiles[i] as unknown as { path?: string; name: string };
      const localPath = file.path;
      if (!localPath) {
        setStatusText("拖拽上传需要本地文件路径，请使用选择文件按钮");
        continue;
      }
      await handlePush(localPath, file.name);
    }
  };

  const handlePull = async (file: RemoteFile) => {
    if (!currentDevice) return;
    const localPath = await save({
      defaultPath: file.name,
    });
    if (!localPath) return;
    try {
      await pullFile(currentDevice.id, file.path, localPath);
      setStatusText(`下载完成: ${file.name}`);
    } catch (err) {
      setStatusText(`下载失败: ${err}`);
    }
  };

  const handleDelete = async (file: RemoteFile) => {
    if (!currentDevice) return;
    if (!confirm(`确定删除 ${file.name} 吗？`)) return;
    try {
      await deleteRemoteFile(currentDevice.id, file.path);
      setStatusText(`已删除: ${file.name}`);
      await loadFiles();
    } catch (err) {
      setStatusText(`删除失败: ${err}`);
    }
  };

  const handleCreateDir = async () => {
    if (!currentDevice || !newDirName) return;
    const remotePath = currentPath === "/" ? `/${newDirName}` : `${currentPath}/${newDirName}`;
    try {
      await createRemoteDirectory(currentDevice.id, remotePath);
      setStatusText(`已创建目录: ${newDirName}`);
      setNewDirName("");
      setShowNewDir(false);
      await loadFiles();
    } catch (err) {
      setStatusText(`创建目录失败: ${err}`);
    }
  };

  return (
    <div className="flex flex-col h-full gap-2">
      <div className="flex items-center gap-1 flex-wrap">
        <Button variant="ghost" size="sm" className="h-7 px-1" onClick={() => navigateTo("/")}>
          <Home className="h-3.5 w-3.5" />
        </Button>
        {breadcrumbs.map((crumb, idx) => {
          const path = "/" + breadcrumbs.slice(0, idx + 1).join("/");
          const isLast = idx === breadcrumbs.length - 1;
          return (
            <div key={path} className="flex items-center">
              <ChevronRight className="h-3 w-3 text-muted-foreground" />
              <Button
                variant={isLast ? "secondary" : "ghost"}
                size="sm"
                className="h-7 text-xs px-1.5"
                onClick={() => !isLast && navigateTo(path)}
                disabled={isLast}
              >
                {crumb}
              </Button>
            </div>
          );
        })}
      </div>

      <div className="flex items-center gap-2 flex-wrap">
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          onClick={navigateUp}
          disabled={currentPath === "/" || !currentDevice}
        >
          <ArrowUp className="h-3 w-3 mr-1" />
          上级
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          onClick={loadFiles}
          disabled={!currentDevice || loading}
        >
          <RefreshCw className={`h-3 w-3 mr-1 ${loading ? "animate-spin" : ""}`} />
          刷新
        </Button>
        <Button variant="outline" size="sm" className="h-7 text-xs" onClick={handleSelectFiles} disabled={!currentDevice}>
          <Upload className="h-3 w-3 mr-1" />
          上传
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          onClick={() => setShowNewDir((v) => !v)}
          disabled={!currentDevice}
        >
          <FolderPlus className="h-3 w-3 mr-1" />
          新建文件夹
        </Button>
      </div>

      {showNewDir && (
        <div className="flex gap-2">
          <Input
            className="h-7 text-xs flex-1"
            placeholder="文件夹名称"
            value={newDirName}
            onChange={(e) => setNewDirName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreateDir()}
          />
          <Button size="sm" className="h-7 text-xs" onClick={handleCreateDir} disabled={!newDirName}>
            创建
          </Button>
        </div>
      )}

      <div
        className={`flex-1 min-h-0 rounded-md border overflow-hidden ${
          dragOver ? "border-primary bg-primary/5" : "bg-card"
        }`}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
      >
        {!currentDevice ? (
          <div className="h-full flex items-center justify-center text-xs text-muted-foreground">
            请先选择设备
          </div>
        ) : error ? (
          <div className="h-full flex flex-col items-center justify-center text-xs text-destructive gap-2 px-4 text-center">
            <span>读取目录失败</span>
            <span className="text-muted-foreground max-w-full truncate">{error}</span>
            <Button variant="outline" size="sm" className="h-7 text-xs mt-1" onClick={loadFiles}>
              重试
            </Button>
          </div>
        ) : files.length === 0 && !loading ? (
          <div className="h-full flex flex-col items-center justify-center text-xs text-muted-foreground gap-2">
            <Upload className="h-6 w-6" />
            <span>当前目录为空，或将文件拖拽到此处上传</span>
          </div>
        ) : (
          <ScrollArea className="h-full">
            <div className="min-w-full">
              <div className="grid grid-cols-[1fr_80px_120px_80px] gap-2 px-3 py-1.5 text-[10px] text-muted-foreground border-b bg-muted">
                <span>名称</span>
                <span className="text-right">大小</span>
                <span>修改时间</span>
                <span className="text-right">操作</span>
              </div>
              {loading && files.length === 0 ? (
                <div className="p-4 text-center text-xs text-muted-foreground">加载中...</div>
              ) : (
                files.map((file) => (
                  <div
                    key={file.path}
                    className="grid grid-cols-[1fr_80px_120px_80px] gap-2 px-3 py-1.5 text-xs items-center hover:bg-muted/50 border-b border-transparent last:border-b-0"
                  >
                    <button
                      className="flex items-center gap-1.5 text-left truncate hover:text-primary"
                      onClick={() => file.is_dir && navigateTo(file.path)}
                      disabled={!file.is_dir}
                    >
                      {file.is_dir ? (
                        <Folder className="h-3.5 w-3.5 text-yellow-500 shrink-0" />
                      ) : (
                        <File className="h-3.5 w-3.5 text-blue-500 shrink-0" />
                      )}
                      <span className="truncate">{file.name}</span>
                    </button>
                    <span className="text-right text-muted-foreground">{formatBytes(file.size)}</span>
                    <span className="text-muted-foreground truncate">{file.modified}</span>
                    <div className="flex justify-end gap-1">
                      {!file.is_dir && (
                        <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={() => handlePull(file)}>
                          <Download className="h-3 w-3" />
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-6 w-6 p-0 text-destructive hover:text-destructive"
                        onClick={() => handleDelete(file)}
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        )}
      </div>
    </div>
  );
}

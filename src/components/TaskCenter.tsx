import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ListTodo } from "lucide-react";
import { cancelTask, clearTaskHistory, listTasks } from "@/lib/tauri";
import { isTaskActive, visibleTasks, type TaskKind, type TaskSnapshot, type TaskStatus } from "@/lib/taskState";
import { useTaskStore } from "@/stores/taskStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogTrigger } from "@/components/ui/dialog";

const KIND: Record<TaskKind, string> = { logcat: "日志采集", recording: "录屏", recordingSave: "录屏保存", scrcpy: "屏幕投影", diagnostic: "诊断导出", weakNetwork: "VPN 弱网" };
const STATUS: Record<TaskStatus, string> = { starting: "启动中", running: "运行中", cancelling: "停止/清理中", completed: "已完成", cancelled: "已停止", failed: "失败", interrupted: "异常中断" };

export function TaskCenter() {
  const { snapshot, receive } = useTaskStore();
  const deviceId = useDeviceStore((state) => state.currentDevice?.id);
  const [currentOnly, setCurrentOnly] = useState(false);
  const [activeOnly, setActiveOnly] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string[]>([]);
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        const next = await listTasks();
        if (!disposed) { receive(next); setError(null); }
      } catch (reason) { if (!disposed) setError(`任务状态读取失败：${reason}`); }
      if (!disposed) timer = setTimeout(poll, 2000);
    };
    void listen<TaskSnapshot>("tasks:changed", (event) => { if (!disposed) receive(event.payload); })
      .then((stop) => { if (disposed) stop(); else unlisten = stop; })
      .catch((reason) => { if (!disposed) setError(`实时任务事件不可用，使用轮询：${reason}`); });
    void poll();
    return () => { disposed = true; unlisten?.(); if (timer) clearTimeout(timer); };
  }, [receive]);

  const stop = async (id: string) => {
    setPending((current) => [...current, id]);
    try { await cancelTask(id); receive(await listTasks()); }
    catch (reason) { setError(`停止请求失败：${reason}`); }
    finally { setPending((current) => current.filter((value) => value !== id)); }
  };
  const clear = async () => {
    if (!confirm("清除已结束的任务历史？不会删除日志、录屏或诊断文件，运行中的任务不受影响。")) return;
    setClearing(true);
    try { await clearTaskHistory(); receive(await listTasks()); }
    catch (reason) { setError(`历史清除失败：${reason}`); }
    finally { setClearing(false); }
  };
  const active = snapshot.tasks.filter(isTaskActive).length;
  const failures = snapshot.tasks.filter((task) => task.status === "failed" || task.status === "interrupted").length;
  const tasks = currentOnly && !deviceId ? [] : visibleTasks(snapshot.tasks, currentOnly ? deviceId : undefined, activeOnly);

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm" className="h-7 text-xs" title={`${active} 个活动任务，${failures} 条失败/中断记录`}>
          <ListTodo className="h-3 w-3 mr-1" />任务 {active > 0 ? `(${active})` : failures > 0 ? "!" : ""}
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-3xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>任务中心</DialogTitle>
          <DialogDescription>任务固定绑定启动时的设备。保留最近 200 条已结束记录；停止录屏不会删除设备文件。</DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap items-center gap-3 text-xs">
          <label><input type="checkbox" checked={currentOnly} onChange={(event) => setCurrentOnly(event.target.checked)} /> 仅当前设备</label>
          <label><input type="checkbox" checked={activeOnly} onChange={(event) => setActiveOnly(event.target.checked)} /> 仅运行中</label>
          <Button size="sm" variant="outline" disabled={clearing || snapshot.shuttingDown} onClick={() => void clear()} className="ml-auto text-xs">清除已结束历史</Button>
        </div>
        {snapshot.shuttingDown && <p role="status" className="text-sm">正在退出并清理任务，请稍候…</p>}
        {(error || snapshot.storageError) && <p role="alert" className="text-sm text-destructive">{error || snapshot.storageError}</p>}
        <div className="overflow-y-auto min-h-0 space-y-3" aria-label="任务列表">
          {tasks.length === 0 && <p className="text-sm text-muted-foreground py-8 text-center">暂无符合条件的任务</p>}
          {tasks.map((task) => (
            <article key={task.id} className="rounded-md border p-3 space-y-2 text-xs">
              <div className="flex items-center gap-2">
                <span className="font-semibold">{KIND[task.kind]}</span>
                <span className={task.status === "failed" || task.status === "interrupted" ? "text-destructive" : "text-muted-foreground"}>{STATUS[task.status]}</span>
                {isTaskActive(task) && <Button className="ml-auto h-6 text-xs" size="sm" variant="outline" disabled={task.status === "cancelling" || pending.includes(task.id)} onClick={() => void stop(task.id)}>停止</Button>}
              </div>
              <div className="break-all text-muted-foreground">设备：{task.deviceId}{task.targetPackage && ` · 应用：${task.targetPackage}`}</div>
              <p className="whitespace-pre-wrap break-all">{task.message}</p>
              {isTaskActive(task) && task.progress !== null && <div className="flex items-center gap-2"><Progress value={task.progress} className="h-1.5" /><span>{task.progress}%</span></div>}
              {!isTaskActive(task) && <p className="break-all">清理结果：{task.cleanup}</p>}
              {task.outputPath && <p className="break-all">文件：{task.outputPath}</p>}
              <div className="text-muted-foreground break-all">{new Date(task.startedAt).toLocaleString()}{task.endedAt && ` → ${new Date(task.endedAt).toLocaleTimeString()}`} · ID {task.id}</div>
            </article>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}

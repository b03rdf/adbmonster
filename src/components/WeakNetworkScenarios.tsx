import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { Activity, ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { WeakNetworkObservation } from "./WeakNetworkObservation";
import {
  deleteWeakNetworkScenario, listWeakNetworkScenarios, runWeakNetworkScenario, saveWeakNetworkScenario,
} from "@/lib/tauri";
import {
  cloneScenario, defaultScenario, normalProfile, phaseFromConfig, scenarioSeconds, validateScenario,
  type NetworkPhase, type NetworkProfile, type NetworkScenario,
} from "@/lib/networkScenario";
import type { WeakNetworkConfig, WeakNetworkStatus } from "@/types/adb";

const fields: { key: keyof NetworkProfile; label: string; max: number; step?: number }[] = [
  { key: "uploadKbps", label: "上行 Kbps", max: 1_000_000 },
  { key: "downloadKbps", label: "下行 Kbps", max: 1_000_000 },
  { key: "latencyMs", label: "延迟 ms", max: 5000 },
  { key: "jitterMs", label: "抖动 ±ms", max: 5000 },
  { key: "lossPercent", label: "UDP 丢包 %", max: 100, step: 0.1 },
  { key: "duplicatePercent", label: "UDP 重复 %", max: 100, step: 0.1 },
  { key: "reorderPercent", label: "UDP 乱序 %", max: 100, step: 0.1 },
];

export function WeakNetworkScenarios({ deviceId, currentConfig, ready, busy, setBusy, onStatus, onStop, active }: {
  deviceId: string;
  currentConfig: WeakNetworkConfig;
  ready: boolean;
  busy: boolean;
  setBusy: (busy: boolean) => void;
  onStatus: (status: WeakNetworkStatus) => void;
  onStop: () => Promise<void>;
  active: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [scenario, setScenario] = useState(() => defaultScenario(currentConfig.targetPackage));
  const [saved, setSaved] = useState<NetworkScenario[]>([]);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    if (open && currentConfig.targetPackage) {
      setScenario(current => current.targetPackage ? current : { ...current, targetPackage: currentConfig.targetPackage });
    }
  }, [open, currentConfig.targetPackage]);
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    listWeakNetworkScenarios().then(items => { if (!disposed) setSaved(items); })
      .catch(reason => { if (!disposed) setError("读取场景失败：" + String(reason)); });
    return () => { disposed = true; };
  }, [open]);

  const load = (next: NetworkScenario) => {
    setScenario(cloneScenario(next)); setError(""); setMessage("配置已载入；点击执行后才会应用到设备。");
  };
  const changePhase = (index: number, changes: Partial<NetworkPhase>) =>
    setScenario(current => ({ ...current, phases: current.phases.map((phase, i) => i === index ? { ...phase, ...changes } : phase) }));
  const move = (index: number, offset: number) => setScenario(current => {
    const phases = [...current.phases];
    [phases[index], phases[index + offset]] = [phases[index + offset], phases[index]];
    return { ...current, phases };
  });
  const saveScenario = async () => {
    const snapshot = cloneScenario({ ...scenario, name: scenario.name.trim() });
    const invalid = validateScenario(snapshot);
    if (invalid) { setError(invalid); return; }
    setSaving(true); setError(""); setMessage("");
    try {
      if (saved.some(item => item.name === snapshot.name) &&
          !await ask("覆盖同名场景“" + snapshot.name + "”？", { title: "保存场景", kind: "warning" })) return;
      setSaved(await saveWeakNetworkScenario(snapshot));
      setSelected(snapshot.name);
      setMessage("场景已保存到本机；重启后可载入相同参数与 seed 重复执行。");
    } catch (reason) { setError("保存失败：" + String(reason)); }
    finally { setSaving(false); }
  };
  const deleteSaved = async () => {
    if (!selected) return;
    const name = selected;
    setSaving(true); setError(""); setMessage("");
    try {
      if (!await ask("删除已保存场景“" + name + "”？编辑器中的配置会保留。", { title: "删除场景", kind: "warning" })) return;
      setSaved(await deleteWeakNetworkScenario(name)); setSelected("");
      setMessage("已删除保存的场景；可用编辑器中的配置重新保存。");
    } catch (reason) { setError("删除失败：" + String(reason)); }
    finally { setSaving(false); }
  };
  const run = async () => {
    const snapshot = cloneScenario(scenario);
    const invalid = validateScenario(snapshot);
    if (invalid) { setError(invalid); return; }
    setBusy(true); setError(""); setMessage("");
    try {
      const status = await runWeakNetworkScenario(deviceId, snapshot);
      onStatus(status);
      setMessage("场景已开始。关闭窗口不影响执行；请在目标应用中产生流量。");
    } catch (reason) { setError("执行失败：" + String(reason)); }
    finally { setBusy(false); }
  };
  const invalid = validateScenario(scenario);
  const editingDisabled = busy || saving;

  return <Dialog open={open} onOpenChange={setOpen}>
    <DialogTrigger asChild><Button variant="outline" size="sm" className="w-full text-xs"><Activity className="h-3.5 w-3.5 mr-1" />场景配置与效果观测</Button></DialogTrigger>
    <DialogContent className="max-w-6xl max-h-[90vh] overflow-y-auto">
      <DialogHeader>
        <DialogTitle>弱网场景与效果观测</DialogTitle>
        <DialogDescription>同一个 VPN 会话分阶段执行，配置可保存在本机。正常阶段仅表示不额外整形，流量仍经过隧道。</DialogDescription>
      </DialogHeader>
      <section className="space-y-3 text-xs">
        <div className="flex flex-wrap gap-2 items-center">
          <select aria-label="已保存的场景" className="h-9 max-w-full rounded border bg-background px-2" value={selected} disabled={editingDisabled}
            onChange={event => { const name = event.target.value; setSelected(name); const item = saved.find(item => item.name === name); if (item) load(item); }}>
            <option value="">选择已保存场景</option>{saved.map(item => <option key={item.name} value={item.name}>{item.name}</option>)}
          </select>
          <Button size="sm" variant="outline" disabled={!selected || editingDisabled} onClick={() => { const item = saved.find(item => item.name === selected); if (item) load(item); }}>重新载入</Button>
          <Button size="sm" variant="outline" onClick={saveScenario} disabled={editingDisabled}>保存配置</Button>
          <Button size="sm" variant="ghost" onClick={deleteSaved} disabled={!selected || editingDisabled}>删除已保存场景</Button>
          <Button size="sm" variant="outline" disabled={editingDisabled} onClick={() => load(defaultScenario(scenario.targetPackage || currentConfig.targetPackage))}>四阶段模板</Button>
          <Button size="sm" variant="outline" disabled={editingDisabled} onClick={() => load({
            schemaVersion: 1, name: "单阶段弱网", targetPackage: currentConfig.targetPackage, seed: scenario.seed, phases: [phaseFromConfig(currentConfig)],
          })}>从侧栏参数创建</Button>
        </div>
        <div className="grid sm:grid-cols-3 gap-3">
          <label className="space-y-1"><span>场景名称</span><Input value={scenario.name} maxLength={160} disabled={editingDisabled} onChange={event => setScenario({ ...scenario, name: event.target.value })} /></label>
          <label className="space-y-1"><span>目标应用包名（随配置保存）</span><Input list="weak-network-packages" value={scenario.targetPackage} placeholder="com.example.game" disabled={editingDisabled} onChange={event => setScenario({ ...scenario, targetPackage: event.target.value.trim() })} /></label>
          <label className="space-y-1"><span>随机种子 seed（重复执行保留）</span><Input type="number" min={0} max={4294967295} step={1} value={scenario.seed} disabled={editingDisabled} onChange={event => setScenario({ ...scenario, seed: Number(event.target.value) })} /></label>
        </div>
        <div className="space-y-2">
          {scenario.phases.map((phase, index) => <fieldset key={index} disabled={editingDisabled} className="rounded border p-3 space-y-2">
            <legend className="px-1 text-muted-foreground">阶段 {index + 1}</legend>
            <div className="flex flex-wrap items-end gap-2">
              <label className="flex-1 min-w-28 space-y-1"><span>阶段名称</span><Input value={phase.name} onChange={event => changePhase(index, { name: event.target.value })} /></label>
              <label className="w-24 space-y-1"><span>持续秒数</span><Input type="number" min={1} max={3600} step={1} value={phase.durationSeconds} onChange={event => changePhase(index, { durationSeconds: Number(event.target.value) })} /></label>
              <label className="flex items-center gap-2 h-9 px-2"><input type="checkbox" checked={phase.offline} onChange={event => changePhase(index, { offline: event.target.checked })} />短时断网</label>
              <Button size="icon" variant="ghost" disabled={editingDisabled || index === 0} title="上移阶段" aria-label="上移阶段" onClick={() => move(index, -1)}><ArrowUp className="h-4 w-4" /></Button>
              <Button size="icon" variant="ghost" disabled={editingDisabled || index === scenario.phases.length - 1} title="下移阶段" aria-label="下移阶段" onClick={() => move(index, 1)}><ArrowDown className="h-4 w-4" /></Button>
              <Button size="icon" variant="ghost" disabled={editingDisabled || scenario.phases.length === 1} title="删除阶段" aria-label="删除阶段" onClick={() => setScenario({ ...scenario, phases: scenario.phases.filter((_, i) => i !== index) })}><Trash2 className="h-4 w-4" /></Button>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 xl:grid-cols-7 gap-2">
              {fields.map(field => <label key={field.key} className="space-y-1 text-muted-foreground"><span>{field.label}</span><Input type="number" min={0} max={field.max} step={field.step ?? 1} value={phase.profile[field.key]} disabled={phase.offline}
                onChange={event => changePhase(index, { profile: { ...phase.profile, [field.key]: Number(event.target.value) } })} /></label>)}
            </div>
            {phase.offline && <p className="text-muted-foreground">此阶段暂停 TCP 转发、丢弃新 UDP；恢复阶段继续使用原会话，应用自身可能超时重连。上面的整形参数在断网阶段不参与新数据转发。</p>}
          </fieldset>)}
        </div>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <Button size="sm" variant="outline" disabled={editingDisabled || scenario.phases.length >= 20} onClick={() => setScenario({ ...scenario, phases: [...scenario.phases, { name: "新阶段", durationSeconds: 10, offline: false, profile: normalProfile() }] })}><Plus className="h-4 w-4 mr-1" />添加阶段</Button>
          <span>合计 {scenarioSeconds(scenario)} 秒 · {scenario.phases.length}/20 阶段</span>
          <div className="flex gap-2">
            <Button size="sm" onClick={run} disabled={!ready || editingDisabled || Boolean(invalid)}>执行此配置{active ? "（替换当前任务）" : ""}</Button>
            <Button size="sm" variant="outline" onClick={onStop} disabled={!active || busy}>停止并恢复</Button>
          </div>
        </div>
        {invalid && <p className="text-amber-600 dark:text-amber-400">{invalid}</p>}
        {!ready && <p className="text-muted-foreground">可以编辑和保存；执行前请在侧栏完成 VPN 助手安装/更新与授权。</p>}
        {error && <p role="alert" className="text-destructive break-all">{error}</p>}
        {message && <p role="status" className="text-primary">{message}</p>}
        <p className="text-muted-foreground leading-relaxed">0 Kbps = 不限速；延迟是单向代理附加延迟，不是 RTT。场景总时长 10～3600 秒，计时从助手启动确认后开始。助手 1.0.3 有额外 90 秒紧急恢复缓冲，正常结束由桌面端定时停止。编辑配置不修改正在运行的快照；固定 seed 不能消除真实流量和调度差异。</p>
      </section>
      <WeakNetworkObservation deviceId={deviceId} visible={open} onReplay={load} />
    </DialogContent>
  </Dialog>;
}

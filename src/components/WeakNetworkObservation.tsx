import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { exportWeakNetworkReport, getWeakNetworkObservation } from "@/lib/tauri";
import {
  configDropPercent, formatMeasured, payloadKbps,
  type DirectionCounters, type NetworkReport, type NetworkScenario,
} from "@/lib/networkScenario";

function Drops({ counts, direction }: { counts: DirectionCounters; direction: string }) {
  return <tr className="border-t [&>td]:p-2">
    <td>{direction}</td><td>{counts.udpReceivedDatagrams}</td><td>{counts.udpPolicyEvaluated}</td>
    <td>{counts.udpConfigDrops} / {formatMeasured(configDropPercent(counts), "%")}</td>
    <td>{counts.udpQueueOverflowDrops}</td><td>{counts.udpOutageDrops}</td>
    <td>{counts.udpPhaseChangeDrops}</td><td>{counts.udpCancelledDatagrams}</td>
    <td>{counts.udpSendErrors}</td><td>{counts.udpForwardedDatagrams} / {counts.udpDuplicateDatagrams}</td>
  </tr>;
}

export function WeakNetworkObservation({ deviceId, visible, onReplay }: {
  deviceId: string; visible: boolean; onReplay: (scenario: NetworkScenario) => void;
}) {
  const [report, setReport] = useState<NetworkReport | null>(null);
  const [pollError, setPollError] = useState("");
  const [exportMessage, setExportMessage] = useState("");
  const [exporting, setExporting] = useState(false);
  useEffect(() => {
    setReport(null);
    setPollError("");
    setExportMessage("");
    if (!visible) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        const next = await getWeakNetworkObservation(deviceId);
        if (!disposed) { setReport(next); setPollError(""); }
      } catch (reason) {
        if (!disposed) setPollError(String(reason));
      } finally {
        if (!disposed) timer = setTimeout(poll, 1000);
      }
    };
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [deviceId, visible]);

  const exportReport = async () => {
    if (!report) return;
    const runId = report.runId;
    setExporting(true);
    setExportMessage("");
    try {
      const outputPath = await save({
        defaultPath: "weak-network-report-" + runId.replace(/[^a-zA-Z0-9_-]/g, "_") + ".json",
        filters: [{ name: "JSON 报告", extensions: ["json"] }],
      });
      if (outputPath) {
        await exportWeakNetworkReport(runId, outputPath);
        setExportMessage("报告已导出：" + outputPath);
      }
    } catch (reason) { setExportMessage("报告导出失败：" + String(reason)); }
    finally { setExporting(false); }
  };

  const lastSample = report?.samples[report.samples.length - 1];
  const counts = report?.measured;
  return <section className="space-y-3 border-t pt-4 text-xs">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <h3 className="text-sm font-medium">效果观测 · 本次执行的不可变配置快照</h3>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={!report} onClick={() => report && onReplay(report.configured)}>
          将本次配置载入编辑器
        </Button>
        <Button size="sm" variant="outline" disabled={!report || exporting} onClick={exportReport}>导出 JSON 报告</Button>
      </div>
    </div>
    {pollError && <p role="alert" className="text-destructive">观测读取失败：{pollError}（下方若有数据则为上次快照）</p>}
    {exportMessage && <p role="status" className="break-all">{exportMessage}</p>}
    {!report || !counts ? <p className="text-muted-foreground">尚无此设备的报告。运行场景或单阶段弱网后，在目标应用中主动产生流量。只自动保留最近一次报告，重要结果请导出。</p> : <>
      <div className="rounded border bg-muted/20 p-3 space-y-1">
        <p>{report.active ? "运行中" : "已结束"} · {report.configured.name} · {(report.elapsedMs / 1000).toFixed(1)} 秒
          {report.active && " · 阶段：" + report.configured.phases[report.currentPhaseIndex]?.name}</p>
        <p className="font-mono break-all">运行 ID：{report.runId} · 包名：{report.configured.targetPackage} · seed：{report.configured.seed}</p>
        <p className="text-muted-foreground">开始：{report.startedAt ? new Date(report.startedAt).toLocaleString() : "尚未开始"} · {report.endReason ?? "停止操作可在本窗口或任务中心进行"}</p>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <div className="rounded border p-2">最近采样上行 / 下行<p className="font-mono mt-1">{formatMeasured(lastSample?.uploadPayloadKbps ?? null)} / {formatMeasured(lastSample?.downloadPayloadKbps ?? null)} Kbps</p><p className="text-muted-foreground">窗口 {lastSample ? lastSample.intervalMs + " ms" : "未采样"}，截至 {lastSample ? (lastSample.elapsedMs / 1000).toFixed(1) + " s" : "—"}</p></div>
        <div className="rounded border p-2">全程平均上行 / 下行<p className="font-mono mt-1">{formatMeasured(payloadKbps(counts.upload, report.elapsedMs))} / {formatMeasured(payloadKbps(counts.download, report.elapsedMs))} Kbps</p><p className="text-muted-foreground">包含空闲和断网时段</p></div>
        <div className="rounded border p-2">TCP 活跃 / 峰值 / 累计<p className="font-mono mt-1">{counts.activeTcpConnections} / {counts.peakTcpConnections} / {counts.totalTcpConnections}</p><p className="text-muted-foreground">建连错误 {counts.connectionErrors}</p></div>
        <div className="rounded border p-2">UDP 关联 活跃 / 峰值 / 累计<p className="font-mono mt-1">{counts.activeUdpAssociations} / {counts.peakUdpAssociations} / {counts.totalUdpAssociations}</p><p className="text-muted-foreground">全局队列 当前 / 峰值：{counts.udpQueueCurrent} / {counts.udpQueuePeak}，上限 256</p></div>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-left text-[11px]">
          <caption className="text-left pb-2 text-muted-foreground">配置值与实际测量值逐阶段对照（速率单位 Kbps；0 配额表示不限速）</caption>
          <thead><tr className="[&>th]:p-2 bg-muted/40"><th>阶段 / 状态</th><th>配置时长 / 实际时长</th><th>配置上 / 下行</th><th>配置延迟 ± 抖动</th><th>配置 UDP 丢 / 重复 / 乱序</th><th>实测平均上 / 下行</th><th>实测配置丢包比例 ↑ / ↓</th><th>实测溢出 ↑ / ↓</th></tr></thead>
          <tbody>{report.configured.phases.map((phase, index) => {
            const result = report.phases.find(item => item.phaseIndex === index);
            const duration = result ? Math.max(0, (result.actualEndMs ?? report.elapsedMs) - result.actualStartMs) : 0;
            const p = phase.profile;
            return <tr key={index} className="border-t [&>td]:p-2">
              <td>{index + 1}. {phase.name}{phase.offline && "（断网）"}<p className="text-muted-foreground">{!result ? "未执行" : result.actualEndMs === null ? "运行中" : "已结束"}</p></td>
              <td>{phase.durationSeconds} s / {result ? (duration / 1000).toFixed(1) + " s" : "—"}</td>
              <td>{p.uploadKbps} / {p.downloadKbps}</td><td>{p.latencyMs} ± {p.jitterMs} ms</td>
              <td>{p.lossPercent}% / {p.duplicatePercent}% / {p.reorderPercent}%</td>
              <td>{result ? formatMeasured(payloadKbps(result.measured.upload, duration)) + " / " + formatMeasured(payloadKbps(result.measured.download, duration)) : "未测量"}</td>
              <td>{result ? formatMeasured(configDropPercent(result.measured.upload), "%") + " / " + formatMeasured(configDropPercent(result.measured.download), "%") : "未测量"}</td>
              <td>{result ? result.measured.upload.udpQueueOverflowDrops + " / " + result.measured.download.udpQueueOverflowDrops : "未测量"}</td>
            </tr>;
          })}</tbody>
        </table>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full whitespace-nowrap text-left text-[11px]">
          <caption className="text-left pb-2 text-muted-foreground">UDP 观测计数：配置丢弃 ÷ 策略样本才是“实测配置丢包比例”；发送数包含成功重复包</caption>
          <thead><tr className="[&>th]:p-2 bg-muted/40"><th>方向</th><th>到达</th><th>策略样本</th><th>配置丢弃 / 比例</th><th>队列溢出</th><th>断网丢弃</th><th>阶段作废</th><th>取消</th><th>发送错误</th><th>发送 / 重复</th></tr></thead>
          <tbody><Drops counts={counts.upload} direction="上行" /><Drops counts={counts.download} direction="下行" /></tbody>
        </table>
      </div>
      <p className="font-mono">成功转发有效载荷（字节）：TCP ↑ {counts.upload.tcpForwardedBytes} / ↓ {counts.download.tcpForwardedBytes}；UDP ↑ {counts.upload.udpForwardedBytes} / ↓ {counts.download.udpForwardedBytes}。完成的限速等待累计 ↑ {counts.upload.shapingWaitMs} / ↓ {counts.download.shapingWaitMs} ms。</p>
      <div className="rounded border border-amber-500/30 bg-amber-500/5 p-3 space-y-2 leading-relaxed">
        <h4 className="font-medium">为什么体验与设置值不同？测量边界与本次证据</h4>
        {report.interpretation.map((note, index) => <p key={index}>{note}</p>)}
      </div>
      <p className="text-muted-foreground">报告不测量 RTT / 真实链路丢包 / 应用唯一有效吞吐，不能将配置延迟当作实测 RTT。JSON 包含完整配置、各阶段计数、约 1 秒吞吐采样及以上解释；不保存网络内容、URL 或认证口令。</p>
    </>}
  </section>;
}

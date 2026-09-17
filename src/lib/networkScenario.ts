import type { WeakNetworkConfig } from "@/types/adb";

export type NetworkProfile = Omit<WeakNetworkConfig, "targetPackage" | "durationSeconds">;
export interface NetworkPhase {
  name: string;
  durationSeconds: number;
  offline: boolean;
  profile: NetworkProfile;
}
export interface NetworkScenario {
  schemaVersion: number;
  name: string;
  targetPackage: string;
  seed: number;
  phases: NetworkPhase[];
}
export interface DirectionCounters {
  tcpReceivedBytes: number;
  tcpForwardedBytes: number;
  udpReceivedBytes: number;
  udpReceivedDatagrams: number;
  udpPolicyEvaluated: number;
  udpForwardedBytes: number;
  udpForwardedDatagrams: number;
  udpConfigDrops: number;
  udpQueueOverflowDrops: number;
  udpOutageDrops: number;
  udpPhaseChangeDrops: number;
  udpCancelledDatagrams: number;
  udpSendErrors: number;
  udpDuplicateDatagrams: number;
  shapingWaitMs: number;
}
export interface NetworkCounters {
  upload: DirectionCounters;
  download: DirectionCounters;
  activeTcpConnections: number;
  totalTcpConnections: number;
  peakTcpConnections: number;
  activeUdpAssociations: number;
  totalUdpAssociations: number;
  peakUdpAssociations: number;
  connectionErrors: number;
  udpQueueCurrent: number;
  udpQueuePeak: number;
}
export interface PhaseResult {
  phaseIndex: number;
  configured: NetworkPhase;
  scheduledStartMs: number;
  actualStartMs: number;
  actualEndMs: number | null;
  measured: NetworkCounters;
}
export interface NetworkSample {
  elapsedMs: number;
  phaseIndex: number;
  intervalMs: number;
  uploadPayloadKbps: number;
  downloadPayloadKbps: number;
  activeTcpConnections: number;
  activeUdpAssociations: number;
  udpQueueCurrent: number;
}
export interface NetworkReport {
  schemaVersion: number;
  runId: string;
  deviceId: string;
  configured: NetworkScenario;
  startedAt: string | null;
  finishedAt: string | null;
  elapsedMs: number;
  active: boolean;
  endReason: string | null;
  currentPhaseIndex: number;
  measured: NetworkCounters;
  phases: PhaseResult[];
  samples: NetworkSample[];
  interpretation: string[];
}

export function normalProfile(): NetworkProfile {
  return { uploadKbps: 0, downloadKbps: 0, latencyMs: 0, jitterMs: 0,
    lossPercent: 0, duplicatePercent: 0, reorderPercent: 0 };
}
export function defaultScenario(targetPackage = ""): NetworkScenario {
  return {
    schemaVersion: 1, name: "正常 → 高延迟 → 短时断网 → 恢复", targetPackage, seed: 1,
    phases: [
      { name: "正常", durationSeconds: 10, offline: false, profile: normalProfile() },
      { name: "高延迟", durationSeconds: 15, offline: false,
        profile: { ...normalProfile(), uploadKbps: 512, downloadKbps: 1024, latencyMs: 500, jitterMs: 50 } },
      { name: "短时断网", durationSeconds: 5, offline: true, profile: normalProfile() },
      { name: "恢复", durationSeconds: 10, offline: false, profile: normalProfile() },
    ],
  };
}
export function phaseFromConfig(config: WeakNetworkConfig): NetworkPhase {
  const { targetPackage: _target, durationSeconds, ...profile } = config;
  return { name: "自定义弱网", durationSeconds, offline: false, profile: { ...profile } };
}
export function cloneScenario(scenario: NetworkScenario): NetworkScenario {
  return { ...scenario, phases: scenario.phases.map(phase => ({ ...phase, profile: { ...phase.profile } })) };
}
export function scenarioSeconds(scenario: NetworkScenario): number {
  return scenario.phases.reduce((sum, phase) => sum + phase.durationSeconds, 0);
}
export function validateScenario(scenario: NetworkScenario): string | null {
  if (scenario.schemaVersion !== 1) return "不支持的场景版本";
  if (!scenario.name.trim() || Array.from(scenario.name).length > 80) return "场景名称须为 1～80 字";
  if (scenario.targetPackage.length > 255 || !/^[a-zA-Z0-9_]+(\.[a-zA-Z0-9_]+)+$/.test(scenario.targetPackage))
    return "请输入有效的目标应用包名";
  if (scenario.targetPackage === "com.rendongfang.adbmonster.vpnhelper") return "不能代理弱网助手自身";
  if (!Number.isInteger(scenario.seed) || scenario.seed < 0 || scenario.seed > 4_294_967_295)
    return "随机种子须为 0～4294967295 的整数";
  if (scenario.phases.length < 1 || scenario.phases.length > 20) return "场景须包含 1～20 个阶段";
  for (const phase of scenario.phases) {
    if (!phase.name.trim() || Array.from(phase.name).length > 80) return "阶段名称须为 1～80 字";
    if (!Number.isInteger(phase.durationSeconds) || phase.durationSeconds < 1 || phase.durationSeconds > 3600)
      return "阶段时长须为 1～3600 秒的整数";
    const p = phase.profile;
    if ([p.uploadKbps, p.downloadKbps].some(value => !Number.isInteger(value) || value < 0 || value > 1_000_000))
      return "带宽须为 0～1,000,000 Kbps 的整数";
    if (![p.latencyMs, p.jitterMs].every(Number.isInteger) || p.latencyMs < 0 || p.latencyMs > 5000 || p.jitterMs < 0 || p.jitterMs > p.latencyMs)
      return "延迟须为 0～5000 ms 的整数，抖动不能大于延迟";
    if ([p.lossPercent, p.duplicatePercent, p.reorderPercent].some(value => !Number.isFinite(value) || value < 0 || value > 100))
      return "丢包、重复和乱序须在 0～100% 之间";
  }
  if (scenarioSeconds(scenario) < 10 || scenarioSeconds(scenario) > 3600) return "场景总时长须为 10～3600 秒";
  return null;
}
export function payloadKbps(counts: DirectionCounters, elapsedMs: number): number | null {
  return elapsedMs > 0 ? (counts.tcpForwardedBytes + counts.udpForwardedBytes) * 8 / elapsedMs : null;
}
export function configDropPercent(counts: DirectionCounters): number | null {
  return counts.udpPolicyEvaluated > 0 ? counts.udpConfigDrops * 100 / counts.udpPolicyEvaluated : null;
}
export function formatMeasured(value: number | null, suffix = ""): string {
  return value === null || !Number.isFinite(value) ? "无样本" : value.toFixed(2) + suffix;
}

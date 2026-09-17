export interface Device {
  id: string;
  model: string;
  androidVersion: string;
  status: string;
  connectionType: "usb" | "emulator" | "network";
}

export interface AutoConnectResult {
  connected: boolean;
  address: string;
  message: string;
}

export interface LogEntry {
  timestamp: string;
  pid: number;
  tid: number;
  level: string;
  tag: string;
  message: string;
}

export interface AppConfig {
  adbPath: string;
  screenshotOutput: string;
  clogOutput: string;
  defaultPackage: string;
}

export interface RemoteFile {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
}

export interface DeviceMetrics {
  batteryLevel: number | null;
  batteryTemperatureC: number | null;
  charging: boolean;
  memoryTotalKb: number;
  memoryAvailableKb: number;
  storageTotalKb: number;
  storageAvailableKb: number;
  cpuUsagePercent: number | null;
  uptimeSeconds: number;
  foregroundActivity: string;
  collectedAt: string;
}

export interface DiagnosticProgress {
  stage: string;
  message: string;
  percent: number;
}

export interface DiagnosticStatus {
  running: boolean;
  progress: DiagnosticProgress | null;
}

export interface RecordingStatus {
  taskId: string | null;
  running: boolean;
  hasRecording: boolean;
  deviceId: string | null;
}

export interface WeakNetworkCapabilities {
  supported: boolean;
  helperInstalled: boolean;
  helperUpdateRequired: boolean;
  vpnAuthorized: boolean;
  helperRunning: boolean;
  helperVersion: string | null;
  activeTargetPackage: string | null;
  expiresAt: string | null;
  message: string;
}

export interface WeakNetworkConfig {
  targetPackage: string;
  uploadKbps: number;
  downloadKbps: number;
  latencyMs: number;
  jitterMs: number;
  lossPercent: number;
  duplicatePercent: number;
  reorderPercent: number;
  durationSeconds: number;
}

export interface WeakNetworkStatus {
  active: boolean;
  deviceId: string | null;
  targetPackage: string | null;
  expiresAt: string | null;
  message: string;
}

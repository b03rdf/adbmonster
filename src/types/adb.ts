export interface Device {
  id: string;
  model: string;
  androidVersion: string;
  status: string;
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

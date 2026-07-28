import { invoke } from "@tauri-apps/api/core";
import type {
  AutoConnectResult,
  Device,
  DeviceMetrics,
  DiagnosticStatus,
  RemoteFile,
  WeakNetworkCapabilities,
  WeakNetworkConfig,
  WeakNetworkStatus,
} from "@/types/adb";

export async function getDevices() {
  return invoke<Device[]>("get_devices");
}

export async function autoConnectLocalEmulator(address: string) {
  return invoke<AutoConnectResult>("auto_connect_local_emulator", { address });
}

export async function connectDevice(ip: string) {
  return invoke<string>("connect_device", { ip });
}

export async function disconnectDevice(ip: string) {
  return invoke<string>("disconnect_device", { ip });
}

export async function getDeviceIp(deviceId: string) {
  return invoke<string>("get_device_ip", { deviceId });
}

export async function tcpipConnect(port: number) {
  return invoke<string>("tcpip_connect", { port });
}

export async function pairDevice(ip: string, port: number, code: string) {
  return invoke<string>("pair_device", { ip, port, code });
}

export async function pairThenConnect(ip: string, pairPort: number, connectPort: number, code: string) {
  return invoke<string>("pair_then_connect", { ip, pairPort, connectPort, code });
}

export async function startScrcpy(deviceId: string) {
  return invoke<string>("start_scrcpy", { deviceId });
}

export async function stopScrcpy() {
  return invoke<void>("stop_scrcpy");
}

export async function isScrcpyRunning() {
  return invoke<boolean>("is_scrcpy_running");
}

export async function startLogcat(deviceId: string, buffer = "main") {
  return invoke<string>("start_logcat", { deviceId, buffer });
}

export async function stopLogcat() {
  return invoke<void>("stop_logcat");
}

export async function isLogcatRunning() {
  return invoke<boolean>("is_logcat_running");
}

export async function exportLogcat(lines: string[], outputPath: string) {
  return invoke<string>("export_logcat", { lines, outputPath });
}

export async function takeScreenshot(deviceId: string, outputPath: string) {
  return invoke<string>("take_screenshot", { deviceId, outputPath });
}

export async function startRecord(deviceId: string) {
  return invoke<string>("start_record", { deviceId });
}

export async function stopRecord(deviceId: string, localPath: string) {
  return invoke<string>("stop_record", { deviceId, localPath });
}

export async function isRecording() {
  return invoke<boolean>("is_recording");
}

export async function pullFile(deviceId: string, remote: string, local: string) {
  return invoke<string>("pull_file", { deviceId, remote, local });
}

export async function pullClog(deviceId: string, packageName: string, localDir: string) {
  return invoke<string>("pull_clog", { deviceId, packageName, localDir });
}

export async function installApk(deviceId: string, apkPath: string) {
  return invoke<string>("install_apk", { deviceId, apkPath });
}

export async function uninstallApk(deviceId: string, packageName: string) {
  return invoke<string>("uninstall_apk", { deviceId, packageName });
}

export async function clearApp(deviceId: string, packageName: string) {
  return invoke<string>("clear_app", { deviceId, packageName });
}

export async function getPackageInfo(deviceId: string, packageName: string) {
  return invoke<string>("get_package_info", { deviceId, packageName });
}

export async function pullApk(deviceId: string, packageName: string, localPath: string) {
  return invoke<string>("pull_apk", { deviceId, packageName, localPath });
}

export async function listPackages(deviceId: string) {
  return invoke<string[]>("list_packages", { deviceId });
}

export async function updateTrayMenu(deviceSummary: string) {
  return invoke<void>("update_tray_menu", { deviceSummary });
}

export async function listRemoteFiles(deviceId: string, path: string) {
  return invoke<RemoteFile[]>("list_remote_files", { deviceId, path });
}

export async function pushFileToRemote(deviceId: string, localPath: string, remotePath: string) {
  return invoke<string>("push_file_to_remote", { deviceId, localPath, remotePath });
}

export async function deleteRemoteFile(deviceId: string, remotePath: string) {
  return invoke<string>("delete_remote_file", { deviceId, remotePath });
}

export async function createRemoteDirectory(deviceId: string, remotePath: string) {
  return invoke<string>("create_remote_directory", { deviceId, remotePath });
}

export async function getDeviceMetrics(deviceId: string) {
  return invoke<DeviceMetrics>("get_device_metrics", { deviceId });
}

export async function createDiagnosticPackage(
  deviceId: string,
  packageName: string | null,
  outputPath: string,
  includeBugreport: boolean,
) {
  return invoke<string>("create_diagnostic_package", {
    deviceId,
    packageName,
    outputPath,
    includeBugreport,
  });
}

export async function getDiagnosticStatus() {
  return invoke<DiagnosticStatus>("get_diagnostic_status");
}

export async function detectWeakNetworkCapabilities(deviceId: string) {
  return invoke<WeakNetworkCapabilities>("detect_weak_network_capabilities", { deviceId });
}

export async function applyWeakNetwork(deviceId: string, config: WeakNetworkConfig) {
  return invoke<WeakNetworkStatus>("apply_weak_network", { deviceId, config });
}

export async function stopWeakNetwork() {
  return invoke<WeakNetworkStatus>("stop_weak_network");
}

export async function forceRestoreWeakNetwork(deviceId: string) {
  return invoke<WeakNetworkStatus>("force_restore_weak_network", { deviceId });
}

export async function getWeakNetworkStatus() {
  return invoke<WeakNetworkStatus>("get_weak_network_status");
}

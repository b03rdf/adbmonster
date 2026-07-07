import { invoke } from "@tauri-apps/api/core";
import type { Device } from "@/types/adb";

export async function getDevices() {
  return invoke<Device[]>("get_devices");
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

export async function startLogcat(deviceId: string, filter?: string) {
  return invoke<string>("start_logcat", { deviceId, filter: filter || null });
}

export async function stopLogcat() {
  return invoke<void>("stop_logcat");
}

export async function isLogcatRunning() {
  return invoke<boolean>("is_logcat_running");
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

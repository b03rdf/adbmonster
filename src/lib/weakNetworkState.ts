import type { WeakNetworkCapabilities, WeakNetworkStatus } from "@/types/adb";

// Session events are authoritative for running state; capability checks own setup/auth state.
export function reconcileWeakNetworkStatus(
  capabilities: WeakNetworkCapabilities | null,
  status: WeakNetworkStatus,
  deviceId: string | undefined,
): WeakNetworkCapabilities | null {
  if (!capabilities || !deviceId) return capabilities;
  const running = status.active && status.deviceId === deviceId;
  return {
    ...capabilities,
    helperRunning: running,
    activeTargetPackage: running ? status.targetPackage : null,
    expiresAt: running ? status.expiresAt : null,
    message: capabilities.supported
      ? running
        ? status.message
        : "非 Root VPN 弱网已就绪；只接管所选应用流量"
      : capabilities.message,
  };
}

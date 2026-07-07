import { create } from "zustand";
import { Device } from "@/types/adb";

interface DeviceState {
  devices: Device[];
  currentDevice: Device | null;
  isRefreshing: boolean;
  setDevices: (devices: Device[]) => void;
  setCurrentDevice: (device: Device | null) => void;
  setIsRefreshing: (isRefreshing: boolean) => void;
}

export const useDeviceStore = create<DeviceState>((set) => ({
  devices: [],
  currentDevice: null,
  isRefreshing: false,
  setDevices: (devices) => set({ devices }),
  setCurrentDevice: (currentDevice) => set({ currentDevice }),
  setIsRefreshing: (isRefreshing) => set({ isRefreshing }),
}));

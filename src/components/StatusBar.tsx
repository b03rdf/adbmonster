import { useAppStore } from "@/stores/appStore";
import { useDeviceStore } from "@/stores/deviceStore";
import { useLogStore } from "@/stores/logStore";
import { useTheme } from "@/hooks/useTheme";
import { Moon, Sun } from "lucide-react";

export function StatusBar() {
  const { statusText } = useAppStore();
  const { currentDevice } = useDeviceStore();
  const { isRunning, lines } = useLogStore();
  const { theme, toggleTheme } = useTheme();

  return (
    <footer className="h-6 flex items-center justify-between px-3 text-[10px] text-muted-foreground bg-muted/50 border-t shrink-0">
      <div className="flex items-center gap-3">
        <span>{statusText}</span>
      </div>
      <div className="flex items-center gap-3">
        {currentDevice && (
          <span>
            设备: {currentDevice.model || currentDevice.id}
          </span>
        )}
        <span>
          日志: {isRunning ? "运行中" : "已停止"} ({lines.length} 条)
        </span>
        <button
          onClick={toggleTheme}
          className="hover:text-foreground transition-colors"
          title={theme === "dark" ? "切换亮色主题" : "切换深色主题"}
        >
          {theme === "dark" ? <Sun className="h-3 w-3" /> : <Moon className="h-3 w-3" />}
        </button>
      </div>
    </footer>
  );
}

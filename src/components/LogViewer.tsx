import { useRef, useEffect, useState, useMemo, useCallback } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useLogcat } from "@/hooks/useLogcat";
import { useDeviceStore } from "@/stores/deviceStore";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import {
  Play, Square, Pause, Trash2, Filter,
  ChevronUp, ChevronDown, Search, X,
} from "lucide-react";

const LEVEL_ORDER = ["V", "D", "I", "W", "E", "F"] as const;
type LogLevel = typeof LEVEL_ORDER[number];

const LEVEL_COLORS: Record<LogLevel, string> = {
  V: "text-gray-400",
  D: "text-blue-500",
  I: "text-green-500",
  W: "text-yellow-500",
  E: "text-red-500",
  F: "text-red-600 font-bold",
};

const LEVEL_BG: Record<LogLevel, string> = {
  V: "bg-gray-500/10",
  D: "bg-blue-500/10",
  I: "bg-green-500/10",
  W: "bg-yellow-500/10",
  E: "bg-red-500/10",
  F: "bg-red-500/20",
};

const LEVEL_TIP: Record<LogLevel, string> = {
  V: "Verbose",
  D: "Debug",
  I: "Info",
  W: "Warning",
  E: "Error",
  F: "Fatal",
};

const TAG_COLORS: Record<string, string> = {
  ActivityManager: "text-orange-400",
  WindowManager: "text-cyan-400",
  SystemServer: "text-purple-400",
  InputDispatcher: "text-amber-400",
  InputReader: "text-amber-300",
  Bluetooth: "text-sky-400",
  Wifi: "text-emerald-400",
  Telephony: "text-pink-400",
  SurfaceFlinger: "text-indigo-400",
  AndroidRuntime: "text-red-500",
  ViewRootImpl: "text-teal-400",
  Choreographer: "text-lime-400",
  dalvikvm: "text-fuchsia-400",
  Zygote: "text-yellow-400",
  PowerManagerService: "text-orange-300",
  BatteryService: "text-green-400",
  AudioFlinger: "text-violet-400",
  MediaPlayer: "text-sky-500",
  Camera: "text-blue-400",
  ConnectivityManager: "text-emerald-300",
  NotificationManager: "text-pink-300",
  PackageManager: "text-indigo-300",
  PackageInstaller: "text-indigo-200",
  NetworkStats: "text-teal-300",
  GraphicsEnvironment: "text-cyan-300",
  Compatibility: "text-orange-200",
  System: "text-gray-400",
  EGL_emulation: "text-violet-300",
  OpenGLRenderer: "text-indigo-300",
};

interface ParsedLog {
  level?: LogLevel;
  tag?: string;
  pid?: string;
  message?: string;
  timestamp?: string;
  isMatch: boolean;
}

function parseLogLine(line: string): ParsedLog {
  let m = line.match(/^([VDIWEF])\/([^(]+)\(\s*(\d+)\):\s?(.*)$/);
  if (m) {
    const level = m[1] as LogLevel;
    return {
      level,
      tag: m[2].trim(),
      pid: m[3],
      message: m[4],
      isMatch: true,
    };
  }

  m = line.match(/^(\S+\s+\S+)\s+(\d+)\s+(\d+)\s+([VDIWEF])\s+([^:]+):\s?(.*)$/);
  if (m) {
    return {
      timestamp: m[1],
      pid: m[2],
      level: m[4] as LogLevel,
      tag: m[5],
      message: m[6],
      isMatch: true,
    };
  }

  return { isMatch: false };
}

function getTagColor(tag: string): string {
  if (TAG_COLORS[tag]) return TAG_COLORS[tag];
  let hash = 0;
  for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash + tag.charCodeAt(i)) | 0;
  const hue = Math.abs(hash) % 360;
  return `text-[hsl(${hue},60%,45%)]`;
}

function highlightText(text: string, query: string): React.ReactNode {
  if (!query) return text;
  const lower = text.toLowerCase();
  const q = query.toLowerCase();
  const parts: React.ReactNode[] = [];
  let idx = 0;

  while (idx < text.length) {
    const pos = lower.indexOf(q, idx);
    if (pos === -1) {
      parts.push(text.slice(idx));
      break;
    }
    if (pos > idx) parts.push(text.slice(idx, pos));
    parts.push(
      <mark key={pos} className="bg-yellow-300/40 dark:bg-yellow-500/30 rounded px-0.5">
        {text.slice(pos, pos + q.length)}
      </mark>,
    );
    idx = pos + q.length;
  }

  return parts.length > 1 ? parts : text;
}

export function LogViewer() {
  const { lines, isRunning, isPaused, filterText, start, stop, togglePause, clearLogs, setFilterText } =
    useLogcat();
  const currentDevice = useDeviceStore((s) => s.currentDevice);
  const virtuosoRef = useRef<VirtuosoHandle>(null);

  const [activeLevels, setActiveLevels] = useState<Set<LogLevel>>(
    new Set(LEVEL_ORDER),
  );
  const [searchText, setSearchText] = useState("");
  const [showSearch, setShowSearch] = useState(false);
  const [currentMatchIdx, setCurrentMatchIdx] = useState(0);
  const [autoScroll, setAutoScroll] = useState(true);

  const toggleLevel = (level: LogLevel) => {
    setActiveLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) next.delete(level);
      else next.add(level);
      return next;
    });
  };

  const filteredLines = useMemo(() => {
    let result = lines;

    if (activeLevels.size < 6) {
      result = result.filter((line) => {
        const p = parseLogLine(line);
        return p.level ? activeLevels.has(p.level) : true;
      });
    }

    if (filterText) {
      const q = filterText.toLowerCase();
      result = result.filter((l) => l.toLowerCase().includes(q));
    }

    return result;
  }, [lines, activeLevels, filterText]);

  const searchMatchIndices = useMemo(() => {
    if (!searchText) return [];
    const q = searchText.toLowerCase();
    const indices: number[] = [];
    for (let i = 0; i < filteredLines.length; i++) {
      if (filteredLines[i].toLowerCase().includes(q)) indices.push(i);
    }
    return indices;
  }, [filteredLines, searchText]);

  const safeMatchIdx = Math.min(currentMatchIdx, Math.max(searchMatchIndices.length - 1, 0));

  const scrollToSearch = useCallback(
    (dir: "next" | "prev") => {
      if (searchMatchIndices.length === 0) return;
      const step = dir === "next" ? 1 : -1;
      const next = (safeMatchIdx + step + searchMatchIndices.length) % searchMatchIndices.length;
      setCurrentMatchIdx(next);
      virtuosoRef.current?.scrollToIndex({
        index: searchMatchIndices[next],
        behavior: "smooth",
        align: "center",
      });
    },
    [searchMatchIndices, safeMatchIdx],
  );

  useEffect(() => {
    if (showSearch) {
      const input = document.getElementById("log-search-input");
      input?.focus();
    }
  }, [showSearch]);

  const prevLenRef = useRef(0);
  useEffect(() => {
    if (!autoScroll || filteredLines.length === 0 || filteredLines.length <= prevLenRef.current) {
      prevLenRef.current = filteredLines.length;
      return;
    }
    prevLenRef.current = filteredLines.length;
    const raf = requestAnimationFrame(() => {
      virtuosoRef.current?.scrollToIndex({
        index: filteredLines.length - 1,
        behavior: "smooth",
      });
    });
    return () => cancelAnimationFrame(raf);
  }, [filteredLines.length, autoScroll]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-1.5 p-1.5 bg-muted/30 border-b flex-wrap">
        <Button
          variant={isRunning ? "default" : "outline"}
          size="sm"
          onClick={isRunning ? stop : start}
          disabled={!currentDevice}
          title={isRunning ? "停止日志" : "开始日志"}
          className="h-7 w-7 p-0 shrink-0"
        >
          {isRunning ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={togglePause}
          disabled={!isRunning}
          title={isPaused ? "继续" : "暂停"}
          className="h-7 w-7 p-0 shrink-0"
        >
          <Pause className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={clearLogs}
          title="清除日志"
          className="h-7 w-7 p-0 shrink-0"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
        <Separator orientation="vertical" className="h-5" />

        {LEVEL_ORDER.map((level) => {
          const active = activeLevels.has(level);
          return (
            <button
              key={level}
              onClick={() => toggleLevel(level)}
              title={`${LEVEL_TIP[level]} — ${active ? "隐藏" : "显示"}`}
              className={`h-6 w-6 rounded text-[11px] font-mono font-bold leading-none transition-all shrink-0
                ${active
                  ? `${LEVEL_COLORS[level]} ${LEVEL_BG[level]} ring-1 ring-inset ring-border`
                  : "text-muted-foreground/40 bg-transparent"
                }
                hover:opacity-80`}
            >
              {level}
            </button>
          );
        })}

        <Separator orientation="vertical" className="h-5" />
        <Filter className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
        <Input
          className="h-7 w-36 text-xs"
          placeholder="过滤文本..."
          value={filterText}
          onChange={(e) => setFilterText(e.target.value)}
        />

        <button
          onClick={() => setAutoScroll(!autoScroll)}
          title={autoScroll ? "自动滚动已开启" : "自动滚动已关闭"}
          className={`h-6 text-[10px] px-1.5 rounded shrink-0 font-medium transition-colors
            ${autoScroll
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground/50 hover:text-muted-foreground"
            }`}
        >
          滚动
        </button>

        <div className="flex-1" />

        <button
          onClick={() => setShowSearch(!showSearch)}
          title="搜索"
          className={`h-7 w-7 rounded flex items-center justify-center shrink-0
            ${showSearch ? "bg-accent" : "hover:bg-accent/50"}`}
        >
          <Search className="h-3.5 w-3.5" />
        </button>

        <span className="text-[10px] text-muted-foreground whitespace-nowrap">
          {filteredLines.length} 条
          {isPaused && " (暂停)"}
        </span>
      </div>

      {showSearch && (
        <div className="flex items-center gap-1.5 px-2 py-1 bg-muted/20 border-b">
          <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
          <Input
            id="log-search-input"
            className="h-7 text-xs flex-1"
            placeholder="搜索日志..."
            value={searchText}
            onChange={(e) => {
              setSearchText(e.target.value);
              setCurrentMatchIdx(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.shiftKey ? scrollToSearch("prev") : scrollToSearch("next");
              }
            }}
          />
          <span className="text-[10px] text-muted-foreground whitespace-nowrap min-w-[5rem] text-right">
            {searchMatchIndices.length > 0
              ? `${safeMatchIdx + 1} / ${searchMatchIndices.length}`
              : searchText
                ? "0 个匹配"
                : ""}
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 shrink-0"
            disabled={searchMatchIndices.length === 0}
            onClick={() => scrollToSearch("prev")}
            title="上一个匹配 (Shift+Enter)"
          >
            <ChevronUp className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 shrink-0"
            disabled={searchMatchIndices.length === 0}
            onClick={() => scrollToSearch("next")}
            title="下一个匹配 (Enter)"
          >
            <ChevronDown className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 shrink-0"
            onClick={() => {
              setShowSearch(false);
              setSearchText("");
            }}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      <div className="flex-1 overflow-hidden">
        {filteredLines.length === 0 ? (
          <div className="flex items-center justify-center h-full text-sm text-muted-foreground px-4 text-center">
            {isRunning
              ? "等待日志输出..."
              : "点击 ▶ 开始日志"}
          </div>
        ) : (
          <Virtuoso
            ref={virtuosoRef}
            className="h-full"
            data={filteredLines}
            followOutput={false}
            itemContent={(index, line) => {
              const parsed = parseLogLine(line);
              const isCurrentMatch = searchMatchIndices[safeMatchIdx] === index;

              return (
                <div
                  className={`px-3 py-0.5 text-xs font-mono border-b border-border/20
                    ${isCurrentMatch ? "bg-yellow-200/20 dark:bg-yellow-400/10" : ""}
                    ${!parsed.level && index % 2 === 0 ? "bg-background" : ""}
                    ${!parsed.level && index % 2 === 1 ? "bg-muted/15" : ""}
                    ${parsed.level && (index % 2 === 0) ? LEVEL_BG[parsed.level]?.replace("/10", "/05") : ""}
                    ${parsed.level && (index % 2 === 1) ? LEVEL_BG[parsed.level]?.replace("/10", "/08") : ""}
                    hover:bg-accent/30`}
                  id={`log-line-${index}`}
                >
                  {parsed.isMatch && parsed.level ? (
                    <span className="flex gap-1.5 items-baseline">
                      <span className={`${LEVEL_COLORS[parsed.level]} shrink-0 w-[1.1rem] text-center`}>
                        {parsed.level}
                      </span>
                      {parsed.pid && (
                        <span className="text-[10px] text-muted-foreground/60 shrink-0">
                          {parsed.pid}
                        </span>
                      )}
                      {parsed.tag && (
                        <span className={`${getTagColor(parsed.tag)} shrink-0`}>
                          {parsed.tag}:
                        </span>
                      )}
                      <span className="break-all">
                        {searchText && parsed.message
                          ? highlightText(parsed.message, searchText)
                          : parsed.message || line}
                      </span>
                    </span>
                  ) : (
                    <span>
                      {searchText ? highlightText(line, searchText) : line}
                    </span>
                  )}
                </div>
              );
            }}
          />
        )}
      </div>
    </div>
  );
}

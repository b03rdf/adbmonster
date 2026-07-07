# ADB Monster (adb缝合怪)

一站式 Android 调试桌面工具。整合 adb 常用功能：设备管理、实时 logcat、截图录屏、APK 管理、scrcpy 投屏等。

## 功能

- **设备管理** — USB/无线连接、QR 码配对、设备列表自动刷新
- **实时 Logcat** — 虚拟滚动渲染、级别/文本过滤、搜索跳转、自动滚屏、标签配色
- **截图 & 录屏** — 一键截图保存、录屏录制并拉取到本地
- **APK 管理** — 安装/卸载、清除数据、查看包信息、提取 APK
- **CLog 导出** — 从设备拉取指定应用的 CLog 目录
- **Scrcpy 投屏** — 一键启动/停止 scrcpy 实时屏幕投影
- **系统托盘** — 关闭窗口最小化到托盘，后台运行

## 技术栈

| 层 | 技术 |
|---|------|
| 桌面框架 | Tauri 2.x |
| 后端 | Rust |
| 前端 | React 19 + TypeScript |
| 构建 | Vite 7 |
| 样式 | Tailwind CSS 4 + shadcn/ui |
| 状态管理 | Zustand |
| 虚拟滚动 | react-virtuoso |

## 前置要求

- [Node.js](https://nodejs.org/) >= 20.19
- [Rust](https://www.rust-lang.org/) nightly toolchain
- [Android 调试桥 (adb)](https://developer.android.com/tools/adb) — 自动搜索 PATH 和内置 bundled 目录
- [scrcpy](https://github.com/Genymobile/scrcpy)（可选，用于投屏功能）

## 开发

```bash
# 安装前端依赖
npm install

# 启动开发模式（前端热更新 + Rust 后端）
npm run tauri dev
```

## 构建

```bash
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。

## 许可

MIT

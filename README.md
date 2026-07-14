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
- [Rust](https://www.rust-lang.org/) stable toolchain
- [Android 调试桥 (adb)](https://developer.android.com/tools/adb) — 开发时优先使用 PATH，发布包会携带 `scrcpy/adb.exe`
- [scrcpy](https://github.com/Genymobile/scrcpy) — 当前仓库已在 `src-tauri/scrcpy/` 中内置 Windows 版本

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

## 质量检查

```bash
# 前端类型检查与生产构建，以及 Rust 格式、Clippy 和测试
npm run check
```

Vite 7 要求 Node.js 20.19+ 或 22.12+，推荐使用当前 Node.js LTS。项目当前主要面向 Windows，打包资源中包含 Windows 版 ADB 和 scrcpy。

## 使用说明

- 无线连接前，需要在 Android 设备上开启开发者选项和无线调试；不同 Android 版本的配对端口与连接端口可能不同。
- 单次录屏使用 Android `screenrecord`，最长 180 秒。
- 关闭主窗口后程序会最小化到系统托盘；请从托盘菜单选择“退出”以完全结束程序。
- 删除远程文件前会二次确认，后端同时拒绝删除 `/`、`/sdcard`、`/storage` 和 `/data` 等保护路径。

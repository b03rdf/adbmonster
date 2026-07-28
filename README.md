# ADB Monster (adb缝合怪)

一站式 Android 调试桌面工具。整合 adb 常用功能：设备管理、实时 logcat、截图录屏、APK 管理、scrcpy 投屏等。

## 功能

- **设备管理** — USB/无线连接、QR 码配对、设备列表自动刷新
- **自动连接** — USB设备自动选中；无可用设备时检测并连接本地模拟器，默认 `127.0.0.1:7555`
- **实时 Logcat** — 虚拟滚动渲染、级别/文本过滤、搜索跳转、自动滚屏、标签配色
- **截图 & 录屏** — 一键截图保存、录屏录制并拉取到本地
- **APK 管理** — 安装/卸载、清除数据、查看包信息、提取 APK
- **CLog 导出** — 从设备拉取指定应用的 CLog 目录
- **Scrcpy 投屏** — 一键启动/停止 scrcpy 实时屏幕投影
- **系统托盘** — 关闭窗口最小化到托盘，后台运行
- **设备实时仪表盘** — 查看 CPU、内存、存储、电量、温度、运行时间和前台 Activity
- **一键诊断包** — 导出设备信息、日志、截图、应用性能数据和可选完整 Bugreport ZIP
- **增强 Logcat** — 多缓冲区、正则过滤、崩溃/ANR 统计、暂停缓存和筛选结果导出
- **手游弱网测试** — 在“工具”标签页调节上下行带宽、延迟、抖动、丢包、重复包和乱序，支持预设、定时恢复与强制恢复

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
- 自动连接默认开启并优先使用USB设备；可通过设备选择器右侧的设置按钮修改本地模拟器端口或关闭自动连接。
- 单次录屏使用 Android `screenrecord`，最长 180 秒。
- 关闭主窗口后程序会最小化到系统托盘；请从托盘菜单选择“退出”以完全结束程序。
- 删除远程文件前会二次确认，后端同时拒绝删除 `/`、`/sdcard`、`/storage` 和 `/data` 等保护路径。

### 弱网测试

1. 连接设备后进入右侧的“工具”标签页，弱网面板会自动检测设备能力。
2. 选择“轻度弱网”“3G 网络”或“极差网络”，也可以直接修改参数形成自定义配置。
3. 点击“应用弱网”开始测试；倒计时结束后会自动恢复，也可以随时点击“停止并恢复”。
4. 如果应用异常退出或状态丢失，重新打开工具后点击“强制恢复当前设备网络”清理残留规则。

支持范围：

- 官方 Android Emulator：支持上下行带宽、延迟和抖动；模拟器控制台本身不支持丢包、重复包与乱序。
- 带 Root 且包含 `tc/netem` 的设备或第三方模拟器：支持出口（上行）限速、延迟、抖动、丢包、重复包与乱序。
- 非 Root 真机：当前无法直接修改系统流量，需要后续配套 VPN 辅助应用；界面会明确显示“不支持”，不会执行系统网络修改。

弱网可能导致无线 ADB 连接中断，测试时优先使用 USB 调试。单次测试时长限制为 10～3600 秒，避免忘记恢复网络。

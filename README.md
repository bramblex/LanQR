# LanQR / 邻享码

LanQR is a Windows desktop utility for sharing a file or folder on the local network with a QR code.
LanQR 是一个 Windows 桌面工具，用来把单个文件或文件夹通过局域网分享出去，并生成可扫码访问的二维码。

The current build ships with a built-in static file server, bilingual UI, and Explorer context-menu integration.
当前版本内置静态文件服务、支持中英文界面，并集成资源管理器右键菜单。

## Features / 功能

- Single-file sharing with a direct download link
- 单文件分享，打开链接后直接下载

- Single-folder sharing with a lightweight directory listing
- 单文件夹分享，打开链接后进入轻量目录页

- QR code, URL, IP, port, status, and target details in the GUI
- GUI 中展示二维码、链接、IP、端口、状态和目标信息

- Automatic language choice with a manual language switcher
- 自动选择中英文，并提供手动语言切换选项

- User-level Explorer context menu install/uninstall
- 支持当前用户级资源管理器右键菜单安装/卸载

- Multiple independent instances can run at the same time
- 支持多个独立实例同时运行，互不干扰

## Stack / 技术方案

- Rust
- `eframe` / `egui`
- `axum` + `tower-http::ServeFile`
- QR generation inside the app
- Windows registry verbs under `HKCU\Software\Classes`

## Language / 语言

LanQR detects the system locale and chooses Chinese when the locale starts with `zh`; otherwise it uses English.
LanQR 会检测系统语言；当系统 locale 以 `zh` 开头时默认显示中文，否则默认显示英文。

You can also switch the app language manually from the in-app language selector.
你也可以在程序内通过语言选择框手动切换显示语言。

When you install the context menu, the menu text is written using the language that is active at install time.
安装右键菜单时，菜单文本会使用安装当下生效的界面语言。

## Runtime Behavior / 运行行为

- File share URL: `http://<ip>:<port>/send/<token>`
- 文件分享 URL：`http://<ip>:<port>/send/<token>`

- Folder share URL: `http://<ip>:<port>/send/<token>/`
- 文件夹分享 URL：`http://<ip>:<port>/send/<token>/`

- Files are served with `Content-Disposition`, so the downloaded filename stays correct
- 文件响应会带 `Content-Disposition`，下载文件名会保持为原始文件名

- Range requests are handled by the server stack, so mobile downloads are more stable
- 服务端支持 `Range` 请求，手机下载更稳定

## Development / 开发运行

```powershell
cargo run -- "D:\test\file.zip"
cargo run -- "D:\test\folder"
cargo run
```

Without arguments, LanQR opens the idle page.
不带参数时，LanQR 会进入空闲页。

## Packaging / 打包

```powershell
.\build.ps1
```

The script does the following:
脚本会自动完成以下动作：

- Build `LanQR.exe` in release mode
- 编译 release 版 `LanQR.exe`

- Copy `LanQR.exe`, `LanQR.ico`, and `README.md` into `dist/LanQR/`
- 把 `LanQR.exe`、`LanQR.ico` 和 `README.md` 复制到 `dist/LanQR/`

- Create `dist/LanQR-<tag-or-version>-<git-hash>.zip`
- 生成 `dist/LanQR-<tag或版本>-<git哈希>.zip`

Release output layout:
发布产物目录：

```text
dist/
  LanQR/
    LanQR.exe
    LanQR.ico
    README.md
  LanQR-<tag-or-version>-<git-hash>.zip
```

## Usage / 使用方式

### Launch with a path / 直接传路径启动

```powershell
LanQR.exe "D:\test\file.zip"
LanQR.exe "D:\test\folder"
```

### Install the context menu / 安装右键菜单

```powershell
LanQR.exe --install-context-menu
LanQR.exe --uninstall-context-menu
```

You can also install or uninstall it from the idle page in the GUI.
也可以在 GUI 空闲页中点击按钮安装或卸载。

Registry keys used by the first version:
第一版写入的注册表位置：

- `HKCU\Software\Classes\*\shell\LanQR`
- `HKCU\Software\Classes\Directory\shell\LanQR`

No administrator permission is required.
不需要管理员权限。

## Limits / 已知限制

- One file or one folder per share session
- 每次只分享一个文件或一个文件夹

- No multi-select support yet
- 暂不支持多选

- Devices must be on the same local network
- 手机和电脑需要位于同一局域网

- Windows Firewall still needs to allow access to the chosen port
- Windows 防火墙仍需允许对应端口访问

- The directory page is intentionally simple: no preview, upload, or search
- 目录页刻意保持轻量，不提供预览、上传或搜索

## Logs / 日志

```text
%LOCALAPPDATA%\LanQR\logs
```

Logs include launch arguments, selected target, IP, port, URL, share-service lifecycle, and errors.
日志会记录启动参数、目标路径、IP、端口、URL、共享服务生命周期和异常信息。

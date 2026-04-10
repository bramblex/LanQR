# LanQR / 邻享码

LanQR 是一个偏工程落地的 Windows 桌面工具：在资源管理器里右键文件或文件夹，启动一个本地 GUI 窗口，直接在程序内启动局域网静态分享服务，并展示可扫码访问的二维码。

第一版目标是稳定、清晰、易打包，不做复杂架构，不做 Web UI，不做 shell extension。

## 功能

- 支持单个文件分享
- 支持单个文件夹分享
- GUI 展示二维码、下载链接、对象名称、本机 IP、端口、状态
- 支持复制链接、重新生成、关闭分享
- 关闭窗口时停止对应分享服务
- 支持当前用户级资源管理器右键菜单安装/卸载
- 支持多个独立实例同时运行，互不干扰

## 技术方案

- 语言：Rust
- GUI：`eframe` / `egui`
- 分享服务：`axum` + `tower-http::ServeFile`
- 二维码：程序内根据最终 URL 自行生成
- 右键菜单：当前用户级注册表 `HKCU\Software\Classes`

## 为什么自己生成二维码

LanQR 直接根据最终 URL 生成二维码，原因很直接：

- GUI 需要稳定可控的二维码图像
- 程序自己掌握 IP、端口、路由路径，直接生成二维码更清晰
- 这样可以把 URL、二维码和 GUI 状态保持一致

## 分享行为

- 分享单个文件时，二维码对应的链接会直接下载该文件
- 分享文件夹时，二维码对应的链接会打开一个简单的目录页，可继续浏览和下载子文件
- 目录和文件都通过随机路由片段隔离，默认 URL 形如 `http://<ip>:<port>/send/<token>` 或 `http://<ip>:<port>/send/<token>/`

## 为什么第一版使用注册表 verb

第一版右键菜单只用注册表 verb，不做复杂 shell extension，原因是：

- 当前用户级即可安装，不依赖管理员权限
- 实现简单，维护成本低
- 更适合 MVP，易验证、易打包、易回滚

## 为什么选择 Rust + egui

- Rust 适合做独立 Windows 工具，单文件可执行产物清晰
- `egui` 集成简单，足够完成工具型桌面 UI
- 不需要浏览器壳，也不需要 Electron/Tauri/Flutter 这类额外运行时

## 开发运行

```powershell
cargo run -- "D:\test\file.zip"
cargo run -- "D:\test\folder"
cargo run
```

无参数时会进入空闲页，不会崩溃。

## 打包

```powershell
.\build.ps1
```

脚本会自动完成这些动作：

- 执行 `cargo build --release`
- 把 `LanQR.exe` 和 `README.md` 复制到顶层 `dist/LanQR/`

最终发布目录为：

```text
dist/
  LanQR/
    LanQR.exe
    README.md
```

## 使用方式

### 直接传路径启动

```powershell
LanQR.exe "D:\test\file.zip"
LanQR.exe "D:\test\folder"
```

### 安装右键菜单

支持命令行：

```powershell
LanQR.exe --install-context-menu
LanQR.exe --uninstall-context-menu
```

也支持无参数启动后在 GUI 空闲页点击安装/卸载按钮。

安装后，文件和文件夹右键菜单都会出现：

```text
生成局域网二维码
```

点击后会执行：

```text
LanQR.exe "%1"
```

## 右键菜单注册位置

LanQR 第一版写入：

- `HKCU\Software\Classes\*\shell\LanQR`
- `HKCU\Software\Classes\Directory\shell\LanQR`

不会强制请求管理员权限。

## 已知限制

- 第一版仅支持单个文件或单个文件夹
- 不支持多选
- 需要手机和电脑处于同一局域网
- 需要 Windows 防火墙允许访问对应端口
- 当前目录页比较简洁，不提供搜索、预览或上传能力
- 大文件和大量并发请求仍然属于轻量级场景，不是高吞吐文件服务

## 日志

日志目录：

```text
%LOCALAPPDATA%\LanQR\logs
```

日志会记录启动参数、目标路径、选中的 IP、端口、URL、分享服务启动/停止状态和异常信息。

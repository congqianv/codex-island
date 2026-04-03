# Codex Island

[![English](https://img.shields.io/badge/English-README-blue)](./README_EN.md) [![macOS](https://img.shields.io/badge/platform-macOS-111111?logo=apple&logoColor=white)](https://www.apple.com/macos/) [![React](https://img.shields.io/badge/React-19-149ECA?logo=react&logoColor=white)](https://react.dev/) [![Rust](https://img.shields.io/badge/Rust-core-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/) [![Swift](https://img.shields.io/badge/Swift-host-F05138?logo=swift&logoColor=white)](https://www.swift.org/)

![Codex Island Icon](./src-tauri/icons/icon.png)

Codex Island 是一个面向 macOS 的 Codex 会话浮岛。它会停靠在屏幕顶部中央，用一个紧凑的系统级入口显示 Codex 当前是 `Idle`、`Working`、`Needs Attention` 还是 `Completed`。

## 当前状态

这是一个持续迭代中的 macOS 项目，目前主要服务于：

- 并行运行多个 Codex CLI / desktop 会话的开发者
- 想快速看到“是否需要我接手”的用户
- 需要一个更接近系统浮层体验的 Codex 状态入口

## 核心能力

- 自动发现本地活跃的 Codex 会话
- 在未展开态显示清晰的任务态：
  - `Idle`
  - `Working`
  - `Needs Attention`
  - `Completed`
- 在需要你处理时高亮提醒
- 展开后查看会话列表和待处理提示
- 点击“处理”后尽力跳转回对应应用或终端
- 支持 `Terminal.app`、`iTerm2`、`VS Code`、Codex app 等宿主识别

## 推荐运行方式

仓库里现在有两条桌面宿主路线：

1. `macos-host`：Swift `NSPanel` 宿主
2. `src-tauri`：Tauri 宿主

当前更推荐 macOS 使用 `macos-host`，原因是它基于 `NSPanel`，更适合做全屏顶部悬浮这类系统浮层能力。

## 项目结构

```text
src/            React UI
src-core/       共享 Rust 核心：发现、状态、聚焦、通知
native-bridge/  Rust bridge，给原生宿主调用
macos-host/     Swift NSPanel 宿主
src-tauri/      Tauri 壳层
scripts/        打包和辅助脚本
```

## 环境要求

- macOS
- Node.js
- `pnpm`
- Rust toolchain
- Xcode Command Line Tools

## 安装依赖

```bash
pnpm install
```

## 开发

### 浏览器模式

```bash
pnpm dev
```

适合只调 React UI。浏览器模式主要使用 mock 数据。

### 原生宿主开发模式

```bash
pnpm native:host
```

这会：

- 构建 `native-bridge`
- 启动 Swift 写的 `CodexIslandHostApp`

### Tauri 开发模式

```bash
pnpm tauri dev
```

这条仍然可用，但当前不作为 macOS 全屏顶部浮层的首选分发形态。

## 打包

### 推荐：打包 Swift host 版 macOS app

```bash
pnpm build:host-app
```

产物路径：

- [macos-host/build/Codex Island.app](/Users/cong/Desktop/AI相关/codex-island/macos-host/build/Codex%20Island.app)

这版 `.app` 会把这些内容一起打进 bundle：

- 前端 `dist`
- `native-bridge` release 可执行文件
- Swift `NSPanel` 宿主程序

### Tauri 打包

```bash
pnpm tauri build
```

产物路径通常在：

- [src-tauri/target/release/bundle/macos/Codex Island.app](/Users/cong/Desktop/AI相关/codex-island/src-tauri/target/release/bundle/macos/Codex%20Island.app)

说明：

- 当前 Tauri 版可正常构建
- 但在“全屏页面顶部浮层”这个目标上，当前更推荐 Swift host 版

更完整的发布说明见：

- [docs/RELEASE.md](/Users/cong/Desktop/AI相关/codex-island/docs/RELEASE.md)
- [docs/RELEASE_EN.md](/Users/cong/Desktop/AI相关/codex-island/docs/RELEASE_EN.md)

## 验证命令

```bash
pnpm test
pnpm build
cargo test --manifest-path src-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

如果本机 Swift/SDK 环境允许，也可以补充：

```bash
swift build --package-path macos-host -c release
```

## 当前限制

- 仅支持 macOS
- 会话识别和等待输入判断仍有启发式成分
- 终端聚焦属于 best-effort，不保证每种宿主都完全一致
- 不同运行形态下的全屏浮层行为仍可能存在系统差异
- 打包 Swift host 版时依赖本机 Swift / SDK 环境正常

## 已知方向

- 更准确的会话与线程匹配
- 更稳定的提醒判断
- 更可靠的应用聚焦和深链回跳
- 更完整的 macOS 发布流程，例如签名、`dmg`、自动更新

## 贡献

欢迎提 issue 和 PR。

- 如果你发现提醒误判、会话丢失、跳转错误，尽量附带复现步骤
- 如果你改动宿主层，请注意区分 `macos-host` 和 `src-tauri` 两条路线
- 提交前建议至少运行一轮验证命令

## 许可证

当前项目使用 [GNU GPL v3.0](/Users/cong/Desktop/AI相关/codex-island/LICENSE)。

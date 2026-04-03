# Codex Island

[![中文](https://img.shields.io/badge/%E4%B8%AD%E6%96%87-README-blue)](./README.md) [![macOS](https://img.shields.io/badge/platform-macOS-111111?logo=apple&logoColor=white)](https://www.apple.com/macos/) [![React](https://img.shields.io/badge/React-19-149ECA?logo=react&logoColor=white)](https://react.dev/) [![Rust](https://img.shields.io/badge/Rust-core-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/) [![Swift](https://img.shields.io/badge/Swift-host-F05138?logo=swift&logoColor=white)](https://www.swift.org/)

![Codex Island Icon](./src-tauri/icons/icon.png)

Codex Island is a macOS floating island for Codex sessions. It stays at the top-center of the screen and gives you a compact system-level surface for `Idle`, `Working`, `Needs Attention`, and `Completed`.

## Current Status

This is an actively iterating macOS project aimed at:

- developers running multiple Codex CLI / desktop sessions in parallel
- users who want to quickly see whether Codex needs them
- workflows that benefit from a system-level Codex status surface

## Core Capabilities

- Detects active local Codex sessions automatically
- Shows a compact task-state model in the collapsed island:
  - `Idle`
  - `Working`
  - `Needs Attention`
  - `Completed`
- Highlights sessions that require your attention
- Expands into a session list and detail view
- Lets you click `Handle` to jump back to the matching app or terminal
- Recognizes hosts such as `Terminal.app`, `iTerm2`, `VS Code`, and the Codex app

## Recommended Runtime

The repository currently contains two desktop-host routes:

1. `macos-host`: Swift `NSPanel` host
2. `src-tauri`: Tauri host

For macOS, the recommended route right now is `macos-host`, because the `NSPanel` host is a better fit for fullscreen top-overlay behavior.

## Project Structure

```text
src/            React UI
src-core/       Shared Rust core for discovery, state, focus, notifications
native-bridge/  Rust bridge used by the native host
macos-host/     Swift NSPanel host
src-tauri/      Tauri shell
scripts/        Packaging and helper scripts
```

## Requirements

- macOS
- Node.js
- `pnpm`
- Rust toolchain
- Xcode Command Line Tools

## Install Dependencies

```bash
pnpm install
```

## Development

### Browser UI Mode

```bash
pnpm dev
```

Useful when you only want to iterate on the React UI. Browser mode mainly uses mock data.

### Native Host Development Flow

```bash
pnpm native:host
```

This command:

- builds `native-bridge`
- launches the Swift-based `CodexIslandHostApp`

### Tauri Development Flow

```bash
pnpm tauri dev
```

This still works, but it is not currently the preferred distribution route for the fullscreen top-overlay macOS experience.

## Packaging

### Recommended: build the Swift host macOS app

```bash
pnpm build:host-app
```

Output:

- [macos-host/build/Codex Island.app](/Users/cong/Desktop/AI相关/codex-island/macos-host/build/Codex%20Island.app)

This bundle includes:

- frontend `dist`
- release `native-bridge`
- Swift `NSPanel` host executable

### Tauri Build

```bash
pnpm tauri build
```

The output is typically:

- [src-tauri/target/release/bundle/macos/Codex Island.app](/Users/cong/Desktop/AI相关/codex-island/src-tauri/target/release/bundle/macos/Codex%20Island.app)

Notes:

- the Tauri app builds successfully
- but for the fullscreen top-overlay goal on macOS, the Swift host is currently the preferred route

For fuller release guidance, see:

- [docs/RELEASE.md](/Users/cong/Desktop/AI相关/codex-island/docs/RELEASE.md)
- [docs/RELEASE_EN.md](/Users/cong/Desktop/AI相关/codex-island/docs/RELEASE_EN.md)

## Verification

```bash
pnpm test
pnpm build
cargo test --manifest-path src-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

If your local Swift / SDK environment is healthy, you can also run:

```bash
swift build --package-path macos-host -c release
```

## Current Limitations

- macOS only
- session detection and waiting-input detection still rely partly on heuristics
- terminal focusing is best-effort rather than guaranteed across every host
- fullscreen overlay behavior may still differ across host implementations
- building the Swift host app depends on a healthy local Swift / SDK setup

## Direction

- more accurate session and thread matching
- more reliable attention detection
- more robust app focusing and handoff
- fuller macOS release flow, including signing, `dmg`, and update automation

## Contributing

Issues and pull requests are welcome.

- If you hit reminder misclassification, missing sessions, or broken focus behavior, include clear reproduction steps when possible
- If you work on host-layer changes, keep the distinction between `macos-host` and `src-tauri` clear
- Run the verification commands before submitting changes

## License

This project currently uses the [GNU GPL v3.0](/Users/cong/Desktop/AI相关/codex-island/LICENSE).

# Contributing to Caffeine for Windows

Thank you for your interest in contributing to **Caffeine for Windows**!

## Core Architecture Philosophy

1. **Zero External Runtime:** Pure native code using Windows Win32 APIs. No .NET runtime, no Electron/WebView, no bulky frameworks.
2. **Lean & Minimal:** The binary size must stay under 200 KB. Add dependencies only if strictly necessary.
3. **Rock Solid:** All state transitions and timer logic must have unit test coverage.

## Development Workflow

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (edition 2024 / 1.85+)
- Windows 10 or 11 (x64)

### Building & Running
```powershell
# Clone the repository
git clone https://github.com/abdul-karim-mia/caffeine-win.git
cd caffeine-win

# Run unit tests
cargo test

# Run in debug mode
cargo run

# Build release binary (stripped & LTO optimized)
cargo build --release
```

## Pull Request Guidelines

1. Fork the repo and create your branch from `main`.
2. Ensure `cargo test` passes without errors or warnings.
3. Keep diffs focused and concise.
4. Submit your pull request with a descriptive title and checklist completed.

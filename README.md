# Caffeine for Windows ☕

A lightweight, zero-dependency Windows 11 keep-awake utility that lives in your system tray, inspired by Android's Caffeine quick tile.

Prevents your screen and PC from sleeping using native Win32 power management.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Binary Size](https://img.shields.io/badge/size-140KB-brightgreen.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2011%20%7C%2010-lightgrey.svg)

---

## ✨ Features

- **Circular Clock Animated Icon:** A live, dynamic pie chart in your system tray that drains clockwise like a clock face as time counts down.
- **Smart Color Indicator:**
  - 🔵 **Cyan/Blue**: Plenty of time left (> 20%)
  - 🟠 **Amber**: Less than 20% remaining
  - 🔴 **Red**: Final 5% remaining
  - ⚪ **Gray dot**: Off / Inactive
  - 💠 **Glowing ring**: Indefinite mode (always awake)
- **One-Click Cycling:** Left-click the tray icon to cycle timers instantly:
  ```
  Off ➔ 15 min ➔ 30 min ➔ 1 hour ➔ 2 hours ➔ 4 hours ➔ Indefinite ➔ Off
  ```
- **Live Tooltip:** Hover to see exact minutes and seconds remaining.
- **Right-Click Menu:** Pick any duration directly, toggle "Start with Windows", or exit.
- **Ultra-Lightweight:** Written in pure Rust with raw Win32 APIs (no .NET, no Electron, no WPF).
- **Single Exe (~140 KB):** Portable, runs immediately without installation.
- **Single Instance:** Prevents duplicate running instances.

---

## 🚀 Building & Testing

### Prerequisites
- [Rust](https://rustup.rs/) (stable toolchain)

### Run Tests
```powershell
cargo test
```

### Build Release Exe
```powershell
cargo build --release
```
The optimized executable will be generated at `target/release/caffeine-win.exe` (~140 KB).

---

## 📄 License
This project is open-source under the [MIT License](LICENSE).

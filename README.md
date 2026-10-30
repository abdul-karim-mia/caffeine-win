# Caffeine for Windows ☕

<p align="center">
  <img src="https://img.shields.io/github/actions/workflow/status/abdul-karim-mia/caffeine-win/ci.yml?branch=main&style=flat-square&label=CI" alt="CI" />
  <img src="https://img.shields.io/github/v/release/abdul-karim-mia/caffeine-win?style=flat-square&color=blue" alt="Release" />
  <img src="https://img.shields.io/badge/binary%20size-115%20KB-success?style=flat-square" alt="Size" />
  <img src="https://img.shields.io/badge/platform-Windows%2011%20%7C%2010-informational?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/badge/license-MIT-purple?style=flat-square" alt="License" />
</p>

An ultra-lightweight, zero-dependency Windows 11 system tray keep-awake utility inspired by Android's Caffeine quick tile.

It prevents your PC and screen from sleeping using official Windows power management APIs. It lives quietly in your system tray with a live circular clock icon that animates and drains like a countdown timer.

---

## ✨ Highlights

- **Zero Dependencies:** Pure native Rust with raw Win32 APIs. No .NET runtime, no Electron, no WebView2, no external DLLs.
- **Microscopic Footprint:** Single portable executable under **115 KB**, consuming ~2 MB RAM.
- **Dynamic Circular Clock Icon:** Draws a live pie-chart progress bar in the system tray in real time using native GDI.
- **Smart Color States:**
  - 🔵 **Cyan / Sky Blue:** Active (> 20% remaining)
  - 🟠 **Amber / Orange:** Warning (5% – 20% remaining)
  - 🔴 **Red Alert:** Final countdown (< 5% remaining)
  - 💠 **Glowing Cyan Ring:** Indefinite mode (always awake)
  - ⚪ **Dim Gray Ring:** Inactive / Off
- **One-Click Cycling:** Left-click the tray icon to cycle timers seamlessly:
  $$\text{Off} \longrightarrow \text{15 min} \longrightarrow \text{30 min} \longrightarrow \text{1 hr} \longrightarrow \text{2 hr} \longrightarrow \text{4 hr} \longrightarrow \text{Indefinite} \longrightarrow \text{Off}$$
- **Context Menu:** Right-click to directly jump to any preset, toggle **"Start with Windows"**, or exit.
- **Dark Mode Aware:** Automatically detects system-wide Windows dark mode and applies a native dark theme to the context menu and GDI tray icon rendering.
- **Auto Reset on Activity:** Toggle **"Auto Reset on Activity"** from the tray menu to restart the active countdown whenever mouse or keyboard input is detected.
- **Safe & Native:** Uses `SetThreadExecutionState` instead of simulating fake keystrokes (unlike old Caffeine tools that send F15 or jitter mice).
- **Single Instance:** Enforced via Win32 Named Mutex to prevent duplicate icons.

---

## 📊 Comparison

| Feature | **Caffeine for Windows** | PowerToys Awake | Classic Caffeine | Mouse Jigglers |
|---|---|---|---|---|
| **Binary Size** | **~115 KB** | ~150–200 MB (bundled) | ~50 KB | Varies |
| **Runtime Dependency** | **None (Native)** | .NET 8 / WPF | None (C++) | Often Java / Python |
| **Tray Icon Animation** | **Live Circular Clock** | Static icons | Static pot | None |
| **Keep-Awake Mechanism** | **`SetThreadExecutionState`** | `SetThreadExecutionState` | Simulated F15 key | Simulated mouse input |
| **Clean Screen Saver Handling** | **Yes** | Yes | Disturbs apps | Disturbs input |
| **One-Click Cycle** | **Yes (Left-Click)** | No | Double-click toggle only | No |

---

## 🖥️ How It Works

### 1. Power State Invalidation
Instead of injecting synthetic mouse movements or keyboard clicks that break typing and trigger anti-cheat software, Caffeine calls the Windows kernel power API:

```rust
SetThreadExecutionState(
    ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
);
```
- `ES_SYSTEM_REQUIRED`: Prevents CPU and system idle sleep.
- `ES_DISPLAY_REQUIRED`: Prevents the monitor and display from turning off.
- `ES_CONTINUOUS`: Informs Windows to keep this policy active until explicitly cleared.

### 2. GDI In-Memory Dynamic Icons
Every second, an in-memory 32×32 bitmap and 1-bit transparency mask are drawn with Win32 GDI primitives (`Pie`, `Ellipse`, `CreateIconIndirect`), creating smooth vector-like clock progress in the system tray.

### 3. Windows Autostart
Enabling "Start with Windows" registers an entry in:
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
No admin rights or UAC elevation required.

### 4. Dark Mode Support
The application reads the system theme via the registry key `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`. It loads `uxtheme.dll` and uses `SetPreferredAppMode` with `AllowDarkModeForWindow` (via a CBT hook on `#32768` dropdown menus) to force dark rendering on context menus and UI elements.

### 5. Auto Reset on Activity
When **"Auto Reset on Activity"** is enabled from the tray context menu, the application monitors system-wide input idle time using `GetLastInputInfo`. On any detected change in user activity (mouse or keyboard), the active timer's countdown is reset to its full duration from the moment of activity.

---

## 📥 Installation & Download

Two distribution options are available from [GitHub Releases](https://github.com/abdul-karim-mia/caffeine-win/releases):

### Portable (.exe)
1. Download the latest `caffeine-win.exe` from [GitHub Releases](https://github.com/abdul-karim-mia/caffeine-win/releases).
2. Move it to your preferred folder (e.g. `C:\Users\<User>\AppData\Local\Programs` or your Utilities folder).
3. Double-click to run!
4. *(Optional)* Right-click the tray icon and check **"Start with Windows"**.

### Installer (setup.exe)
1. Download `setup.exe` from [GitHub Releases](https://github.com/abdul-karim-mia/caffeine-win/releases).
2. Run the installer — it creates desktop and Start Menu shortcuts.
3. *(Optional)* Check **"Start with Windows"** during setup for auto-startup.
4. *(Optional)* Uninstall anytime via Windows Settings → Apps or the uninstaller shortcut.

---
### 🏗️ Building from Source

### Prerequisites
- [Rust](https://rustup.rs/) (stable toolchain)
- Windows 10 or Windows 11

### Commands
```powershell
# Clone the repository
git clone https://github.com/abdul-karim-mia/caffeine-win.git
cd caffeine-win

# Run test suite
cargo test

# Build optimized release binary
cargo build --release
```

The compiled binary will be placed at:
```
target\release\caffeine-win.exe
```

---

## 🤝 Contributing

Contributions, bug reports, and suggestions are welcome!
Please check out [CONTRIBUTING.md](CONTRIBUTING.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).

# Changelog

All notable changes to Caffeine for Windows are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com); versions match the
`version` field in `Cargo.toml` (SemVer) and the `v*` tags on GitHub.

## [0.2.5] - 2026-11-17

### Added

- **5-minute timer preset**, first in both the left-click cycle and the context menu.
- **"Reactivate timer after wake"** option in the tray menu: remembers the last timed
  duration and re-arms it as soon as the user's input returns after a keep-awake
  countdown expired. System-initiated wake-ups (maintenance, standby transition) carry
  no input and never re-arm the timer, so the PC still sleeps normally after the
  timeout. Expiry-only by design: turning the timer off manually never re-arms it.

## [0.2.4] - 2026-11-11

### Added

- App icon, installer and Microsoft Store (MSIX) packaging, release pipeline updates.

## [0.2.3] - 2026-11-11

### Changed

- Release workflow simplified to the portable binary only (no ARM64 installer asset).

## [0.2.2] - 2026-11-11

### Added

- GitHub stars and total download counters on the website (unauthenticated API, 1h cache).
- Responsive website on mobile and tablet.

### Fixed

- README Markdown formatting.

## [0.2.1] - 2026-11-11

### Added

- System-detecting download section on the website with x64 + ARM64 options.
- Native ARM64 builds shipped by CI, running independently of the x64 job.

## [0.2.0] - 2026-10-30

### Added

- Custom tray context menu matching Windows UI: acrylic backdrop with Mica fallback,
  Segoe Fluent Icons, animated hover pill, opaque fill with border and shadow,
  click-to-close, floating clear of the tray icons.
- Duration rows toggle off on re-pick, plus a 45-minute preset.
- Live system dark/light theme sync for the menu and idle icon.
- Windows screen-off timeout shown as the resting state in the menu header.
- Click-acknowledgement sweep on the tray icon.

### Fixed

- Idle tray icon readable on any taskbar: inverted with the system theme, ring no
  longer clipped by its own mask, stray line removed from the icon mask.
- Hover pill alignment on every row.

## [0.1.0] - 2026-10-30

### Added

- Initial release: zero-dependency Win32 tray keep-awake utility.
- `SetThreadExecutionState` keep-awake (no fake keystrokes or mouse jiggling).
- Left-click countdown cycle and right-click preset menu.
- Animated GDI circular clock tray icon with progress ring.
- Start with Windows autostart via the HKCU Run key.
- Single-instance enforcement and system dark mode detection.

> Store listing copy for the Partner Center submission. Fields map to the Partner
> Center "Store listing" page. Length limits are Microsoft's as of 2026-11: keep
> the description under 10,000 characters (this is ~1,600). Screenshots live in
> `screenshots/` — see `screenshots/README.md` for what to capture. The privacy
> policy URL is https://caffeine-win.vercel.app/privacy.html.

# Product name

Caffeine Win

# Short description (one line, shown under the title in some surfaces)

Keep your PC and screen awake from the system tray, with a live countdown clock icon.

# Description

Caffeine Win keeps your Windows PC and its screen awake, from the system tray, as
light as a tray app can be: about 290 KB, no runtimes, no dependencies, no admin
rights needed.

It uses the official Windows power API (SetThreadExecutionState) instead of the
fake-keystroke F15 trick older tools rely on - so nothing it does disturbs your
typing, your games, or anti-cheat software.

What it does

- Keeps the PC and display awake using the official Windows power API
- One left-click cycles the timer: 15 min, 30 min, 45 min, 1 h, 2 h, 4 h, or indefinitely
- Right-click picks any duration directly, toggles "Auto reset on activity", or exits
- A live animated circular clock icon drains in the tray as the timer counts down
- Colour tells you the state at a glance: blue active, amber under 20%, red in the last 5%, cyan ring for indefinite, dim ring for off
- Starts with Windows, so it is there when you log in (the Store build uses the Windows StartupTask; no registry edits)
- Follows your system light/dark theme for its menu and icon
- Native x64 and ARM64 builds for Copilot+ PCs

What it does not do

- No network connections, ever: nothing to sign in to, nothing to update in the background
- No telemetry, no analytics, no crash reporting
- No admin rights, no services, no drivers
- No fake keystrokes or mouse movement
- Open source under the MIT License: the whole app is one tray icon and one menu

# Keywords / search terms (max 7)

keep awake, stay awake, screen, sleep, timer, tray, power

# Categories

Utilities / System utilities

# Privacy policy URL

https://caffeine-win.vercel.app/privacy.html

# Support contact

https://github.com/abdul-karim-mia/caffeine-win/issues

# Notes for certification

Tray utility. No visible main window: the process exists in the notification
area only, like Volume, Network or OneDrive icons. runFullTrust is required to
draw the tray icon. Autostart uses the StartupTask extension instead of the
registry Run key, which MSIX virtualises. The app makes no network connections.

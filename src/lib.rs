// Architecture:
// 1. Invisible Message-Only Window for Tray events and WM_TIMER ticks.
// 2. Win32 NotifyIcon (Shell_NotifyIconW) with dynamic HICON generated via GDI.
// 3. Keep-Awake via SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED).
// 4. Timer cycle: Off -> 15m -> 30m -> 1h -> 2h -> 4h -> Infinite -> Off.
// 5. Dynamic Pie-chart Clock icon drawn on 32x32 bitmap in memory every 1 sec.
// 6. Zero external dependencies, pure native Win32.

pub mod keepawake;
pub mod icon;
pub mod state;
pub mod single_instance;
pub mod autostart;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerDuration {
    Off,
    Minutes(u32),
    Hours(u32),
    Infinite,
}

impl TimerDuration {
    pub const STEPS: [TimerDuration; 7] = [
        TimerDuration::Off,
        TimerDuration::Minutes(15),
        TimerDuration::Minutes(30),
        TimerDuration::Hours(1),
        TimerDuration::Hours(2),
        TimerDuration::Hours(4),
        TimerDuration::Infinite,
    ];

    pub fn total_seconds(&self) -> Option<u64> {
        match *self {
            TimerDuration::Off => None,
            TimerDuration::Minutes(m) if m > 0 => Some(m as u64 * 60),
            TimerDuration::Hours(h) if h > 0 => Some(h as u64 * 3600),
            TimerDuration::Minutes(_) | TimerDuration::Hours(_) => Some(0),
            TimerDuration::Infinite => None,
        }
    }

    pub fn is_active(&self) -> bool {
        !matches!(self, TimerDuration::Off)
    }

    pub fn next(&self) -> TimerDuration {
        let idx = Self::STEPS.iter().position(|s| s == self).unwrap_or(0);
        Self::STEPS[(idx + 1) % Self::STEPS.len()]
    }

    pub fn label(&self) -> &'static str {
        match *self {
            TimerDuration::Off => "Off",
            TimerDuration::Minutes(15) => "15 min",
            TimerDuration::Minutes(30) => "30 min",
            TimerDuration::Hours(1) => "1 hour",
            TimerDuration::Hours(2) => "2 hours",
            TimerDuration::Hours(4) => "4 hours",
            TimerDuration::Infinite => "Indefinite",
            _ => "Custom",
        }
    }
}

pub struct AppState {
    pub current_mode: TimerDuration,
    pub remaining_seconds: u64,
    pub initial_duration_seconds: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_mode: TimerDuration::Off,
            remaining_seconds: 0,
            initial_duration_seconds: 0,
        }
    }

    /// Sets the active mode. A custom zero-length duration is normalized to Off.
    pub fn set_mode(&mut self, mode: TimerDuration) {
        if matches!(mode, TimerDuration::Minutes(0) | TimerDuration::Hours(0)) {
            self.current_mode = TimerDuration::Off;
            self.remaining_seconds = 0;
            self.initial_duration_seconds = 0;
            return;
        }

        self.current_mode = mode;
        match mode.total_seconds() {
            Some(secs) if secs > 0 => {
                self.remaining_seconds = secs;
                self.initial_duration_seconds = secs;
            }
            _ => {
                self.remaining_seconds = 0;
                self.initial_duration_seconds = 0;
            }
        }
    }

    pub fn toggle_active(&mut self) -> TimerDuration {
        let next = if self.current_mode.is_active() {
            TimerDuration::Off
        } else {
            TimerDuration::Minutes(15)
        };
        self.set_mode(next);
        next
    }

    pub fn cycle_next(&mut self) -> TimerDuration {
        let next = self.current_mode.next();
        self.set_mode(next);
        next
    }

    /// Ticks one second down. Returns true exactly when a timed mode expires.
    /// Restarts the full countdown for the currently selected timed mode.
    /// Infinite and Off modes are intentionally unchanged.
    pub fn reset_countdown(&mut self) {
        if let Some(seconds) = self.current_mode.total_seconds() {
            if seconds > 0 {
                self.remaining_seconds = seconds;
                self.initial_duration_seconds = seconds;
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        match self.current_mode {
            TimerDuration::Off | TimerDuration::Infinite => false,
            _ if self.remaining_seconds == 0 => {
                self.set_mode(TimerDuration::Off);
                true
            }
            _ => {
                self.remaining_seconds -= 1;
                if self.remaining_seconds == 0 {
                    self.set_mode(TimerDuration::Off);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Progress from 0.0 (empty/expired) to 1.0 (full duration remaining).
    pub fn progress(&self) -> f32 {
        match self.current_mode {
            TimerDuration::Off => 0.0,
            TimerDuration::Infinite => 1.0,
            _ if self.initial_duration_seconds == 0 => 0.0,
            _ => (self.remaining_seconds as f32 / self.initial_duration_seconds as f32)
                .clamp(0.0, 1.0),
        }
    }

    pub fn tooltip_text(&self) -> String {
        match self.current_mode {
            TimerDuration::Off => "Caffeine: Off (Click to activate)".to_string(),
            TimerDuration::Infinite => "Caffeine: Always Awake (Indefinite)".to_string(),
            _ => {
                let mins = self.remaining_seconds / 60;
                let secs = self.remaining_seconds % 60;
                if mins >= 60 {
                    format!("Caffeine: {}h {}m remaining", mins / 60, mins % 60)
                } else {
                    format!("Caffeine: {:02}:{:02} remaining", mins, secs)
                }
            }
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

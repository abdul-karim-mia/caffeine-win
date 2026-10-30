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
            TimerDuration::Minutes(m) => Some(m as u64 * 60),
            TimerDuration::Hours(h) => Some(h as u64 * 3600),
            TimerDuration::Infinite => None,
        }
    }

    pub fn is_active(&self) -> bool {
        !matches!(self, TimerDuration::Off)
    }

    pub fn next(&self) -> TimerDuration {
        let idx = Self::STEPS.iter().position(|s| s == self).unwrap_or(0);
        let next_idx = (idx + 1) % Self::STEPS.len();
        Self::STEPS[next_idx]
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

    pub fn set_mode(&mut self, mode: TimerDuration) {
        self.current_mode = mode;
        match mode.total_seconds() {
            Some(secs) => {
                self.remaining_seconds = secs;
                self.initial_duration_seconds = secs;
            }
            None => {
                self.remaining_seconds = 0;
                self.initial_duration_seconds = 0;
            }
        }
    }

    pub fn cycle_next(&mut self) -> TimerDuration {
        let nxt = self.current_mode.next();
        self.set_mode(nxt);
        nxt
    }

    /// Ticks 1 second down. Returns true if expired on this tick.
    pub fn tick(&mut self) -> bool {
        if self.current_mode == TimerDuration::Infinite {
            return false;
        }
        if self.remaining_seconds > 0 {
            self.remaining_seconds -= 1;
            if self.remaining_seconds == 0 {
                self.current_mode = TimerDuration::Off;
                self.initial_duration_seconds = 0;
                return true;
            }
        }
        false
    }

    /// Progress from 0.0 (empty/expired) to 1.0 (full duration remaining).
    /// Infinite returns 1.0. Off returns 0.0.
    pub fn progress(&self) -> f32 {
        match self.current_mode {
            TimerDuration::Off => 0.0,
            TimerDuration::Infinite => 1.0,
            _ => {
                if self.initial_duration_seconds == 0 {
                    0.0
                } else {
                    (self.remaining_seconds as f32 / self.initial_duration_seconds as f32)
                        .clamp(0.0, 1.0)
                }
            }
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
                    let hrs = mins / 60;
                    let rem_m = mins % 60;
                    format!("Caffeine: {}h {}m remaining", hrs, rem_m)
                } else {
                    format!("Caffeine: {:02}:{:02} remaining", mins, secs)
                }
            }
        }
    }
}

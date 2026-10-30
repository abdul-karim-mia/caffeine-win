use crate::state::{AppState, TimerDuration};

#[test]
fn test_timer_cycle_sequence() {
    let mut state = AppState::new();
    assert_eq!(state.current_mode, TimerDuration::Off);

    assert_eq!(state.cycle_next(), TimerDuration::Minutes(15));
    assert_eq!(state.remaining_seconds, 15 * 60);

    assert_eq!(state.cycle_next(), TimerDuration::Minutes(30));
    assert_eq!(state.remaining_seconds, 30 * 60);

    assert_eq!(state.cycle_next(), TimerDuration::Hours(1));
    assert_eq!(state.remaining_seconds, 3600);

    assert_eq!(state.cycle_next(), TimerDuration::Hours(2));
    assert_eq!(state.remaining_seconds, 7200);

    assert_eq!(state.cycle_next(), TimerDuration::Hours(4));
    assert_eq!(state.remaining_seconds, 14400);

    assert_eq!(state.cycle_next(), TimerDuration::Infinite);
    assert_eq!(state.remaining_seconds, 0);

    assert_eq!(state.cycle_next(), TimerDuration::Off);
    assert_eq!(state.remaining_seconds, 0);
}

#[test]
fn test_tick_countdown_and_expiration() {
    let mut state = AppState::new();
    state.set_mode(TimerDuration::Minutes(15));
    state.remaining_seconds = 2; // Artificially fast-forward

    assert_eq!(state.tick(), false);
    assert_eq!(state.remaining_seconds, 1);
    assert_eq!(state.current_mode, TimerDuration::Minutes(15));

    // Expiration tick
    assert_eq!(state.tick(), true);
    assert_eq!(state.remaining_seconds, 0);
    assert_eq!(state.current_mode, TimerDuration::Off);
}

#[test]
fn test_infinite_never_expires() {
    let mut state = AppState::new();
    state.set_mode(TimerDuration::Infinite);
    assert_eq!(state.tick(), false);
    assert_eq!(state.current_mode, TimerDuration::Infinite);
}

#[test]
fn test_progress_calculation() {
    let mut state = AppState::new();
    assert_eq!(state.progress(), 0.0);

    state.set_mode(TimerDuration::Infinite);
    assert_eq!(state.progress(), 1.0);

    state.set_mode(TimerDuration::Minutes(10));
    assert_eq!(state.progress(), 1.0);

    state.remaining_seconds = 300; // Half remaining
    assert!((state.progress() - 0.5).abs() < 0.001);
}

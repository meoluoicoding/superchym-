use std::thread;
use std::time::Duration;

use mushroom_bot::timer::Timer;

#[test]
fn timer_tracks_elapsed_and_budget() {
    let mut timer = Timer::new();
    thread::sleep(Duration::from_millis(5));
    assert!(timer.elapsed_ms() >= 1);
    assert!(timer.timed_out(1));

    timer.start();
    assert!(!timer.timed_out(100));
}

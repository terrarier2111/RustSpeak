use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn current_time_millis() -> Duration {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch
}
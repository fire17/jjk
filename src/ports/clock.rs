use std::time::{Duration, Instant};
use time::OffsetDateTime;

pub(crate) trait Clock {
    fn now_utc(&self) -> OffsetDateTime;
    fn monotonic_now(&self) -> Instant;

    fn deadline_after(&self, duration: Duration) -> Instant {
        self.monotonic_now() + duration
    }
}

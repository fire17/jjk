use std::time::Instant;
use time::OffsetDateTime;
use crate::ports::clock::Clock;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemClock;
impl Clock for SystemClock {
    fn now_utc(&self) -> OffsetDateTime { OffsetDateTime::now_utc() }
    fn monotonic_now(&self) -> Instant { Instant::now() }
}

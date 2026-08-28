use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LockOwner {
    pub process_id: u32,
    pub operation: Option<String>,
}

pub(crate) trait WriterLock {
    type Guard;
    type Error;

    fn try_acquire(&self, timeout: Duration, owner: LockOwner) -> Result<Self::Guard, Self::Error>;
}

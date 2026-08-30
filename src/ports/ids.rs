use uuid::Uuid;

pub(crate) trait IdSource {
    fn new_v7(&self) -> Uuid;
}

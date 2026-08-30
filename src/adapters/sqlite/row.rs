/// Converts an integer stored by SQLite into a checked sequence number.
pub(super) fn sequence(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            std::mem::size_of::<i64>(),
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

//! PostgreSQL refusals that callers turn into domain errors.

/// Whether `error` breaks a unique constraint (SQLSTATE 23505): an address,
/// a name or a slug already taken.
pub fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|code| code == "23505")
}

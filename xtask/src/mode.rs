//! Host awareness for lint and CI replication.

/// Whether commands are already running on native Windows.
pub(crate) const fn is_native() -> bool {
    cfg!(windows)
}

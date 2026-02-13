//! Shared error types for the entire workspace.
//!
//! These types live at the crate root to avoid circular dependencies
//! between `application` and `objects2d`.

/// Application-level error type.
#[derive(Debug)]
pub enum AppError {
    CustomError(String),
    // Add more variants as needed
}

/// Convenience alias used throughout the codebase.
pub type AppResult<T = ()> = Result<T, AppError>;

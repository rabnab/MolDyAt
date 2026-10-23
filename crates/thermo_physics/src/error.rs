// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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

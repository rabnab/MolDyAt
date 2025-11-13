use crate::frame::AppError;
use ggez::GameError;

pub(crate) mod custom_event_loop;
mod ggez_renderer;
mod ggez_renderer_backend;
mod ggez_renderer_builder;
pub(crate) mod ggez_window;

impl From<GameError> for AppError {
    fn from(err: GameError) -> Self {
        match err {
            GameError::CustomError(msg) => AppError::CustomError(msg),
            _ => AppError::CustomError("Unknown GameError".to_string()),
        }
    }
}

impl From<AppError> for GameError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::CustomError(msg) => GameError::CustomError(msg),
        }
    }
}

use crate::frame::AppError;
use crate::objects2d::Vec2D;
use ggez::GameError;
use ggez::mint::Point2;

pub(crate) mod custom_event_loop;
#[cfg(test)]
mod custom_event_loop_tests;
mod ggez_renderer;
mod ggez_renderer_backend;
mod ggez_renderer_builder;
pub(crate) mod ggez_window;

impl From<Vec2D> for Point2<f32> {
    fn from(val: Vec2D) -> Self {
        Point2 { x: val.x, y: val.y }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_point2() {
        let x_target: f32 = 4.;
        let y_target: f32 = 1.234E-9;
        let v = Vec2D::new(x_target, y_target);
        let p: Point2<f32> = v.into();
        assert_eq!(p.x, x_target);
        assert_eq!(p.y, y_target);
    }
}

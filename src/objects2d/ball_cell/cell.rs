use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use ggez::Context;
use ggez::graphics::{Color, Mesh};
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

pub type BallCell<'a> = Rc<RefCell<Ball2D<'a>>>;

pub struct Cell<'a> {
    pub balls: Vec<BallCell<'a>>,
    pub bounds: CellBounds,
    pub rect_mesh: Option<Mesh>,
}

#[derive(Clone, Copy)]
pub struct CellBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Debug for CellBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cell x {{ {:.2}, {:.2} }}, y {{ {:.2}, {:.2} }}",
            self.min_x, self.max_x, self.min_y, self.max_y
        )
    }
}

impl Debug for Cell<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "balls {} bounds: {:?}", self.balls.len(), self.bounds)
    }
}

impl<'a> Cell<'a> {
    pub fn new(bounds: CellBounds, rect_color: Color, ctx: Option<&Context>) -> Self {
        let rect = ggez::graphics::Rect::new(
            0.,
            0.,
            bounds.max_x - bounds.min_x,
            bounds.min_y - bounds.max_y,
        );

        let rect_mesh = ctx.map(|the_ctx| {
            Mesh::new_rectangle(the_ctx, ggez::graphics::DrawMode::fill(), rect, rect_color)
                .unwrap()
        });

        Self {
            balls: Vec::new(),
            bounds,
            rect_mesh,
        }
    }

    pub fn add_ball(&mut self, ball: BallCell<'a>) {
        self.balls.push(ball);
    }

    pub fn remove_ball(&mut self, ball_id: usize) {
        self.balls.retain(|b| b.borrow().get_id() != ball_id);
    }

    pub fn get_balls(&self) -> impl Iterator<Item = &BallCell<'a>> {
        self.balls.iter()
    }

    pub fn drain_balls(&mut self) -> Vec<BallCell<'a>> {
        self.balls.drain(..).collect()
    }

    pub fn get_top_left(&self) -> Vec2D {
        Vec2D::new(self.bounds.min_x, self.bounds.min_y)
    }

    pub fn get_rect_mesh(&self) -> Option<&Mesh> {
        self.rect_mesh.as_ref()
    }

    pub fn clear(&mut self) {
        self.balls.clear();
    }
}

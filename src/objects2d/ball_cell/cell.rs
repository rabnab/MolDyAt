use crate::objects2d::ball_impl::Ball2D;
use std::cell::RefCell;
use std::rc::Rc;

pub type BallCell<'a> = Rc<RefCell<Ball2D<'a>>>;

pub struct Cell<'a> {
    pub balls: Vec<BallCell<'a>>,
    pub bounds: CellBounds,
}

#[derive(Debug, Clone, Copy)]
pub struct CellBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl<'a> Cell<'a> {
    pub fn new(bounds: CellBounds) -> Self {
        Self {
            balls: Vec::new(),
            bounds,
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

    pub fn clear(&mut self) {
        self.balls.clear();
    }
}

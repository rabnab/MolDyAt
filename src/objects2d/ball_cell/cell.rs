use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use std::cell::RefCell;
use std::fmt::Debug;
use std::hash::Hash;
use std::rc::Rc;

pub type BallCell = Rc<RefCell<Ball2D>>;

pub struct Cell {
    pub balls: Vec<BallCell>,
    pub bounds: CellBounds,
    pub id: CellId,
}
#[derive(Clone, Copy)]
pub struct CellId {
    pub row: usize,
    pub col: usize,
    pub id: usize,
}

impl Debug for CellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}x{})", self.row, self.col)
    }
}

impl CellId {
    pub fn new(row: usize, col: usize, id: usize) -> Self {
        Self { row, col, id }
    }
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

impl Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "balls {} bounds: {:?}", self.balls.len(), self.bounds)
    }
}

impl Cell {
    pub fn new(bounds: CellBounds, row: usize, column: usize, num_column: usize) -> Self {
        let id = CellId::new(row, column, row * num_column + column);

        Self {
            balls: Vec::new(),
            bounds,
            id,
        }
    }

    pub fn add_ball(&mut self, ball: BallCell) {
        self.balls.push(ball);
    }

    pub fn remove_ball(&mut self, ball_id: usize) {
        self.balls.retain(|b| b.borrow().get_id() != ball_id);
    }

    pub fn get_balls(&self) -> impl Iterator<Item = &BallCell> {
        self.balls.iter()
    }

    pub fn drain_balls(&mut self) -> Vec<BallCell> {
        self.balls.drain(..).collect()
    }

    pub fn get_top_left(&self) -> Vec2D {
        Vec2D::new(self.bounds.min_x, self.bounds.min_y)
    }

    pub fn get_bottom_right(&self) -> Vec2D {
        Vec2D::new(self.bounds.max_x, self.bounds.max_y)
    }

    pub fn clear(&mut self) {
        self.balls.clear();
    }
}

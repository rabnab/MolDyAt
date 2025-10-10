#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    /// Create a new Rect from top-left coordinates, width and height
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Create a Rect from top-left and bottom-right coordinates
    pub fn from_tl_br(tl: (f32, f32), br: (f32, f32)) -> Self {
        let (x1, y1) = tl;
        let (x2, y2) = br;
        Self {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }

    pub fn get_tl(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn get_br(&self) -> (f32, f32) {
        (self.x + self.w, self.y + self.h)
    }
}

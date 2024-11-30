use std::fmt;
use ggez::mint::Point2;

#[derive(Debug)]
pub struct Vec2D {
    pub x: f32,
    pub y: f32,
}


impl Vec2D {
    pub fn new(_x: f32, _y: f32)  -> Vec2D {
        Vec2D {
            x: _x,
            y: _y,
        }
    }

    pub fn norm_sq(pt: &Vec2D) -> f32 {
        pt.x*pt.x + pt.y * pt.y
    }

    pub fn norm(pt: &Vec2D) -> f32 {
        f32::sqrt(Vec2D::norm_sq(pt))
    }
    
    pub fn copy_scaled(pt: &Vec2D, scale:f32) -> Vec2D {
        Vec2D::new( pt.x * scale, pt.y * scale)
    }


    /// calculates the distance vector from this ball to the other ball
    pub fn dist(&self, other: &Vec2D) -> Vec2D {
        let dist_x = other.x - self.x;
        let dist_y = other.y - self.y;
        Vec2D::new(dist_x, dist_y)
    }
    
    pub fn add(&mut self, pt: Vec2D) {
       self.x += pt.x;
       self.y += pt.y;
    }

    fn _scale(&mut self, scale:f32) {
        self.x *= scale;
        self.y *= scale;
    }

    fn _negate(&mut self) {
        self._scale(-1.);
    }
}

impl fmt::Display for Vec2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({:.2E},{:.2E})", self.x, self.y)
    }
}

impl Into<Point2<f32>> for Vec2D {
    fn into(self) -> Point2<f32> {
        Point2 { x: self.x, y: self.y }
    }
}

impl Clone for Vec2D {
    fn clone(&self) -> Vec2D {
        Vec2D::new(self.x, self.y)
    }
}

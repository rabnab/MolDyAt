use std::fmt;
use ggez::mint::Point2;

pub struct Point2D {
    pub x: f32,
    pub y: f32,
}


impl Point2D {
    pub fn new(_x: f32, _y: f32)  -> Point2D {
        Point2D {
            x: _x,
            y: _y,
        }
    }

    fn norm_sq(pt: &Point2D) -> f32 {
        pt.x*pt.x + pt.y * pt.y
    }

    pub fn norm(pt: &Point2D) -> f32 {
        f32::sqrt(Point2D::norm_sq(pt))
    }
    
    pub fn copy_scaled(pt: &Point2D, scale:f32) -> Point2D {
        Point2D::new( pt.x * scale, pt.y * scale)
    }


    /// calculates the distance vector from this ball to the other ball
    pub fn dist(&self, other: &Point2D) -> Point2D {
        let dist_x = other.x - self.x;
        let dist_y = other.y - self.y;
        Point2D::new(dist_x, dist_y)
    }
    
    pub fn add(&mut self, pt: Point2D) {
       self.x += pt.x;
       self.y += pt.y;
    }

    fn scale(&mut self, scale:f32) {
        self.x *= scale;
        self.y *= scale;
    }

    fn negate(&mut self) {
        self.scale(-1.);
    }
}

impl fmt::Display for Point2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

impl Into<Point2<f32>> for Point2D {
    fn into(self) -> Point2<f32> {
        Point2 { x: self.x, y: self.y }
    }
}

impl Clone for Point2D {
    fn clone(&self) -> Point2D {
        Point2D::new(self.x, self.y)
    }
}

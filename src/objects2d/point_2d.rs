use std::fmt;
use ggez::mint::Point2;

#[derive(Debug)]
#[derive(PartialEq)]
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

    pub fn norm_sq_of(pt: &Vec2D) -> f32 {
        pt.x*pt.x + pt.y * pt.y
    }

    pub fn norm_of(pt: &Vec2D) -> f32 {
        f32::sqrt(Vec2D::norm_sq_of(pt))
    }
    
    pub fn scaled_copy_of(pt: &Vec2D, scale:f32) -> Vec2D {
        Vec2D::new( pt.x * scale, pt.y * scale)
    }

    pub fn scaled_copy(&self, scale:f32) -> Vec2D {
        Vec2D::scaled_copy_of(self, scale)
    }

    /// calculates the distance vector from this ball to the other ball
    pub fn dist(&self, other: &Vec2D) -> Vec2D {
        let dist_x = other.x - self.x;
        let dist_y = other.y - self.y;
        Vec2D::new(dist_x, dist_y)
    }

    pub fn norm(&self) -> f32 {
        Vec2D::norm_of(self)
    }

    pub fn norm_sq(&self) -> f32 {
        Vec2D::norm_sq_of(self)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_point2(){
        let x_target:f32 = 4.;
        let y_target:f32 = 1.234E-9;
        let v = Vec2D::new(x_target, y_target);
        let p: Point2<f32> = v.into();
        assert_eq!(p.x, x_target);
        assert_eq!(p.y, y_target);
    }

    #[test]
    fn test_clone_vec2d(){
        let x_target:f32 = 4.;
        let y_target:f32 = 1.234E-9;
        let mut v1 = Vec2D::new(x_target, y_target);
        let v2 = v1.clone();

        assert_eq!(v2.x, v1.x);
        assert_eq!(v2.y, v1.y);

        v1.x = x_target*2.;
        assert_ne!(v1.x, v2.x);
    }

     #[test]
    fn test_new() {
        let v = Vec2D::new(3.0, 4.0);
        assert_eq!(v.x, 3.0);
        assert_eq!(v.y, 4.0);
    }

    #[test]
    fn test_norm_sq() {
        let v = Vec2D::new(3.0, 4.0);
        assert_eq!(Vec2D::norm_sq_of(&v), 25.0);
    }

    #[test]
    fn test_norm() {
        let v = Vec2D::new(3.0, 4.0);
        assert_eq!(Vec2D::norm_of(&v), 5.0);
    }

    #[test]
    fn test_copy_scaled() {
        let v = Vec2D::new(2.0, 3.0);
        let scaled = Vec2D::scaled_copy_of(&v, 2.0);
        assert_eq!(scaled.x, 4.0);
        assert_eq!(scaled.y, 6.0);
    }

    #[test]
    fn test_dist() {
        let v1 = Vec2D::new(1.0, 2.0);
        let v2 = Vec2D::new(4.0, 6.0);
        let dist = v1.dist(&v2);
        assert_eq!(dist.x, 3.0);
        assert_eq!(dist.y, 4.0);
    }

    #[test]
    fn test_add() {
        let mut v1 = Vec2D::new(1.0, 2.0);
        let v2 = Vec2D::new(3.0, 4.0);
        v1.add(v2);
        assert_eq!(v1.x, 4.0);
        assert_eq!(v1.y, 6.0);
    }

    #[test]
    fn test_private_scale() {
        let mut v = Vec2D::new(2.0, 3.0);
        v._scale(2.0);
        assert_eq!(v.x, 4.0);
        assert_eq!(v.y, 6.0);
    }

    #[test]
    fn test_private_negate() {
        let mut v = Vec2D::new(2.0, -3.0);
        v._negate();
        assert_eq!(v.x, -2.0);
        assert_eq!(v.y, 3.0);
    }

    #[test]
    fn test_display() {
        let v = Vec2D::new(1234.5678, -0.0009);
        assert_eq!(format!("{}", v), "(1.23E3,-9.00E-4)");
    }
}

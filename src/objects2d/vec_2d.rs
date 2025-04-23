use ggez::mint::Point2;
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Vec2D {
    pub x: f32,
    pub y: f32,
}

impl Vec2D {
    pub fn new(x: f32, y: f32) -> Vec2D {
        Vec2D { x, y }
    }

    /// Returns the squared norm.
    pub fn norm_sq(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Returns the norm (magnitude).
    pub fn norm(&self) -> f32 {
        self.norm_sq().sqrt()
    }

    /// Creates a scaled copy of the vector.
    pub fn scaled(&self, scale: f32) -> Vec2D {
        Vec2D::new(self.x * scale, self.y * scale)
    }

    /// Adds another vector and returns a new vector.
    pub fn add(&self, pt: Vec2D) -> Vec2D {
        Vec2D::new(self.x + pt.x, self.y + pt.y)
    }

    /// Adds another vector, scaled by a factor, and returns a new vector.
    pub fn add_scaled(&self, pt: Vec2D, scale: f32) -> Vec2D {
        let scaled_pt = pt.scaled(scale);
        Vec2D::new(self.x + scaled_pt.x, self.y + scaled_pt.y)
    }

    /// Computes the distance vector to another vector.
    pub fn dist(&self, other: &Vec2D) -> Vec2D {
        Vec2D::new(other.x - self.x, other.y - self.y)
    }

    /// Adds another vector to this one mutably.
    pub fn add_mut(&mut self, pt: Vec2D) {
        *self = self.add_scaled(pt, 1.0);
    }

    /// Scales the vector in place.
    #[allow(dead_code)]
    pub fn scale_mut(&mut self, scale: f32) {
        self.x *= scale;
        self.y *= scale;
    }

    /// Negates the vector in place.
    #[allow(dead_code)]
    fn negate_mut(&mut self) {
        self.scale_mut(-1.0);
    }
}

impl fmt::Display for Vec2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({:.2E}, {:.2E})", self.x, self.y)
    }
}

impl Into<Point2<f32>> for Vec2D {
    fn into(self) -> Point2<f32> {
        Point2 {
            x: self.x,
            y: self.y,
        }
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
    fn test_into_point2() {
        let x_target: f32 = 4.;
        let y_target: f32 = 1.234E-9;
        let v = Vec2D::new(x_target, y_target);
        let p: Point2<f32> = v.into();
        assert_eq!(p.x, x_target);
        assert_eq!(p.y, y_target);
    }

    #[test]
    fn test_clone_vec2d() {
        let x_target: f32 = 4.;
        let y_target: f32 = 1.234E-9;
        let mut v1 = Vec2D::new(x_target, y_target);
        let v2 = v1.clone();

        assert_eq!(v2.x, v1.x);
        assert_eq!(v2.y, v1.y);

        v1.x = x_target * 2.;
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
        assert_eq!(Vec2D::norm_sq(&v), 25.0);
    }

    #[test]
    fn test_norm() {
        let v = Vec2D::new(3.0, 4.0);
        assert_eq!(Vec2D::norm(&v), 5.0);
    }

    #[test]
    fn test_copy_scaled() {
        let v = Vec2D::new(2.0, 3.0);
        let scaled = v.scaled(2.0);
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
        v1.add_mut(v2);
        assert_eq!(v1.x, 4.0);
        assert_eq!(v1.y, 6.0);
    }

    #[test]
    fn test_private_scale() {
        let mut v = Vec2D::new(2.0, 3.0);
        v.scale_mut(2.0);
        assert_eq!(v.x, 4.0);
        assert_eq!(v.y, 6.0);
    }

    #[test]
    fn test_private_negate() {
        let mut v = Vec2D::new(2.0, -3.0);
        v.negate_mut();
        assert_eq!(v.x, -2.0);
        assert_eq!(v.y, 3.0);
    }

    #[test]
    fn test_display() {
        let v = Vec2D::new(1234.5678, -0.0009);
        assert_eq!(format!("{}", v), "(1.23E3, -9.00E-4)");
    }
}

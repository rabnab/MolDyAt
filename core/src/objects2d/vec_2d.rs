use std::fmt;
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Vec2D {
    pub x: f32,
    pub y: f32,
}

impl Vec2D {
    /// Create a new 2D vector.
    ///
    /// # Arguments
    /// * `x` - X component
    /// * `y` - Y component
    pub fn new(x: f32, y: f32) -> Vec2D {
        Vec2D { x, y }
    }

    /// Returns the squared norm.
    pub fn norm_sq(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Create a zero vector.
    pub fn zero() -> Vec2D {
        Vec2D::new(0.0, 0.0)
    }

    /// Check if both components are finite (not NaN or infinite).
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Returns the norm (magnitude).
    pub fn norm(&self) -> f32 {
        self.norm_sq().sqrt()
    }

    /// Dot product.
    pub fn dot(&self, other: &Vec2D) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Returns the unit vector (normalized).
    /// Returns zero vector if magnitude is zero.
    pub fn normalize(&self) -> Vec2D {
        let norm = self.norm();
        if norm > 0.0 {
            self.scaled(1.0 / norm)
        } else {
            Vec2D::zero()
        }
    }

    /// Creates a scaled copy of the vector.
    pub fn scaled(&self, scale: f32) -> Vec2D {
        Vec2D::new(self.x * scale, self.y * scale)
    }

    /// Adds another vector and returns a new vector.
    fn add_vec(&self, pt: &Vec2D) -> Vec2D {
        Vec2D::new(self.x + pt.x, self.y + pt.y)
    }

    /// Adds another vector, scaled by a factor, and returns a new vector.
    /// Equivalent to: self + (pt * scale)
    ///
    /// # Arguments
    /// * `pt` - Vector to add
    /// * `scale` - Scale factor for pt
    pub fn add_scaled(&self, pt: &Vec2D, scale: f32) -> Vec2D {
        let scaled_pt = pt.scaled(scale);
        Vec2D::new(self.x + scaled_pt.x, self.y + scaled_pt.y)
    }

    /// Computes the distance vector to another vector.
    /// Returns vector pointing from self to other.
    ///
    /// # Arguments
    /// * `other` - Target vector
    pub fn dist(&self, other: &Vec2D) -> Vec2D {
        Vec2D::new(other.x - self.x, other.y - self.y)
    }

    pub fn distance_squared(&self, other: &Vec2D) -> f32 {
        self.dist(other).norm_sq()
    }

    /// Adds another vector to this one mutably.
    fn add_mut(&mut self, pt: &Vec2D) {
        *self = self.add_scaled(pt, 1.0);
    }

    /// Scales the vector in place.
    fn scale_mut(&mut self, scale: f32) {
        self.x *= scale;
        self.y *= scale;
    }

    /// Negates the vector in place.
    fn negate_mut(&mut self) {
        self.scale_mut(-1.0);
    }
}

impl Add<Vec2D> for Vec2D {
    type Output = Vec2D;

    fn add(self, rhs: Vec2D) -> Self::Output {
        self.add_vec(&rhs)
    }
}
impl Add<&Vec2D> for Vec2D {
    type Output = Vec2D;

    fn add(self, rhs: &Vec2D) -> Self::Output {
        self.add_vec(rhs)
    }
}

impl Add<&Vec2D> for &Vec2D {
    type Output = Vec2D;

    fn add(self, rhs: &Vec2D) -> Self::Output {
        self.add_vec(rhs)
    }
}

impl Add<Vec2D> for &Vec2D {
    type Output = Vec2D;

    fn add(self, rhs: Vec2D) -> Self::Output {
        self.add_vec(&rhs)
    }
}

impl Sub<&Vec2D> for Vec2D {
    type Output = Vec2D;
    fn sub(self, rhs: &Vec2D) -> Self::Output {
        self.add_scaled(rhs, -1.0)
    }
}

impl AddAssign for Vec2D {
    fn add_assign(&mut self, rhs: Self) {
        self.add_mut(&rhs);
    }
}

impl SubAssign for Vec2D {
    fn sub_assign(&mut self, rhs: Self) {
        self.add_mut(&(rhs.scaled(-1.0)));
    }
}

impl Sub<&Vec2D> for &Vec2D {
    type Output = Vec2D;
    fn sub(self, rhs: &Vec2D) -> Self::Output {
        self.add_scaled(rhs, -1.0)
    }
}

impl Mul<f32> for Vec2D {
    type Output = Vec2D;

    fn mul(self, rhs: f32) -> Self::Output {
        self.scaled(rhs)
    }
}
impl MulAssign<f32> for Vec2D {
    fn mul_assign(&mut self, rhs: f32) {
        self.scale_mut(rhs);
    }
}

impl Mul<f32> for &Vec2D {
    type Output = Vec2D;

    fn mul(self, rhs: f32) -> Self::Output {
        self.scaled(rhs)
    }
}

impl Neg for Vec2D {
    type Output = Vec2D;
    fn neg(self) -> Self::Output {
        self.scaled(-1.0)
    }
}

impl fmt::Display for Vec2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({:.2E}, {:.2E})", self.x, self.y)
    }
}

impl From<[f32; 2]> for Vec2D {
    fn from(coords: [f32; 2]) -> Self {
        Vec2D::new(coords[0], coords[1])
    }
}

impl From<(f32, f32)> for Vec2D {
    fn from(coords: (f32, f32)) -> Self {
        Vec2D::new(coords.0, coords.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot() {
        let a = Vec2D::new(3.0, -2.0);
        assert_eq!(a.dot(&Vec2D::new(4.0, 5.0)), 2.0);
        assert_eq!(a.dot(&Vec2D::new(2.0, 3.0)), 0.0); // perpendicular
        assert_eq!(a.dot(&a), a.norm_sq());
    }

    #[test]
    fn test_from_array() {
        let v = Vec2D::from([1.0, 2.0]);
        assert_eq!(v.x, 1.0);
        assert_eq!(v.y, 2.0);
    }

    #[test]
    fn test_clone_vec2d() {
        let x_target: f32 = 4.;
        let y_target: f32 = 1.234E-9;
        let mut v1 = Vec2D::new(x_target, y_target);
        let v2 = v1;

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

    #[test]
    fn test_mul() {
        let v = Vec2D::new(1.0, 2.0);
        let v2 = v * 2.0;
        assert_eq!(v2.x, 2.0);
        assert_eq!(v2.y, 4.0);
    }

    #[test]
    fn test_mul_ref() {
        let v = &Vec2D::new(1.0, 2.0);
        let v2 = v * 2.0;
        assert_eq!(v2.x, 2.0);
        assert_eq!(v2.y, 4.0);
    }

    #[test]
    fn test_mul_assign() {
        let mut v = Vec2D::new(1.0, 2.0);
        v *= 2.0;
        assert_eq!(v.x, 2.0);
        assert_eq!(v.y, 4.0);
    }

    #[test]
    fn test_add() {
        let v1 = Vec2D::new(1.0, 2.0);
        let v2 = Vec2D::new(3.0, 4.0);
        let v3 = v1 + v2;
        assert_eq!(v3.x, 4.0);
        assert_eq!(v3.y, 6.0);
    }

    #[test]
    fn test_add_vec_ref() {
        let v1 = &Vec2D::new(1.0, 2.0);
        let v2 = Vec2D::new(3.0, 4.0);
        let v3 = v2 + v1;
        assert_eq!(v3.x, 4.0);
        assert_eq!(v3.y, 6.0);
    }

    #[test]
    fn test_add_ref_vec() {
        let v1 = &Vec2D::new(1.0, 2.0);
        let v2 = Vec2D::new(3.0, 4.0);
        let v3 = v1 + v2;
        assert_eq!(v3.x, 4.0);
        assert_eq!(v3.y, 6.0);
    }

    #[test]
    fn test_add_ref_ref() {
        let v1 = &Vec2D::new(1.0, 2.0);
        let v2 = &Vec2D::new(3.0, 4.0);
        let v3 = v1 + v2;
        assert_eq!(v3.x, 4.0);
        assert_eq!(v3.y, 6.0);
    }
}

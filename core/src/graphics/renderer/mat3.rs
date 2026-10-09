// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;

/// Simple 3x3 matrix for 2D transformations.
/// Used for efficient coordinate transforms with caching.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    // Row-major order for row vectors: [m0 m1 m2; m3 m4 m5; m6 m7 m8]
    m0: f32,
    m1: f32,
    m2: f32,
    m3: f32,
    m4: f32,
    m5: f32,
    m6: f32,
    m7: f32,
    m8: f32,
}

impl Mat3 {
    /// Create identity matrix
    pub fn identity() -> Self {
        Self {
            m0: 1.0,
            m1: 0.0,
            m2: 0.0,
            m3: 0.0,
            m4: 1.0,
            m5: 0.0,
            m6: 0.0,
            m7: 0.0,
            m8: 1.0,
        }
    }

    /// Create translation matrix
    pub fn translation(tx: f32, ty: f32) -> Self {
        Self {
            m0: 1.0,
            m1: 0.0,
            m2: tx,
            m3: 0.0,
            m4: 1.0,
            m5: ty,
            m6: 0.0,
            m7: 0.0,
            m8: 1.0,
        }
    }

    /// Create scale matrix
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m0: sx,
            m1: 0.0,
            m2: 0.0,
            m3: 0.0,
            m4: sy,
            m5: 0.0,
            m6: 0.0,
            m7: 0.0,
            m8: 1.0,
        }
    }

    /// Matrix multiplication (self * other)
    pub fn mul(&self, other: &Mat3) -> Mat3 {
        Mat3 {
            m0: self.m0 * other.m0 + self.m1 * other.m3 + self.m2 * other.m6,
            m1: self.m0 * other.m1 + self.m1 * other.m4 + self.m2 * other.m7,
            m2: self.m0 * other.m2 + self.m1 * other.m5 + self.m2 * other.m8,
            m3: self.m3 * other.m0 + self.m4 * other.m3 + self.m5 * other.m6,
            m4: self.m3 * other.m1 + self.m4 * other.m4 + self.m5 * other.m7,
            m5: self.m3 * other.m2 + self.m4 * other.m5 + self.m5 * other.m8,
            m6: self.m6 * other.m0 + self.m7 * other.m3 + self.m8 * other.m6,
            m7: self.m6 * other.m1 + self.m7 * other.m4 + self.m8 * other.m7,
            m8: self.m6 * other.m2 + self.m7 * other.m5 + self.m8 * other.m8,
        }
    }

    /// Compute matrix inverse
    pub fn inverse(&self) -> Mat3 {
        let det = self.m0 * (self.m4 * self.m8 - self.m5 * self.m7)
            - self.m1 * (self.m3 * self.m8 - self.m5 * self.m6)
            + self.m2 * (self.m3 * self.m7 - self.m4 * self.m6);

        if det.abs() < 1e-10 {
            return Mat3::identity(); // Fallback to identity if singular
        }

        let inv_det = 1.0 / det;

        Mat3 {
            m0: (self.m4 * self.m8 - self.m5 * self.m7) * inv_det,
            m1: (self.m2 * self.m7 - self.m1 * self.m8) * inv_det,
            m2: (self.m1 * self.m5 - self.m2 * self.m4) * inv_det,
            m3: (self.m5 * self.m6 - self.m3 * self.m8) * inv_det,
            m4: (self.m0 * self.m8 - self.m2 * self.m6) * inv_det,
            m5: (self.m2 * self.m3 - self.m0 * self.m5) * inv_det,
            m6: (self.m3 * self.m7 - self.m4 * self.m6) * inv_det,
            m7: (self.m1 * self.m6 - self.m0 * self.m7) * inv_det,
            m8: (self.m0 * self.m4 - self.m1 * self.m3) * inv_det,
        }
    }

    /// Transform a 2D point: [x, y] -> [x', y']
    pub fn transform_point2(&self, p: &[f32; 2]) -> Vec2D {
        let x = self.m0 * p[0] + self.m1 * p[1] + self.m2;
        let y = self.m3 * p[0] + self.m4 * p[1] + self.m5;
        Vec2D::new(x, y)
    }
}

impl std::ops::Mul for Mat3 {
    type Output = Mat3;

    fn mul(self, other: Mat3) -> Mat3 {
        self.mul(&other)
    }
}

impl std::ops::Mul<&Mat3> for Mat3 {
    type Output = Mat3;

    fn mul(self, other: &Mat3) -> Mat3 {
        Mat3::mul(&self, other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq_mat(a: Mat3, b: Mat3, eps: f32) -> bool {
        (a.m0 - b.m0).abs() <= eps
            && (a.m1 - b.m1).abs() <= eps
            && (a.m2 - b.m2).abs() <= eps
            && (a.m3 - b.m3).abs() <= eps
            && (a.m4 - b.m4).abs() <= eps
            && (a.m5 - b.m5).abs() <= eps
            && (a.m6 - b.m6).abs() <= eps
            && (a.m7 - b.m7).abs() <= eps
            && (a.m8 - b.m8).abs() <= eps
    }

    #[test]
    fn test_mat3_identity() {
        let i = Mat3::identity();
        let p = i.transform_point2(&[5.0, 3.0]);
        assert!((p.x - 5.0).abs() < 1e-6);
        assert!((p.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_translation() {
        let t = Mat3::translation(2.0, 3.0);
        let p = t.transform_point2(&[1.0, 1.0]);
        assert!((p.x - 3.0).abs() < 1e-6);
        assert!((p.y - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_scale() {
        let s = Mat3::scale(2.0, 3.0);
        let p = s.transform_point2(&[1.0, 1.0]);
        assert!((p.x - 2.0).abs() < 1e-6);
        assert!((p.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_multiply() {
        let t = Mat3::translation(1.0, 2.0);
        let s = Mat3::scale(2.0, 2.0);
        let combined = t * s;

        let p = combined.transform_point2(&[1.0, 1.0]);
        // Scale first: (1, 1) -> (2, 2)
        // Translate: (2, 2) -> (3, 4)
        assert!((p.x - 3.0).abs() < 1e-6);
        assert!((p.y - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_inverse() {
        let t = Mat3::translation(5.0, 3.0);
        let inv = t.inverse();
        let p1 = [1.0, 2.0];
        let transformed = t.transform_point2(&p1);
        let back = inv.transform_point2(&[transformed.x, transformed.y]);

        assert!((back.x - p1[0]).abs() < 1e-5);
        assert!((back.y - p1[1]).abs() < 1e-5);
    }
}

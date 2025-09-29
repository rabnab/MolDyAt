use crate::frame::window_definition::WindowDefinition;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallPhysics;
use ggez::Context;
use ggez::graphics::{Color, DrawMode, Mesh, MeshBuilder};

const MIN_BALL_SIZE: f32 = 0.15;
const RADIUS_COMPARISON_EPSILON: f32 = 1e-4; // Extracted constant for the comparison threshold

pub struct NewBallDef {
    ball_size_idx: usize,
    ball_size: Vec<f32>,
    ball_mesh: Vec<Mesh>,
    pub(crate) rand_speed: bool,
}

impl Default for NewBallDef {
    fn default() -> Self {
        // Mirror the size generation used in `new`, but without requiring a Context for meshes.
        const NUM_BALL_SIZES: usize = 5;
        const BASE_EXPONENT: f32 = 2.0;

        let ball_sizes: Vec<f32> = (0..NUM_BALL_SIZES)
            .map(|i| MIN_BALL_SIZE * BASE_EXPONENT.powi(i as i32))
            .collect();

        // We cannot build Mesh without a ggez Context. Keep an empty vector as a placeholder.
        // The drawing pipeline already handles missing meshes by skipping rendering when None.
        // Reserve capacity equal to the number of sizes to emphasize intended 1:1 mapping.
        let ball_mesh: Vec<Mesh> = Vec::with_capacity(ball_sizes.len());

        NewBallDef {
            ball_size: ball_sizes,
            ball_size_idx: 0, // Start with the smallest ball size
            ball_mesh,
            rand_speed: false,
        }
    }
}

impl NewBallDef {
    pub(crate) fn get_mesh(&self, ball_radius: f32) -> Option<&Mesh> {
        let idx = self.find_ball_index_by_radius(ball_radius)?;
        self.ball_mesh.get(idx)
    }

    pub(crate) fn new(ctx: &Context, ball_physics: &BallPhysics) -> NewBallDef {
        const NUM_BALL_SIZES: usize = 5; // Number of ball sizes to create
        const CIRCLE_TOLERANCE: f32 = 0.1; // Tolerance for circle drawing
        const STROKE_THICKNESS: f32 = 2.0; // Border thickness for stroke drawing
        const BASE_EXPONENT: f32 = 2.0; // Base scaling exponent for sizes

        let ball_sizes: Vec<f32> = (0..NUM_BALL_SIZES)
            .map(|i| MIN_BALL_SIZE * BASE_EXPONENT.powi(i as i32))
            .collect();

        let ball_meshes: Vec<Mesh> = ball_sizes
            .iter()
            .map(|&radius| {
                Self::create_ball_mesh(
                    ctx,
                    radius,
                    ball_physics.equilibrium_of_radius(radius),
                    CIRCLE_TOLERANCE,
                    STROKE_THICKNESS,
                )
            })
            .collect();

        NewBallDef {
            ball_size: ball_sizes,
            ball_size_idx: 2,
            ball_mesh: ball_meshes,
            rand_speed: false,
        }
    }

    /// Helper function to create the mesh for a ball
    fn create_ball_mesh(
        ctx: &Context,
        ball_size: f32,
        ball_eq_radius: f32,
        tolerance: f32,
        stroke_thickness: f32,
    ) -> Mesh {
        let mut builder = MeshBuilder::new();

        // Inner filled circle
        builder
            .circle(
                DrawMode::fill(),
                Vec2D::new(0.0, 0.0),
                WindowDefinition::len_to_px(ball_size),
                tolerance,
                Color::RED,
            )
            .unwrap();

        // Outer stroked circle
        builder
            .circle(
                DrawMode::stroke(stroke_thickness),
                Vec2D::new(0.0, 0.0),
                WindowDefinition::len_to_px(ball_eq_radius),
                tolerance,
                Color::MAGENTA,
            )
            .unwrap();

        // Outer stroked circle

        let cutoff = 4_f32;
        builder
            .circle(
                DrawMode::stroke(stroke_thickness),
                Vec2D::new(0.0, 0.0),
                WindowDefinition::len_to_px(ball_eq_radius + cutoff),
                tolerance,
                Color::from_rgb(110, 110, 110),
            )
            .unwrap();

        let ball_mesh_data = builder.build();
        Mesh::from_data(ctx, ball_mesh_data)
    }

    fn find_ball_index_by_radius(&self, target_radius: f32) -> Option<usize> {
        self.ball_size.iter().position(|&r| {
            let difference = (r - target_radius).abs(); // Introduced variable for readability
            difference < RADIUS_COMPARISON_EPSILON
        })
    }

    pub(crate) fn get_ball_size(&self) -> f32 {
        self.ball_size[self.ball_size_idx]
    }

    pub(crate) fn get_ball_mass(&self) -> f32 {
        let density = 2. / 10.;
        1. + density * (self.get_ball_size() / MIN_BALL_SIZE - 1.).powf(1.2)
        // .powi(2)
    }

    pub(crate) fn next_ball_size(&mut self) {
        self.ball_size_idx = (self.ball_size_idx + 1) % self.ball_size.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let default_ball_def = NewBallDef::default();
        assert_eq!(default_ball_def.get_ball_size(), MIN_BALL_SIZE);
        assert_eq!(default_ball_def.get_ball_mass(), 1.);
    }

    #[test]
    fn test_next_ball_size_wraps_and_monotonic_mass() {
        const EPS: f32 = 1e-6;
        let mut def = NewBallDef::default();
        let mut seen_sizes = Vec::new();
        let mut last_mass = def.get_ball_mass();
        let n = def.ball_size.len();
        // Cycle through all sizes to test monotonicity within one full cycle
        for i in 0..n {
            let size = def.get_ball_size();
            seen_sizes.push(size);
            let mass = def.get_ball_mass();
            // Mass should be non-decreasing within the cycle
            assert!(
                mass >= last_mass - EPS,
                "mass not monotonic at step {}: {} < {}",
                i,
                mass,
                last_mass
            );
            last_mass = mass;
            def.next_ball_size();
        }
        // After one more next, we should wrap to the first size
        let first_size = seen_sizes[0];
        let first_mass = 1.0; // from default mass formula at smallest size
        assert_eq!(def.get_ball_size(), first_size);
        assert!((def.get_ball_mass() - first_mass).abs() < EPS);
        // Default sizes should follow geometric progression with base 2

        for i in 1..seen_sizes.len().min(5) {
            assert!(
                (seen_sizes[i] - seen_sizes[i - 1] * 2.0).abs() < EPS,
                "sizes not doubling: {} vs {}",
                seen_sizes[i],
                seen_sizes[i - 1] * 2.0
            );
        }
    }
}

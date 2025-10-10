// const RADIUS_COMPARISON_EPSILON: f32 = 1e-4; // Extracted constant for the comparison threshold
//
// pub struct NewBallDef {
//     ball_size_idx: usize,
//     ball_size: Vec<f32>,
//     pub(crate) rand_speed: bool,
// }
//
// impl Default for NewBallDef {
//     fn default() -> Self {
//         // Mirror the size generation used in `new`, but without requiring a Context for meshes.
//         const NUM_BALL_SIZES: usize = 5;
//         const BASE_EXPONENT: f32 = 2.0;
//
//         let ball_sizes: Vec<f32> = (0..NUM_BALL_SIZES)
//             .map(|i| MIN_BALL_SIZE * BASE_EXPONENT.powi(i as i32))
//             .collect();
//
//         NewBallDef {
//             ball_size: ball_sizes,
//             ball_size_idx: 0, // Start with the smallest ball size
//             rand_speed: false,
//         }
//     }
// }
//
// impl NewBallDef {
//     pub(crate) fn new(ball_physics: &BallPhysics) -> NewBallDef {
//         const NUM_BALL_SIZES: usize = 5; // Number of ball sizes to create
//
//         const BASE_EXPONENT: f32 = 2.0; // Base scaling exponent for sizes
//
//         let ball_sizes: Vec<f32> = (0..NUM_BALL_SIZES)
//             .map(|i| MIN_BALL_SIZE * BASE_EXPONENT.powi(i as i32))
//             .collect();
//
//         NewBallDef {
//             ball_size: ball_sizes,
//             ball_size_idx: 2,
//             rand_speed: false,
//         }
//     }
//
//     fn find_ball_index_by_radius(&self, target_radius: f32) -> Option<usize> {
//         self.ball_size.iter().position(|&r| {
//             let difference = (r - target_radius).abs(); // Introduced variable for readability
//             difference < RADIUS_COMPARISON_EPSILON
//         })
//     }
//
//     pub(crate) fn get_ball_size(&self) -> f32 {
//         self.ball_size[self.ball_size_idx]
//     }
//
//     pub(crate) fn get_ball_mass(&self) -> f32 {
//         Self::get_ball_mass_of_size(self.get_ball_size())
//     }
//
//     /// Get ball size by index (0..4), returning MIN_BALL_SIZE if index is out of bounds
// pub(crate) fn get_ball_size_of_idx(&self, idx: usize) -> f32 {
//     *self.ball_size.get(idx).unwrap_or(&MIN_BALL_SIZE)
// }

// pub(crate) fn get_ball_mass_of_size(size: f32) -> f32 {
//     let density = 2. / 10.;
//     1. + density * (size / MIN_BALL_SIZE - 1.).powf(1.2)
// }
//
// pub(crate) fn next_ball_size(&mut self) {
//     self.ball_size_idx = (self.ball_size_idx + 1) % self.ball_size.len();
// }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_defaults() {
//         let default_ball_def = NewBallDef::default();
//         assert_eq!(default_ball_def.get_ball_size(), MIN_BALL_SIZE);
//         assert_eq!(default_ball_def.get_ball_mass(), 1.);
//     }
//
//     #[test]
//     fn test_next_ball_size_wraps_and_monotonic_mass() {
//         const EPS: f32 = 1e-6;
//         let mut def = NewBallDef::default();
//         let mut seen_sizes = Vec::new();
//         let mut last_mass = def.get_ball_mass();
//         let n = def.ball_size.len();
//         // Cycle through all sizes to test monotonicity within one full cycle
//         for i in 0..n {
//             let size = def.get_ball_size();
//             seen_sizes.push(size);
//             let mass = def.get_ball_mass();
//             // Mass should be non-decreasing within the cycle
//             assert!(
//                 mass >= last_mass - EPS,
//                 "mass not monotonic at step {}: {} < {}",
//                 i,
//                 mass,
//                 last_mass
//             );
//             last_mass = mass;
//             def.next_ball_size();
//         }
//         // After one more next, we should wrap to the first size
//         let first_size = seen_sizes[0];
//         let first_mass = 1.0; // from default mass formula at smallest size
//         assert_eq!(def.get_ball_size(), first_size);
//         assert!((def.get_ball_mass() - first_mass).abs() < EPS);
//         // Default sizes should follow geometric progression with base 2
//
//         for i in 1..seen_sizes.len().min(5) {
//             assert!(
//                 (seen_sizes[i] - seen_sizes[i - 1] * 2.0).abs() < EPS,
//                 "sizes not doubling: {} vs {}",
//                 seen_sizes[i],
//                 seen_sizes[i - 1] * 2.0
//             );
//         }
//     }
// }

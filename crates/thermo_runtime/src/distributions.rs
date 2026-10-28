// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Equilibrium statistics of the balls (#343, plan item V6): the speed
//! histogram next to the 2D Maxwell–Boltzmann distribution at the current
//! temperature, and the radial distribution function g(r) per species pair.
//!
//! Both are computed from a snapshot's balls, off the physics step and only
//! when asked for (`--distributions`, the distributions export), so a run
//! without them pays nothing. All loops are serial and in ball order, so
//! the results do not depend on the thread count.

use crate::application::renderer_snapshot::{BallRenderData, RendererSnapshot};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{SpeciesId, SpeciesSet};

/// Bins of the speed histogram, from 0 to the largest speed.
pub const SPEED_BINS: usize = 32;
/// Largest distance of g(r), in reduced units (`σ` of `N2`): past the
/// default pair cutoff of 4 σ.
pub const RDF_MAX_DISTANCE: f32 = 5.0;
/// Bins of g(r) from 0 to [`RDF_MAX_DISTANCE`].
pub const RDF_BINS: usize = 50;

/// The speed histogram and the 2D Maxwell–Boltzmann counts expected in the
/// same bins.
///
/// Speeds are taken in the centre-of-mass frame, and the temperature is the
/// simulation's (`Σ m |v - v_com|² / (2 (N - 1))`, `k_B = 1`). In 2D a
/// ball of mass `m` has the speed density `(m v / T) exp(-m v² / 2T)`, so
/// the expected count of a bin `[a, b)` is `Σ_balls exp(-m a²/2T) -
/// exp(-m b²/2T)`, exact per bin and correct for mixed masses.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedHistogram {
    pub bin_width: f32,
    /// Balls per bin.
    pub counts: Vec<u32>,
    /// Maxwell–Boltzmann count per bin at `temperature`.
    pub expected: Vec<f32>,
    pub temperature: f32,
}

impl SpeedHistogram {
    /// Speed at the middle of bin `index`.
    pub fn bin_centre(&self, index: usize) -> f32 {
        (index as f32 + 0.5) * self.bin_width
    }
}

/// g(r) of one species pair.
#[derive(Debug, Clone, PartialEq)]
pub struct PairDistribution {
    /// Species of the reference balls, then of their neighbours.
    pub species: (SpeciesId, SpeciesId),
    /// Reference balls used: those at least [`RDF_MAX_DISTANCE`] from every
    /// wall of the world, so no shell is cut off by a wall.
    pub reference_balls: usize,
    /// g per bin; NaN without reference balls or neighbours.
    pub g: Vec<f32>,
}

/// g(r) of every species pair present.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialDistribution {
    pub bin_width: f32,
    /// One entry per pair `(a, b)` with `a <= b`, in species table order.
    pub pairs: Vec<PairDistribution>,
}

impl RadialDistribution {
    /// Distance at the middle of bin `index`.
    pub fn bin_centre(&self, index: usize) -> f32 {
        (index as f32 + 0.5) * self.bin_width
    }
}

/// Both distributions of one snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct Distributions {
    /// `None` with fewer than two balls or all at rest.
    pub speed: Option<SpeedHistogram>,
    pub radial: RadialDistribution,
}

/// The distributions of `snapshot`'s balls (it needs the `BALLS` layer).
pub fn distributions(snapshot: &RendererSnapshot) -> Distributions {
    Distributions {
        speed: speed_histogram(&snapshot.balls, SPEED_BINS),
        radial: radial_distribution(
            &snapshot.balls,
            &snapshot.frame.world_rect,
            RDF_MAX_DISTANCE,
            RDF_BINS,
        ),
    }
}

/// The speed histogram of `balls` in `bins` bins up to the largest speed.
pub fn speed_histogram(balls: &[BallRenderData], bins: usize) -> Option<SpeedHistogram> {
    if balls.len() < 2 || bins == 0 {
        return None;
    }
    let total_mass: f32 = balls.iter().map(|b| b.mass).sum();
    if total_mass <= 0.0 {
        return None;
    }
    let (px, py) = balls.iter().fold((0.0, 0.0), |(x, y), b| {
        (x + b.mass * b.velocity.x, y + b.mass * b.velocity.y)
    });
    let (cx, cy) = (px / total_mass, py / total_mass);
    let speed = |b: &BallRenderData| (b.velocity.x - cx).hypot(b.velocity.y - cy);
    let twice_kinetic: f32 = balls.iter().map(|b| b.mass * speed(b).powi(2)).sum();
    let temperature = twice_kinetic / (2.0 * (balls.len() - 1) as f32);
    let max_speed = balls.iter().map(speed).fold(0.0_f32, f32::max);
    if temperature <= 0.0 || max_speed <= 0.0 {
        return None;
    }
    // A hair wider than the fastest ball, so it falls into the last bin.
    let bin_width = max_speed * (1.0 + 1e-6) / bins as f32;
    let mut counts = vec![0; bins];
    for ball in balls {
        let bin = ((speed(ball) / bin_width) as usize).min(bins - 1);
        counts[bin] += 1;
    }
    // Balls per mass: the expected counts need one term per mass, not per ball.
    let mut masses: Vec<(f32, u32)> = Vec::new();
    for ball in balls {
        match masses.iter_mut().find(|(m, _)| *m == ball.mass) {
            Some((_, n)) => *n += 1,
            None => masses.push((ball.mass, 1)),
        }
    }
    let survival = |v: f32| -> f32 {
        masses
            .iter()
            .map(|&(m, n)| n as f32 * (-m * v * v / (2.0 * temperature)).exp())
            .sum()
    };
    let expected = (0..bins)
        .map(|i| survival(i as f32 * bin_width) - survival((i + 1) as f32 * bin_width))
        .collect();
    Some(SpeedHistogram {
        bin_width,
        counts,
        expected,
        temperature,
    })
}

/// g(r) of every species pair among `balls` in `world`, up to `r_max` in
/// `bins` bins.
///
/// For a pair `(a, b)`: `g(r) = n(r) / (N_ref ρ_b 2π r dr)`, with `n(r)` the
/// `b` balls counted in the shell around the `N_ref` reference balls of `a`
/// and `ρ_b` the number density of `b` (one less for `a = b`, since a ball
/// is not its own neighbour). Only reference balls at least `r_max` from
/// every wall count, so every shell lies inside the world; an ideal gas
/// then gives g ≈ 1 at every r.
///
/// `ρ_b` is the mean density of the whole world: g is normalised for a
/// world of uniform density. Where the density varies on its own (a gas
/// stratified under gravity, chambers split by walls or valves at
/// different densities), g levels off above 1 at large r even without
/// interactions; read that plateau as the density variation, not as
/// structure. No bins (`bins == 0` or `r_max <= 0`) give no pairs.
pub fn radial_distribution(
    balls: &[BallRenderData],
    world: &Rect,
    r_max: f32,
    bins: usize,
) -> RadialDistribution {
    if bins == 0 || r_max.is_nan() || r_max <= 0.0 {
        return RadialDistribution {
            bin_width: 0.0,
            pairs: Vec::new(),
        };
    }
    let bin_width = r_max / bins as f32;
    let mut counts_per_species = [0usize; SpeciesSet::CAPACITY];
    for ball in balls {
        if let Some(count) = counts_per_species.get_mut(ball.species.index()) {
            *count += 1;
        }
    }
    let present: Vec<usize> = (0..SpeciesSet::CAPACITY)
        .filter(|&i| counts_per_species[i] > 0)
        .collect();
    let (x0, x1) = (
        world.x.min(world.x + world.w),
        world.x.max(world.x + world.w),
    );
    let (y0, y1) = (
        world.y.min(world.y + world.h),
        world.y.max(world.y + world.h),
    );
    let area = (x1 - x0) * (y1 - y0);
    let grid = BallGrid::new(balls, x0, y0, x1, y1, r_max);
    let interior = |b: &BallRenderData| {
        b.position.x - x0 >= r_max
            && x1 - b.position.x >= r_max
            && b.position.y - y0 >= r_max
            && y1 - b.position.y >= r_max
    };

    // One neighbour pass for every pair: counts per (reference species,
    // neighbour species), by place in `present`, are added to the pair
    // with the smaller species first (as `a <= b` below).
    let p = present.len();
    let place = |species: SpeciesId| present.iter().position(|&s| s == species.index());
    let mut hist = vec![0u64; p * p * bins];
    let mut references = vec![0usize; p];
    for (i, ball) in balls.iter().enumerate() {
        let Some(a) = place(ball.species).filter(|_| interior(ball)) else {
            continue;
        };
        references[a] += 1;
        grid.for_each_near(ball, |j, other| {
            if j == i {
                return;
            }
            let Some(b) = place(other.species).filter(|&b| b >= a) else {
                return;
            };
            let r = (other.position.x - ball.position.x).hypot(other.position.y - ball.position.y);
            if r < r_max {
                hist[(a * p + b) * bins + ((r / bin_width) as usize).min(bins - 1)] += 1;
            }
        });
    }

    let mut pairs = Vec::new();
    for a in 0..p {
        for b in a..p {
            let neighbours = counts_per_species[present[b]] - usize::from(a == b);
            let density = neighbours as f32 / area;
            let counts = &hist[(a * p + b) * bins..][..bins];
            let g = (0..bins)
                .map(|i| {
                    let (r0, r1) = (i as f32 * bin_width, (i + 1) as f32 * bin_width);
                    let shell = std::f32::consts::PI * (r1 * r1 - r0 * r0);
                    let ideal = references[a] as f32 * density * shell;
                    if ideal > 0.0 {
                        counts[i] as f32 / ideal
                    } else {
                        f32::NAN
                    }
                })
                .collect();
            let id = |index: usize| SpeciesId::new(present[index] as u16);
            pairs.push(PairDistribution {
                species: (id(a), id(b)),
                reference_balls: references[a],
                g,
            });
        }
    }
    RadialDistribution { bin_width, pairs }
}

/// The balls sorted into square cells of at least `r_max`, so the
/// neighbours within `r_max` of a ball are in its cell and the 8 around it.
struct BallGrid<'a> {
    balls: &'a [BallRenderData],
    /// Ball indices per cell, row-major.
    cells: Vec<Vec<usize>>,
    cols: usize,
    rows: usize,
    x0: f32,
    y0: f32,
    cell: f32,
}

impl<'a> BallGrid<'a> {
    fn new(balls: &'a [BallRenderData], x0: f32, y0: f32, x1: f32, y1: f32, r_max: f32) -> Self {
        let cols = (((x1 - x0) / r_max).floor() as usize).max(1);
        let rows = (((y1 - y0) / r_max).floor() as usize).max(1);
        let cell = ((x1 - x0) / cols as f32).max((y1 - y0) / rows as f32);
        let mut grid = Self {
            balls,
            cells: vec![Vec::new(); cols * rows],
            cols,
            rows,
            x0,
            y0,
            cell,
        };
        for (i, ball) in balls.iter().enumerate() {
            let (c, r) = grid.cell_of(ball);
            grid.cells[r * cols + c].push(i);
        }
        grid
    }

    fn cell_of(&self, ball: &BallRenderData) -> (usize, usize) {
        let index = |offset: f32, n: usize| ((offset / self.cell).max(0.0) as usize).min(n - 1);
        (
            index(ball.position.x - self.x0, self.cols),
            index(ball.position.y - self.y0, self.rows),
        )
    }

    /// `f(index, ball)` for every ball in `ball`'s cell and the 8 around it.
    fn for_each_near(&self, ball: &BallRenderData, mut f: impl FnMut(usize, &BallRenderData)) {
        let (c, r) = self.cell_of(ball);
        for row in r.saturating_sub(1)..=(r + 1).min(self.rows - 1) {
            for col in c.saturating_sub(1)..=(c + 1).min(self.cols - 1) {
                for &j in &self.cells[row * self.cols + col] {
                    f(j, &self.balls[j]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

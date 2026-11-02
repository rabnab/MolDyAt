// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Colour scale of the scalar field overlay, relative to the world's
//! averages (#402).
//!
//! Each field is shown against a reference taken from the whole grid, so its
//! colours mean the same whatever the state of the world:
//!
//! | Field       | Reference                         | Colour range       |
//! |-------------|-----------------------------------|--------------------|
//! | Temperature | `T̄`, density-weighted mean        | `0 … 2 T̄`          |
//! | Density     | `ρ̄`, mean over all cells          | `0 … 2 ρ̄`          |
//! | Flow speed  | thermal speed `v_th = √T̄` (m = 1) | `0 … v_th`         |
//! | Pressure    | `P̄`, mean over cells with a value | `P̄ ± ρ̄ T̄`          |
//!
//! The pressure band is the ideal-gas pressure `ρ̄ T̄`, so it stays finite
//! when the mean pressure is near zero. The references follow the world
//! slowly ([`REFERENCE_SMOOTHING_SECS`]), and a lock (`view.lock_field_scale`)
//! freezes them so changes can be compared against a fixed scale.

use std::time::Instant;
use thermo_physics::application::thermo_field::CellFields;

use super::state_layers::ScalarField;

/// Time constant (seconds) of the moving average of the references: slow
/// enough that the colours don't breathe with noise, fast enough to follow
/// heating or compression.
pub const REFERENCE_SMOOTHING_SECS: f32 = 2.0;

/// Smallest reference (and colour range width) used, so an empty or frozen
/// world never divides by zero.
const MIN_REFERENCE: f32 = 1e-6;

/// The world's averages the field colours are relative to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldReference {
    /// Mean temperature, weighted by the cells' density (≈ the global
    /// kinetic temperature).
    pub temperature: f32,
    /// Mean number density over all cells (balls per unit area).
    pub density: f32,
    /// Mean pressure over the cells that have one.
    pub pressure: f32,
    /// Frozen by the lock key.
    pub locked: bool,
}

/// What the heatmap and its colour bar show for one field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldScale {
    /// Values (reduced units) mapped to the ends of the colour scale.
    pub range: (f32, f32),
    /// The reference value the range is built from.
    pub reference: f32,
    /// What `reference` is: `avg` (a mean) or `vth` (thermal speed).
    pub reference_name: &'static str,
    pub locked: bool,
}

impl FieldReference {
    /// The averages of `cells`; `None` while no cell has a temperature (no
    /// balls, or only lone balls in their cells).
    pub fn from_cells(cells: &CellFields) -> Option<Self> {
        let density = cells.density.as_slice();
        let temperature = cells.temperature.as_slice();
        let pressure = cells.pressure.as_slice();

        let (mut weighted_t, mut weight) = (0.0_f64, 0.0_f64);
        for (&t, &rho) in temperature.iter().zip(density) {
            if t.is_finite() && rho.is_finite() {
                weighted_t += f64::from(t) * f64::from(rho);
                weight += f64::from(rho);
            }
        }
        if weight <= 0.0 {
            return None;
        }

        let mean = |values: &[f32]| {
            let (sum, n) = values
                .iter()
                .filter(|v| v.is_finite())
                .fold((0.0_f64, 0usize), |(s, n), &v| (s + f64::from(v), n + 1));
            (n > 0).then(|| (sum / n as f64) as f32)
        };
        Some(Self {
            temperature: (weighted_t / weight) as f32,
            density: mean(density).unwrap_or(0.0),
            pressure: mean(pressure).unwrap_or(0.0),
            locked: false,
        })
    }

    /// The colour scale of `field` against these averages.
    pub fn scale(&self, field: ScalarField) -> FieldScale {
        let t = self.temperature.max(MIN_REFERENCE);
        let rho = self.density.max(MIN_REFERENCE);
        let (range, reference, reference_name) = match field {
            ScalarField::Temperature => ((0.0, 2.0 * t), t, "avg"),
            ScalarField::Density => ((0.0, 2.0 * rho), rho, "avg"),
            ScalarField::FlowSpeed => {
                let v_th = t.sqrt();
                ((0.0, v_th), v_th, "vth")
            }
            ScalarField::Pressure => {
                let band = rho * t;
                let p = self.pressure;
                ((p - band, p + band), p, "avg")
            }
        };
        FieldScale {
            range,
            reference,
            reference_name,
            locked: self.locked,
        }
    }

    /// Move each average towards `target` by `alpha` (0 keeps, 1 jumps).
    fn blend(&mut self, target: &Self, alpha: f32) {
        self.temperature += (target.temperature - self.temperature) * alpha;
        self.density += (target.density - self.density) * alpha;
        self.pressure += (target.pressure - self.pressure) * alpha;
    }
}

/// Keeps the references of the field overlay: smoothed over time, or frozen
/// while locked.
#[derive(Debug, Clone, Default)]
pub struct FieldReferenceTracker {
    current: Option<FieldReference>,
    /// World the references belong to (`FrameInfo::world_generation`); a
    /// new world starts them afresh unless they are locked.
    generation: u64,
    last: Option<Instant>,
}

impl FieldReferenceTracker {
    /// Take the averages of `cells` (of world `generation`) at `now` into
    /// account and return the references to draw with. While `locked`, the
    /// references stay as they are (the first ones are taken as they come).
    pub fn update(
        &mut self,
        cells: &CellFields,
        generation: u64,
        locked: bool,
        now: Instant,
    ) -> Option<FieldReference> {
        let dt = self
            .last
            .map_or(f32::INFINITY, |last| now.duration_since(last).as_secs_f32());
        self.last = Some(now);

        let frozen = locked && self.current.is_some();
        if !frozen && let Some(target) = FieldReference::from_cells(cells) {
            match &mut self.current {
                Some(current) if generation == self.generation => {
                    // Weight from the elapsed time: frame-rate independent,
                    // and after a long pause it snaps to the new values.
                    current.blend(&target, 1.0 - (-dt / REFERENCE_SMOOTHING_SECS).exp());
                }
                _ => self.current = Some(target),
            }
            self.generation = generation;
        }
        self.current.map(|reference| FieldReference {
            locked,
            ..reference
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A 1 x 2 grid: one cell with `(t, rho, p)`, one empty cell.
    fn cells(t: f32, rho: f32, p: f32) -> CellFields {
        let mut cells = CellFields::filled(1, 2, f32::NAN);
        cells.temperature.set(0, 0, t);
        cells.density.set(0, 0, rho);
        cells.density.set(0, 1, 0.0);
        cells.pressure.set(0, 0, p);
        cells
    }

    #[test]
    fn references_are_the_density_weighted_temperature_and_the_cell_means() {
        let mut fields = CellFields::filled(1, 2, 0.0);
        fields.temperature.set(0, 0, 1.0);
        fields.temperature.set(0, 1, 4.0);
        fields.density.set(0, 0, 0.6);
        fields.density.set(0, 1, 0.2);
        fields.pressure.set(0, 0, 1.0);
        fields.pressure.set(0, 1, f32::NAN);
        let r = FieldReference::from_cells(&fields).unwrap();
        assert!((r.temperature - (0.6 + 0.8) / 0.8).abs() < 1e-6);
        assert!((r.density - 0.4).abs() < 1e-6);
        assert_eq!(r.pressure, 1.0);
    }

    #[test]
    fn no_reference_without_a_temperature() {
        assert!(FieldReference::from_cells(&CellFields::filled(2, 2, f32::NAN)).is_none());
        assert!(FieldReference::from_cells(&CellFields::empty()).is_none());
    }

    #[test]
    fn scales_are_relative_to_the_references() {
        let r = FieldReference {
            temperature: 4.0,
            density: 0.5,
            pressure: -1.0,
            locked: false,
        };
        assert_eq!(r.scale(ScalarField::Temperature).range, (0.0, 8.0));
        assert_eq!(r.scale(ScalarField::Density).range, (0.0, 1.0));
        let flow = r.scale(ScalarField::FlowSpeed);
        assert_eq!((flow.range, flow.reference_name), ((0.0, 2.0), "vth"));
        // P̄ ± ρ̄ T̄
        let p = r.scale(ScalarField::Pressure);
        assert_eq!((p.range, p.reference), ((-3.0, 1.0), -1.0));
    }

    #[test]
    fn a_cold_world_still_has_a_range() {
        let r = FieldReference {
            temperature: 0.0,
            density: 0.0,
            pressure: 0.0,
            locked: false,
        };
        for field in [
            ScalarField::Temperature,
            ScalarField::Pressure,
            ScalarField::Density,
            ScalarField::FlowSpeed,
        ] {
            let (low, high) = r.scale(field).range;
            assert!(high > low, "{field:?}");
        }
    }

    #[test]
    fn tracker_follows_the_world_smoothly() {
        let mut tracker = FieldReferenceTracker::default();
        let t0 = Instant::now();
        let first = tracker.update(&cells(1.0, 1.0, 1.0), 0, false, t0).unwrap();
        assert_eq!(first.temperature, 1.0);
        let step = Duration::from_secs_f32(REFERENCE_SMOOTHING_SECS);
        let next = tracker
            .update(&cells(3.0, 1.0, 1.0), 0, false, t0 + step)
            .unwrap();
        // One time constant: 1 - 1/e of the way.
        let expected = 1.0 + 2.0 * (1.0 - (-1.0_f32).exp());
        assert!((next.temperature - expected).abs() < 1e-5);
    }

    #[test]
    fn tracker_holds_the_references_while_locked() {
        let mut tracker = FieldReferenceTracker::default();
        let t0 = Instant::now();
        tracker.update(&cells(1.0, 1.0, 1.0), 0, false, t0);
        let later = t0 + Duration::from_secs(60);
        let locked = tracker
            .update(&cells(5.0, 1.0, 1.0), 0, true, later)
            .unwrap();
        assert_eq!((locked.temperature, locked.locked), (1.0, true));
        // Released: after a long time it has caught up.
        let released = tracker
            .update(
                &cells(5.0, 1.0, 1.0),
                0,
                false,
                later + Duration::from_secs(60),
            )
            .unwrap();
        assert!((released.temperature - 5.0).abs() < 1e-3);
        assert!(!released.locked);
    }

    #[test]
    fn tracker_restarts_for_a_new_world() {
        let mut tracker = FieldReferenceTracker::default();
        let t0 = Instant::now();
        tracker.update(&cells(1.0, 1.0, 1.0), 0, false, t0);
        let next = tracker
            .update(
                &cells(5.0, 1.0, 1.0),
                1,
                false,
                t0 + Duration::from_millis(10),
            )
            .unwrap();
        assert_eq!(next.temperature, 5.0);
    }

    #[test]
    fn tracker_keeps_the_last_references_when_the_world_empties() {
        let mut tracker = FieldReferenceTracker::default();
        let t0 = Instant::now();
        tracker.update(&cells(2.0, 1.0, 1.0), 0, false, t0);
        let empty = CellFields::filled(1, 2, f32::NAN);
        let kept = tracker
            .update(&empty, 0, false, t0 + Duration::from_secs(1))
            .unwrap();
        assert_eq!(kept.temperature, 2.0);
    }
}

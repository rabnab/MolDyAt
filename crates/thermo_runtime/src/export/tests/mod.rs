// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::CreateSnapshot;
use crate::metrics::{MetricsRegistry, SeriesMetadata};
use thermo_physics::test_support::create_main_content;

mod files;
mod series;
mod snapshot;

/// Split one CSV line into cells, honouring `"` quoting.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => cells.push(std::mem::take(&mut cell)),
            (c, _) => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    text.lines().map(parse_csv_line).collect()
}

fn meta(unit: Option<&'static str>, per_species: bool) -> SeriesMetadata {
    SeriesMetadata {
        unit,
        description: "test series",
        is_per_species: per_species,
    }
}

fn stepped_registry() -> MetricsRegistry {
    let mut registry = MetricsRegistry::new(100);
    // Three samples; `late` is registered at the second one.
    for (i, step) in [10u64, 20, 30].into_iter().enumerate() {
        if i == 1 {
            registry.register_series("late".into(), meta(None, false));
        }
        registry.push_step_for_test(step);
        registry.record("E_kin", i as f32 + 0.5);
        registry.record("E_pot", -(i as f32));
        if i >= 1 {
            registry.record("late", i as f32 * 100.0);
        }
    }
    registry
}

fn snapshot_with_balls() -> crate::application::renderer_snapshot::RendererSnapshot {
    let (mut content, _) = create_main_content();
    content.add_random_candidates(5);
    content.create_snapshot()
}

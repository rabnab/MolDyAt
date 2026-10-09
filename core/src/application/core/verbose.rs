// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
// use crate::application::SimulationToggler;

impl MainContent {
    pub(crate) fn start_handling_verbosity_input(&mut self) {
        println!("Enter a ball ID (or 'a' to set all balls to verbose) followed by mode ([cmos]):");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let input_trimed = input.trim();
            let mut mode = VerboseInfoMode::Main;
            let mut particle_indicator = input_trimed;
            if input_trimed.len() >= 2 {
                match input_trimed.chars().last() {
                    Some('c') => mode = VerboseInfoMode::Console,
                    Some('m') => mode = VerboseInfoMode::Main,
                    Some('o') => mode = VerboseInfoMode::Other,
                    Some('s') => mode = VerboseInfoMode::Speed,
                    _ => {}
                }
                (particle_indicator, _) = input_trimed.split_at(input_trimed.len() - 1);
            }

            if let Ok(number) = particle_indicator.parse::<usize>() {
                match number {
                    0 => {
                        self.clear_verbose_balls();
                    }
                    _ => {
                        self.toggle_ball_verbose(number, mode);
                    }
                }
            } else if particle_indicator.trim() == "a" {
                self.make_all_verbose(mode);
            }
        }
    }
    fn toggle_ball_internal(ball_found: &mut Ball2D, mode: &VerboseInfoMode) {
        match mode {
            VerboseInfoMode::Console => ball_found.verbose.toggle_console(),
            VerboseInfoMode::Main => ball_found.verbose.toggle_main_neighbour(),
            VerboseInfoMode::Other => ball_found.verbose.toggle_other_neighbour(),
            VerboseInfoMode::Speed => ball_found.verbose.toggle_speed(),
        }
    }

    pub(crate) fn make_all_verbose(&mut self, mode: VerboseInfoMode) {
        for b in &mut self.world.balls {
            Self::toggle_ball_internal(b, &mode);
        }
    }
    fn toggle_ball_verbose(&mut self, number: usize, mode: VerboseInfoMode) {
        if let Some(ball) = self.world.balls.iter_mut().find(|b| b.get_id() == number) {
            Self::toggle_ball_internal(ball, &mode);
        }
    }
    fn clear_verbose_balls(&mut self) {
        for b in &mut self.world.balls {
            b.verbose.clear_all();
        }
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
// use crate::application::SimulationToggler;

impl MainContent {
    pub(crate) fn start_handling_verbosity_input(&mut self) {
        // Not a log record: this prompt asks for a line on stdin, so it must
        // show whatever the log filter is. Interactive (window) runs only.
        #[allow(clippy::print_stdout)]
        {
            println!(
                "Enter a ball ID (or 'a' to set all balls to verbose) followed by mode ([cmos]):"
            );
        }
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let console_enabled = self.apply_verbosity_input(&input);
            if console_enabled && !crate::logging::verbose_enabled() {
                log::warn!(
                    "console verbose output is hidden by the log filter; \
                     start with RUST_LOG=info,{}=debug to see it",
                    crate::logging::VERBOSE_TARGET
                );
            }
        }
    }

    /// Applies one line of verbosity input (`<ball id | a | 0><mode>`).
    /// Returns true if it switched console mode on for at least one ball.
    pub(crate) fn apply_verbosity_input(&mut self, input: &str) -> bool {
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
                    false
                }
                _ => self.toggle_ball_verbose(number, mode),
            }
        } else if particle_indicator.trim() == "a" {
            self.make_all_verbose(mode)
        } else {
            false
        }
    }

    /// Returns true if this switched console mode on for the ball.
    fn toggle_ball_internal(ball_found: &mut Ball2D, mode: &VerboseInfoMode) -> bool {
        match mode {
            VerboseInfoMode::Console => {
                ball_found.verbose.toggle_console();
                return ball_found.verbose.console;
            }
            VerboseInfoMode::Main => ball_found.verbose.toggle_main_neighbour(),
            VerboseInfoMode::Other => ball_found.verbose.toggle_other_neighbour(),
            VerboseInfoMode::Speed => ball_found.verbose.toggle_speed(),
        }
        false
    }

    /// Returns true if this switched console mode on for at least one ball.
    pub(crate) fn make_all_verbose(&mut self, mode: VerboseInfoMode) -> bool {
        let mut console_enabled = false;
        for b in &mut self.world.balls {
            console_enabled |= Self::toggle_ball_internal(b, &mode);
        }
        console_enabled
    }
    /// Returns true if this switched console mode on for the ball.
    fn toggle_ball_verbose(&mut self, number: usize, mode: VerboseInfoMode) -> bool {
        self.world
            .balls
            .iter_mut()
            .find(|b| b.get_id() == number)
            .is_some_and(|ball| Self::toggle_ball_internal(ball, &mode))
    }
    fn clear_verbose_balls(&mut self) {
        for b in &mut self.world.balls {
            b.verbose.clear_all();
        }
    }
}

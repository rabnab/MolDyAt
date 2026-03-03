use crate::application::{LogStuff, MainContent};
use crate::objects2d::ball_impl::{Ball2D, Named, get_update_hood_call_cnt};

impl LogStuff for MainContent {
    fn any_log(&self) -> bool {
        self.show_timings //|| self.show_ball_stats
    }

    /// Print comprehensive help message with all available keyboard shortcuts.
    fn print_help_message() {
        const HELP_KEYS_INFO: &str = "HELP: The following Keys are available:";
        const HELP_OPTION_H: &str = "H - Show this help message.";
        const HELP_OPTION_D: &str = "D - Toggle the display of real data.";
        const HELP_OPTION_S: &str = "S - Toggle step-by-step mode.";
        const HELP_OPTION_T: &str = "T - Toggle the display of timing information.";
        const HELP_OPTION_INCREASE_SPEED: &str = "+ - Increase the simulation speed.";
        const HELP_OPTION_DECREASE_SPEED: &str =
            "- - Decrease the simulation speed (minimum speed is 1).";
        const HELP_OPTION_B: &str = "B - Toggle the display of ball statistics.";
        const HELP_OPTION_V: &str = "V - Set verbosity for ball statistics:\n\
            - Enter a specific ball ID to toggle verbose mode for that ball.\n\
            - Enter 'a' to set all balls to verbose.\n\
            - Enter '0' to disable verbose mode for all balls.";
        const HELP_OPTION_0: &str = "0 - Reset viewport to default view (1.0x zoom, centered).";
        const HELP_OPTION_SPACE: &str = "SPACE - Pause or resume the simulation.";

        log::info!(
            "{header}\n\
             {opt_h}\n\
             {opt_d}\n\
             {opt_s}\n\
             {opt_t}\n\
             {opt_plus}\n\
             {opt_minus}\n\
             {opt_b}\n\
             {opt_v}\n\
             {opt_0}\n\
             {opt_space}",
            header = HELP_KEYS_INFO,
            opt_h = HELP_OPTION_H,
            opt_d = HELP_OPTION_D,
            opt_s = HELP_OPTION_S,
            opt_t = HELP_OPTION_T,
            opt_plus = HELP_OPTION_INCREASE_SPEED,
            opt_minus = HELP_OPTION_DECREASE_SPEED,
            opt_b = HELP_OPTION_B,
            opt_v = HELP_OPTION_V,
            opt_0 = HELP_OPTION_0,
            opt_space = HELP_OPTION_SPACE,
        );
    }

    /// Generate status text with simulation statistics.
    /// Returns None if ball stats display is disabled.
    /// Uses real units (SI) if real_data flag is set, otherwise simulation units.
    fn status_text(&self, fps: u32, dt: f32) -> Option<String> {
        if !self.show_ball_stats {
            return None;
        }
        let num_balls = self.get_number_of_balls();
        let return_value = if self.real_data {
            self.sim.get_status_text_real(fps, num_balls, dt)
        } else {
            self.sim
                .get_status_text(fps, num_balls, get_update_hood_call_cnt(), dt)
        };
        Some(return_value)
    }

    /// Format float using scientific notation for very large/small values.
    /// Uses "e" notation when |value| >= 10^4 or |value| < 10^-3 (excluding zero).
    fn format_g(value: f32, precision: usize) -> String {
        if value.abs() >= 1e4 || value.abs() < 1e-3 && value != 0.0 {
            format!("{:.*e}", precision - 1, value) // Use scientific notation
        } else {
            format!("{:.*}", precision, value) // Use normal float notation
        }
    }

    fn log_closeness_update(ball: &Ball2D) {
        if ball.verbose.console {
            log::debug!(
                "update_closeness: {} nearest/farthest: {}/{}",
                ball.get_name(),
                ball.hood.nearest,
                ball.hood.farthest
            );
        }
    }

    fn log_timings(&self, timer: crate::timing_monad::TimingMonad) {
        if self.show_timings {
            timer.log();
        }
    }

    fn log_separator_line() {
        log::debug!("----------------------------------------");
    }
}

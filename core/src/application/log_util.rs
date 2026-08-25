use crate::application::sim_stats::{HIGH_GRAVITY_MPS2, NORMAL_GRAVITY_MPS2};
use crate::application::{LogStuff, MainContent};
use crate::objects2d::ball_impl::{Ball2D, Named, get_update_hood_call_cnt};

impl LogStuff for MainContent {
    fn any_log(&self) -> bool {
        self.world().show_timings //|| self.world().show_ball_stats
    }

    /// Print comprehensive help message with all available keyboard shortcuts.
    fn print_help_message() {
        const HELP_KEYS_INFO: &str = "HELP: Available keyboard shortcuts:";
        const HELP_OPTION_F1: &str = "F1 - Stop all balls.";
        const HELP_OPTION_F2: &str = "F2 - Toggle rendering.";
        const HELP_OPTION_F3: &str = "F3 - Toggle cutoff rings.";
        const HELP_OPTION_F4: &str = "F4 - Toggle timing display.";
        const HELP_OPTION_F5: &str = "F5 - Add 100 random balls.";
        const HELP_OPTION_TAB: &str = "TAB - Toggle the energy window.";
        const HELP_OPTION_H: &str = "H - Show this help message.";
        const HELP_OPTION_ESC: &str = "ESC - Clear all balls.";
        const HELP_OPTION_SPACE: &str = "SPACE - Pause or resume the simulation.";
        const HELP_OPTION_D: &str = "D - Toggle the display of real data.";
        let help_option_g = format!(
            "G - Cycle gravity: off -> {:.2} m/s^2 -> {:.2} m/s^2.",
            NORMAL_GRAVITY_MPS2, HIGH_GRAVITY_MPS2
        );
        const HELP_OPTION_S: &str = "S - Toggle step-by-step mode.";
        const HELP_OPTION_T: &str =
            "T - Cycle thermo overlay: Off -> Grid -> Temperature -> Pressure -> Off.";
        const HELP_OPTION_P: &str = "P - Cycle plot content.";
        const HELP_OPTION_R: &str = "R - Randomize all ball speeds.";
        const HELP_OPTION_B: &str = "B - Toggle the display of ball statistics.";
        const HELP_OPTION_V: &str = "V - Set verbosity for ball statistics:\n\
            - Enter a specific ball ID to toggle verbose mode for that ball.\n\
            - Enter 'a' to set all balls to verbose.\n\
            - Enter '0' to disable verbose mode for all balls.";
        const HELP_OPTION_0: &str = "0 - Reset viewport to default view (1.0x zoom, centered).";
        const HELP_OPTION_INCREASE_SPEED: &str = "+ / = - Double the simulation time step.";
        const HELP_OPTION_DECREASE_SPEED: &str =
            "- / _ - Halve the simulation time step.";
        const HELP_OPTION_ALT_INCREASE_WAIT: &str = "Alt + + / = - Increase wait time.";
        const HELP_OPTION_ALT_DECREASE_WAIT: &str = "Alt + - / _ - Decrease wait time.";
        const HELP_OPTION_SHIFT: &str = "Shift - Enable random ball speed.";

        log::info!(
            "{header}\n\
             {opt_f1}\n\
             {opt_f2}\n\
             {opt_f3}\n\
             {opt_f4}\n\
             {opt_f5}\n\
             {opt_tab}\n\
             {opt_h}\n\
             {opt_esc}\n\
             {opt_space}\n\
             {opt_d}\n\
             {opt_g}\n\
             {opt_s}\n\
             {opt_t}\n\
             {opt_p}\n\
             {opt_r}\n\
             {opt_b}\n\
             {opt_v}\n\
             {opt_0}\n\
             {opt_plus}\n\
             {opt_minus}\n\
             {opt_alt_plus}\n\
             {opt_alt_minus}\n\
             {opt_shift}",
            header = HELP_KEYS_INFO,
            opt_f1 = HELP_OPTION_F1,
            opt_f2 = HELP_OPTION_F2,
            opt_f3 = HELP_OPTION_F3,
            opt_f4 = HELP_OPTION_F4,
            opt_f5 = HELP_OPTION_F5,
            opt_tab = HELP_OPTION_TAB,
            opt_h = HELP_OPTION_H,
            opt_esc = HELP_OPTION_ESC,
            opt_space = HELP_OPTION_SPACE,
            opt_d = HELP_OPTION_D,
            opt_g = help_option_g,
            opt_s = HELP_OPTION_S,
            opt_t = HELP_OPTION_T,
            opt_p = HELP_OPTION_P,
            opt_r = HELP_OPTION_R,
            opt_b = HELP_OPTION_B,
            opt_v = HELP_OPTION_V,
            opt_0 = HELP_OPTION_0,
            opt_plus = HELP_OPTION_INCREASE_SPEED,
            opt_minus = HELP_OPTION_DECREASE_SPEED,
            opt_alt_plus = HELP_OPTION_ALT_INCREASE_WAIT,
            opt_alt_minus = HELP_OPTION_ALT_DECREASE_WAIT,
            opt_shift = HELP_OPTION_SHIFT,
        );
    }

    /// Generate status text with simulation statistics.
    /// Returns None if ball stats display is disabled.
    /// Uses real units (SI) if real_data flag is set, otherwise simulation units.
    fn status_text(&self, fps: u32, dt: f32) -> Option<String> {
        if !self.world().show_ball_stats {
            return None;
        }
        let world = self.world();
        let num_balls = world.balls.len();
        let return_value = if world.real_data {
            world.sim.get_status_text_real(fps, num_balls, dt)
        } else {
            world
                .sim
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
        if self.world().show_timings {
            timer.log();
        }
    }

    fn log_separator_line() {
        log::debug!("----------------------------------------");
    }
}

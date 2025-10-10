use crate::frame::{LogStuff, MainContent};
use crate::objects2d::ball_impl::{Ball2D, Named, get_update_hood_call_cnt};

impl LogStuff for MainContent {
    fn any_log(&self) -> bool {
        self.show_timings //|| self.show_ball_stats
    }

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
        const HELP_OPTION_SPACE: &str = "SPACE - Pause or resume the simulation.";

        println!(
            "{}",
            format!(
                "{header}\n\
             {opt_h}\n\
             {opt_d}\n\
             {opt_s}\n\
             {opt_t}\n\
             {opt_plus}\n\
             {opt_minus}\n\
             {opt_b}\n\
             {opt_v}\n\
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
                opt_space = HELP_OPTION_SPACE,
            )
        );
    }

    fn status_text(&self, fps: u32) -> Option<String> {
        if !self.show_ball_stats {
            return None;
        }
        let num_balls = self.get_number_of_balls();
        let return_value = if self.real_data {
            self.sim.get_status_text_real(fps, num_balls)
        } else {
            self.sim
                .get_status_text(fps, num_balls, get_update_hood_call_cnt())
        };
        Some(return_value)
    }

    fn format_g(value: f32, precision: usize) -> String {
        if value.abs() >= 1e4 || value.abs() < 1e-3 && value != 0.0 {
            format!("{:.*e}", precision - 1, value) // Use scientific notation
        } else {
            format!("{:.*}", precision, value) // Use normal float notation
        }
    }

    fn log_closeness_update(ball: &Ball2D) {
        if ball.verbose.console {
            println!(
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
        println!("----------------------------------------");
    }
}

pub fn log_update_hood_start(ball: &crate::objects2d::ball_impl::Ball2D, cutoff: f32) {
    println!("update_hood: {:?} dist_lim: {} ", ball, cutoff,);
}

pub fn log_neighbour_add(
    ball: &crate::objects2d::ball_impl::Ball2D,
    cur_ref: &crate::objects2d::ball_impl::Ball2D,
    dist_sq: f32,
    dist_limit_sq: f32,
) {
    if ball.verbose.console {
        println!(
            "\tneighbours {} at ({},{}) dist {}",
            cur_ref.get_name(),
            cur_ref.get_position().x,
            cur_ref.get_position().y,
            f32::sqrt(dist_sq)
        );
    } else if cur_ref.verbose.console {
        println!(
            "\t {} is neighbour of {} since its dist is {} (dist_limit {})",
            cur_ref.get_name(),
            ball.get_name(),
            f32::sqrt(dist_sq),
            f32::sqrt(dist_limit_sq)
        )
    }
}

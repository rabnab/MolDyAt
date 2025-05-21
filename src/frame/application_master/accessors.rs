use super::*;
use ggez::graphics::Shader;

impl<'a> MainContent<'a> {
    pub(crate) fn get_window_definition(&self) -> &WindowDefinition {
        &self.win_definition
    }
    pub(crate) fn mut_window_definition(&mut self) -> &mut WindowDefinition {
        &mut self.win_definition
    }

    pub(crate) fn get_game_logic_data(&mut self) -> &mut GameLogicData {
        &mut self.game_data
    }
    pub(crate) fn plot_type(&self) -> PlotContent {
        self.show_plot
    }
    pub(crate) fn cycle_plot_content(&mut self) {
        self.show_plot = match self.show_plot {
            PlotContent::NONE => PlotContent::EKIN,
            PlotContent::EKIN => PlotContent::EPOT,
            PlotContent::EPOT => PlotContent::ETOT,
            PlotContent::ETOT => PlotContent::ALL,
            PlotContent::ALL => PlotContent::NONE,
        };
    }
    pub(crate) fn get_shader(&self) -> Option<Shader> {
        self.shader.clone()
    }
    pub(crate) fn enable_drawing_cursor(&mut self, repeated: bool) {
        if !repeated {
            self.sim.set_draw_cursor(true);
        }
    }
}

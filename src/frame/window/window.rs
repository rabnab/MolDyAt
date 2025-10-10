use crate::frame::window::{Point2D, WindowEvent};

pub trait Window<E> {
    fn set_title(&mut self, title: &str);
    fn get_title(&self) -> &str;
    fn set_size(&mut self, width: u32, height: u32);
    fn get_size(&self) -> (u32, u32);
    fn set_fullscreen(&mut self, fullscreen: bool);
    fn is_fullscreen(&self) -> bool;
    fn close(&mut self);

    // Input and events
    fn poll_events(&mut self) -> Vec<WindowEvent>;
    fn set_cursor_visible(&mut self, visible: bool);
    fn set_cursor_position(&mut self, pos: Point2D);
    fn create_window(&self);
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[derive(Default)]
pub struct VerboseInfo {
    pub console: bool,
    main_neighbour: bool,
    other_neighbour: bool,
    speed: bool,
}

pub enum VerboseInfoMode {
    Console,
    Main,
    Other,
    Speed,
}

impl VerboseInfo {
    pub fn lines(&self) -> bool {
        self.main_neighbour || self.other_neighbour || self.speed
    }

    pub(crate) fn toggle_console(&mut self) {
        self.console = !self.console;
    }

    pub(crate) fn toggle_main_neighbour(&mut self) {
        self.main_neighbour = !self.main_neighbour;
    }

    pub(crate) fn toggle_other_neighbour(&mut self) {
        self.other_neighbour = !self.other_neighbour;
    }

    pub(crate) fn toggle_speed(&mut self) {
        self.speed = !self.speed;
    }

    pub(crate) fn any(&self) -> bool {
        self.console || self.lines()
    }

    pub fn is_main(&self) -> bool {
        self.main_neighbour
    }
    pub(crate) fn clear_all(&mut self) {
        self.console = false;
        self.main_neighbour = false;
        self.other_neighbour = false;
        self.speed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbose_toggles_and_lines() {
        let mut v = VerboseInfo::default();
        assert!(!v.lines());
        assert!(!v.any());
        assert!(!v.is_main());
        v.toggle_console();
        assert!(v.console);
        assert!(v.any());
        v.toggle_main_neighbour();
        assert!(v.is_main());
        assert!(v.lines());
        v.toggle_other_neighbour();
        v.toggle_speed();
        v.clear_all();
        assert!(!v.console && !v.is_main());
        assert!(!v.lines());
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::renderer::event_loop::line_segment_tool::{
    LineSegmentDrawState, LineSegmentToolHandler,
};
use crate::graphics::renderer::event_loop::operator_tool_handler::OperatorToolHandler;

/// Switching tools must reset a line-segment handler that is mid-draw.
///
/// Regression test for: the old handler was replaced without calling
/// `reset()`, silently dropping in-progress drawing state.
#[test]
fn reset_clears_mid_draw_line_segment_state() {
    use thermo_physics::objects2d::Vec2D;

    let mut handler = LineSegmentToolHandler::new();
    // Simulate the user starting a line draw.
    handler.state = LineSegmentDrawState::Drawing {
        start: Vec2D::new(1.0, 2.0),
    };

    // This is what `process_action(SelectOperatorTool(..))` now calls
    // on the outgoing handler before replacing it.
    handler.reset();

    assert_eq!(handler.state, LineSegmentDrawState::Idle);
}

/// Resetting an already-idle handler is harmless.
#[test]
fn reset_on_idle_handler_is_noop() {
    let mut handler = LineSegmentToolHandler::new();
    assert_eq!(handler.state, LineSegmentDrawState::Idle);

    handler.reset();

    assert_eq!(handler.state, LineSegmentDrawState::Idle);
}

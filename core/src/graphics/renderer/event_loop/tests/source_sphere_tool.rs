// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::renderer::event_loop::DEFAULT_SOURCE_RADIUS;

// Just verify the constant is accessible and has a reasonable value.
#[test]
#[allow(clippy::assertions_on_constants)]
fn default_source_radius_is_positive() {
    assert!(DEFAULT_SOURCE_RADIUS > 0.0);
}

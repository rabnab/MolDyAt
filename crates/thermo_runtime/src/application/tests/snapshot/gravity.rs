// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot tests moved from `thermo_physics` `core/tests/gravity.rs` (#308).

use crate::application::CreateSnapshot;
use thermo_physics::test_support::create_main_content;

#[test]
fn test_snapshot_reports_gravity_force_multiple() {
    let (mut main_content, _) = create_main_content();

    assert_eq!(
        main_content.create_snapshot().frame.gravity_force_multiple,
        0
    );

    main_content.toggle_gravity();
    assert_eq!(
        main_content.create_snapshot().frame.gravity_force_multiple,
        1
    );

    main_content.toggle_gravity();
    assert_eq!(
        main_content.create_snapshot().frame.gravity_force_multiple,
        5
    );
}

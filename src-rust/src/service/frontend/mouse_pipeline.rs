// Mouse input pipeline — processes extracted mouse values, updates position,
// routes reports, and handles screen switching.

use crate::domain::constants::{
    PacketType, MAX_SCREEN_COORD, MIN_SCREEN_COORD, ABSOLUTE, RELATIVE,
    OS_MACOS, OS_WINDOWS,
};
use crate::domain::mouse;
use crate::domain::mouse_logic::{
    self, MouseValues, SwitchDirection, SwitchContext, ScreenSwitchAction,
};
use crate::domain::structs::Device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

const MACOS_SWITCH_MOVE_X: i16 = 10;
const MACOS_SWITCH_MOVE_COUNT: usize = 5;

/// Process extracted mouse values: update position, build report, route, handle screen switch.
pub fn process_report(
    state: &mut Device,
    hal: &(impl ReportRouter + OutputControl),
    values: &MouseValues,
) {
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }
    let output = &state.config.output[output_idx];

    let (new_x, new_y, dir) = mouse_logic::update_mouse_position(
        state.pointer_x, state.pointer_y, values,
        output.speed_x, output.speed_y,
        state.mouse_zoom, state.config.enable_acceleration != 0,
        state.config.jump_threshold,
    );
    state.pointer_x = new_x;
    state.pointer_y = new_y;
    state.mouse_buttons = values.buttons as i16;

    let report = mouse_logic::create_mouse_report(
        state.pointer_x, state.pointer_y, values,
        state.relative_mouse, state.gaming_mode,
    );

    let report_bytes = [
        report.buttons,
        report.x.to_le_bytes()[0], report.x.to_le_bytes()[1],
        report.y.to_le_bytes()[0], report.y.to_le_bytes()[1],
        report.wheel as u8,
        report.pan as u8,
        report.mode,
    ];

    hal.route_mouse(state, &report_bytes);

    if dir != SwitchDirection::None {
        do_screen_switch(state, hal, dir);
    }
}

/// Evaluate screen switch and execute the appropriate action.
pub fn do_screen_switch(
    state: &mut Device,
    hal: &(impl OutputControl + ReportQueue + PeerLink),
    direction: SwitchDirection,
) {
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    let output = &state.config.output[output_idx];
    let ctx = SwitchContext {
        switch_lock: state.switch_lock,
        gaming_mode: state.gaming_mode,
        mouse_buttons: state.mouse_buttons,
        screen_pos: output.pos,
        screen_index: output.screen_index,
        screen_count: output.screen_count,
    };

    match mouse_logic::decide_screen_switch(direction, &ctx) {
        ScreenSwitchAction::Nothing => {}
        ScreenSwitchAction::SwitchToOtherPc => {
            let output_number = output.number;
            let dir_int = match direction {
                SwitchDirection::Left => 1,
                SwitchDirection::Right => 2,
                _ => return,
            };
            switch_to_peer(state, hal, output_number, (1 - state.active_output) as i32, dir_int);
        }
        ScreenSwitchAction::SwitchVirtualDesktop { new_index } => {
            let os = output.os;
            let dir_int = match direction {
                SwitchDirection::Left => 1,
                SwitchDirection::Right => 2,
                _ => return,
            };
            switch_virtual_desktop(state, hal, os, new_index as i32, dir_int);
            state.config.output[output_idx].screen_index = new_index;
        }
    }
}

/// Switch active output to the other board, parking the mouse pointer.
pub fn switch_to_peer(
    state: &mut Device,
    hal: &(impl OutputControl + ReportQueue + PeerLink),
    output_number: u32,
    output_to: i32,
    direction: i32,
) {
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    let mouse_park_pos = state.config.output[output_idx].mouse_park_pos;
    let mouse_y = match mouse_park_pos {
        0 => MIN_SCREEN_COORD,
        1 => MAX_SCREEN_COORD,
        _ => state.pointer_y,
    };

    let hidden = [
        0u8,
        MAX_SCREEN_COORD.to_le_bytes()[0], MAX_SCREEN_COORD.to_le_bytes()[1],
        mouse_y.to_le_bytes()[0], mouse_y.to_le_bytes()[1],
        0, 0, 0,
    ];

    output_report_raw(hal, state, &hidden);
    hal.switch_output(output_to as u8);

    state.pointer_x = if direction == 1 { MAX_SCREEN_COORD } else { MIN_SCREEN_COORD };

    let other = 1 - output_number;
    if (output_number as usize) < state.config.output.len()
        && (other as usize) < state.config.output.len()
    {
        let from = &state.config.output[output_number as usize];
        let to = &state.config.output[other as usize];
        state.pointer_y = mouse::scale_y_coordinate(
            state.pointer_y,
            (from.border.top, from.border.bottom),
            (to.border.top, to.border.bottom),
        );
    }
}

/// Route a mouse report based on active output (without timestamp — used for switch animations).
fn output_report_raw(hal: &(impl ReportQueue + PeerLink), state: &Device, report: &[u8; 8]) {
    if state.is_active_output() {
        hal.push_mouse_report(report);
    } else {
        hal.send_packet(report, PacketType::MouseReport as u8);
    }
}

/// Switch virtual desktop (OS-dependent behavior).
fn switch_virtual_desktop(
    state: &mut Device,
    hal: &(impl ReportQueue + PeerLink),
    os: u8,
    new_index: i32,
    direction: i32,
) {
    match os {
        OS_MACOS => switch_desktop_macos(state, hal, direction),
        OS_WINDOWS => { state.relative_mouse = new_index > 1; }
        _ => {}
    }
    state.pointer_x = if direction == 2 { MIN_SCREEN_COORD } else { MAX_SCREEN_COORD };
}

/// macOS-specific virtual desktop switch (edge + relative moves).
fn switch_desktop_macos(
    state: &Device,
    hal: &(impl ReportQueue + PeerLink),
    direction: i32,
) {
    let left = direction == 1;
    let edge_x = if left { MIN_SCREEN_COORD } else { MAX_SCREEN_COORD };
    let edge = [
        state.mouse_buttons as u8,
        edge_x.to_le_bytes()[0], edge_x.to_le_bytes()[1],
        (MAX_SCREEN_COORD / 2).to_le_bytes()[0], (MAX_SCREEN_COORD / 2).to_le_bytes()[1],
        0, 0, ABSOLUTE,
    ];
    output_report_raw(hal, state, &edge);

    let move_x: i16 = if left { -MACOS_SWITCH_MOVE_X } else { MACOS_SWITCH_MOVE_X };
    let rel = [
        state.mouse_buttons as u8,
        move_x.to_le_bytes()[0], move_x.to_le_bytes()[1],
        0, 0, 0, 0, RELATIVE,
    ];
    for _ in 0..MACOS_SWITCH_MOVE_COUNT {
        output_report_raw(hal, state, &rel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    fn make_state_with_output() -> Device {
        let mut state = Device::zeroed();
        state.active_output = 0;
        state.board_role = 0; // active output
        state.config.output[0].speed_x = 16;
        state.config.output[0].speed_y = 16;
        state
    }

    #[test]
    fn process_report_updates_position() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.pointer_x = 1000;
        state.pointer_y = 1000;

        let values = MouseValues { move_x: 100, move_y: -50, wheel: 0, pan: 0, buttons: 0 };
        process_report(&mut state, &hal, &values);

        // Position should have changed
        assert_ne!(state.pointer_x, 1000);
        // Report should be routed (active output → local queue)
        assert_eq!(hal.mouse_reports.borrow().len(), 1);
    }

    #[test]
    fn process_report_routes_to_peer_when_inactive() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.active_output = 1; // NOT active (board_role=0)

        let values = MouseValues { move_x: 10, move_y: 0, wheel: 0, pan: 0, buttons: 0 };
        process_report(&mut state, &hal, &values);

        // Should route via peer link, not local queue
        assert!(hal.mouse_reports.borrow().is_empty());
        assert_eq!(hal.sent_packets.borrow().len(), 1);
    }

    #[test]
    fn switch_to_peer_parks_mouse_top() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.config.output[0].mouse_park_pos = 0; // park top

        switch_to_peer(&mut state, &hal, 0, 1, 2); // switch right

        assert_eq!(hal.output_switched.get(), Some(1));
        assert_eq!(state.pointer_x, MIN_SCREEN_COORD); // right → min
    }

    #[test]
    fn switch_to_peer_parks_mouse_bottom() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.config.output[0].mouse_park_pos = 1; // park bottom

        switch_to_peer(&mut state, &hal, 0, 1, 1); // switch left

        assert_eq!(hal.output_switched.get(), Some(1));
        assert_eq!(state.pointer_x, MAX_SCREEN_COORD); // left → max
    }

    #[test]
    fn do_screen_switch_locked() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.switch_lock = true;

        do_screen_switch(&mut state, &hal, SwitchDirection::Left);

        // Nothing should happen — switch locked
        assert_eq!(hal.output_switched.get(), None);
    }

    #[test]
    fn do_screen_switch_gaming_mode() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.gaming_mode = true;

        do_screen_switch(&mut state, &hal, SwitchDirection::Right);

        assert_eq!(hal.output_switched.get(), None);
    }

    #[test]
    fn output_report_raw_routes_correctly() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        let report = [1u8, 2, 3, 4, 5, 6, 7, 8];

        // Active output → local queue
        state.board_role = 0;
        state.active_output = 0;
        output_report_raw(&hal, &state, &report);
        assert_eq!(hal.mouse_reports.borrow().len(), 1);

        // Inactive → peer
        state.active_output = 1;
        output_report_raw(&hal, &state, &report);
        assert_eq!(hal.sent_packets.borrow().len(), 1);
    }

    #[test]
    fn test_process_report_with_zoom() {
        let hal = MockHal::new();
        let mut state_zoom = make_state_with_output();
        state_zoom.pointer_x = 16000;
        state_zoom.pointer_y = 16000;
        state_zoom.mouse_zoom = true;

        let mut state_normal = make_state_with_output();
        state_normal.pointer_x = 16000;
        state_normal.pointer_y = 16000;
        state_normal.mouse_zoom = false;

        let values = MouseValues { move_x: 200, move_y: 100, wheel: 0, pan: 0, buttons: 0 };

        process_report(&mut state_zoom, &hal, &values);
        let zoom_x = state_zoom.pointer_x;

        let hal2 = MockHal::new();
        process_report(&mut state_normal, &hal2, &values);
        let normal_x = state_normal.pointer_x;

        // Zoom mode should produce less movement (speed >> 2)
        assert!(zoom_x < normal_x, "zoom_x={} should be less than normal_x={}", zoom_x, normal_x);
    }

    #[test]
    fn test_process_report_gaming_mode_no_switch() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.gaming_mode = true;
        state.pointer_x = 100; // near left edge

        // Large leftward movement that would normally trigger screen switch
        let values = MouseValues { move_x: -500, move_y: 0, wheel: 0, pan: 0, buttons: 0 };
        process_report(&mut state, &hal, &values);

        // No screen switch should occur (gaming mode blocks it)
        assert_eq!(hal.output_switched.get(), None);
        // Report should still be routed
        assert_eq!(hal.mouse_reports.borrow().len(), 1);
    }

    #[test]
    fn test_switch_to_peer_preserves_y() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.config.output[0].mouse_park_pos = 2; // previous = preserve Y
        state.pointer_y = 12345;

        switch_to_peer(&mut state, &hal, 0, 1, 2); // switch right

        assert_eq!(hal.output_switched.get(), Some(1));
        // Verify the hidden report used the preserved Y (not MIN or MAX)
        let reports = hal.mouse_reports.borrow();
        assert!(!reports.is_empty());
        let hidden = &reports[0];
        let y_in_report = i16::from_le_bytes([hidden[3], hidden[4]]);
        assert_eq!(y_in_report, 12345);
    }

    #[test]
    fn test_process_report_screen_count_one_no_switch() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.config.output[0].screen_count = 1;
        state.config.output[0].screen_index = 1;
        state.config.output[0].pos = 2; // RIGHT — other PC is LEFT
        state.pointer_x = 100; // near left edge

        // Large leftward movement — would normally trigger Left switch
        let values = MouseValues { move_x: -500, move_y: 0, wheel: 0, pan: 0, buttons: 0 };
        process_report(&mut state, &hal, &values);

        // With screen_count=1 and screen_index=1, going LEFT (toward other PC)
        // should trigger SwitchToOtherPc. But we want to verify behavior:
        // screen_count=1 means only one virtual desktop — no virtual desktop switching
        // However, it will still switch to other PC since screen_index=1.
        // To truly prevent switch, we need the switch direction to match screen_pos.
        // Let's set pos=1 (LEFT) so going LEFT (same direction) tries virtual desktop,
        // but screen_count=1 means no more screens → Nothing.
        let hal2 = MockHal::new();
        let mut state2 = make_state_with_output();
        state2.config.output[0].screen_count = 1;
        state2.config.output[0].screen_index = 1;
        state2.config.output[0].pos = 1; // LEFT
        state2.pointer_x = 100;

        let values = MouseValues { move_x: -500, move_y: 0, wheel: 0, pan: 0, buttons: 0 };
        process_report(&mut state2, &hal2, &values);

        // Going Left with pos=LEFT, screen_index=1, screen_count=1
        // → screen_pos == dir_val, screen_index not < screen_count → Nothing
        assert_eq!(hal2.output_switched.get(), None);
    }

    #[test]
    fn test_process_report_output_out_of_range() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        state.active_output = 99; // out of range

        let values = MouseValues { move_x: 100, move_y: 50, wheel: 0, pan: 0, buttons: 0 };
        // Should not crash — early return due to output_idx >= output.len()
        process_report(&mut state, &hal, &values);

        // No reports should be routed
        assert!(hal.mouse_reports.borrow().is_empty());
        assert!(hal.sent_packets.borrow().is_empty());
    }

    #[test]
    fn test_border_y_scaling_accuracy() {
        let hal = MockHal::new();
        let mut state = make_state_with_output();
        // Output 0: border top=1000, bottom=2000 → from_range = 32767 - 1000 - 2000 = 29767
        state.config.output[0].border.top = 1000;
        state.config.output[0].border.bottom = 2000;
        state.config.output[0].number = 0;
        // Output 1: border top=3000, bottom=4000 → to_range = 32767 - 3000 - 4000 = 25767
        state.config.output[1].border.top = 3000;
        state.config.output[1].border.bottom = 4000;
        state.config.output[1].number = 1;

        state.pointer_y = 16000;

        // switch_to_peer from output 0 to output 1
        switch_to_peer(&mut state, &hal, 0, 1, 2);

        // scale_y_coordinate: (y - from_top) * to_range / from_range + to_top
        let from_top = 1000i64;
        let from_bottom = 2000i64;
        let to_top = 3000i64;
        let to_bottom = 4000i64;
        let from_range = 32767 - from_top - from_bottom; // 29767
        let to_range = 32767 - to_top - to_bottom;       // 25767
        let expected = ((16000 - from_top) * to_range / from_range + to_top) as i16;
        assert_eq!(state.pointer_y, expected);
    }
}

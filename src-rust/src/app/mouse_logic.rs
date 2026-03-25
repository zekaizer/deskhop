use crate::app::constants::{ABSOLUTE, RELATIVE};
use crate::app::mouse;
use crate::app::screensaver::MouseReport;

/// Extended precision mouse movement values (from HID report extraction)
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseValues {
    pub move_x: i32,
    pub move_y: i32,
    pub wheel: i32,
    pub pan: i32,
    pub buttons: i32,
}

/// Result of updating mouse position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchDirection {
    None,
    Left,
    Right,
}

/// Screen switch decision context
pub struct SwitchContext {
    pub switch_lock: bool,
    pub gaming_mode: bool,
    pub mouse_buttons: i16,
    pub screen_pos: u8,       // output.pos (LEFT=1, RIGHT=2)
    pub screen_index: u32,
    pub screen_count: u32,
}

/// Calculate updated mouse position and determine if screen switch is needed.
pub fn update_mouse_position(
    pointer_x: i16,
    pointer_y: i16,
    values: &MouseValues,
    speed_x: i32,
    speed_y: i32,
    mouse_zoom: bool,
    enable_acceleration: bool,
    jump_threshold: u16,
) -> (i16, i16, SwitchDirection) {
    let zoom_shift: u32 = if mouse_zoom { 2 } else { 0 }; // MOUSE_ZOOM_SCALING_FACTOR

    let accel_fp = mouse::calculate_mouse_acceleration_factor_fp(
        values.move_x,
        values.move_y,
        enable_acceleration,
    );

    // Fixed-point multiplication: (move * accel_fp * speed) >> 8
    // accel_fp is ×256, so >>8 normalizes back
    let sx = speed_x >> zoom_shift;
    let sy = speed_y >> zoom_shift;
    let offset_x = ((values.move_x as i64 * accel_fp as i64 * sx as i64) >> 8) as i32;
    let offset_y = ((values.move_y as i64 * accel_fp as i64 * sy as i64) >> 8) as i32;

    let switch = mouse::is_screen_switch_needed(pointer_x as i32, offset_x, jump_threshold);

    let new_x = mouse::move_and_keep_on_screen(pointer_x as i32, offset_x) as i16;
    let new_y = mouse::move_and_keep_on_screen(pointer_y as i32, offset_y) as i16;

    let direction = match switch {
        -1 => SwitchDirection::Left,
        1 => SwitchDirection::Right,
        _ => SwitchDirection::None,
    };

    (new_x, new_y, direction)
}

/// Create a mouse report from current state
pub fn create_mouse_report(
    pointer_x: i16,
    pointer_y: i16,
    values: &MouseValues,
    relative_mouse: bool,
    gaming_mode: bool,
) -> MouseReport {
    if relative_mouse || gaming_mode {
        MouseReport {
            buttons: values.buttons as u8,
            x: values.move_x as i16,
            y: values.move_y as i16,
            wheel: values.wheel as i8,
            pan: values.pan as i8,
            mode: RELATIVE,
        }
    } else {
        MouseReport {
            buttons: values.buttons as u8,
            x: pointer_x,
            y: pointer_y,
            wheel: values.wheel as i8,
            pan: values.pan as i8,
            mode: ABSOLUTE,
        }
    }
}

/// Determine what action to take when a screen switch is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSwitchAction {
    Nothing,
    SwitchToOtherPc,
    SwitchVirtualDesktop { new_index: u32 },
}

/// Decide whether/how to switch screens based on direction and current state
pub fn decide_screen_switch(
    direction: SwitchDirection,
    ctx: &SwitchContext,
) -> ScreenSwitchAction {
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;

    if direction == SwitchDirection::None {
        return ScreenSwitchAction::Nothing;
    }

    if ctx.switch_lock || ctx.gaming_mode {
        return ScreenSwitchAction::Nothing;
    }

    let dir_val = match direction {
        SwitchDirection::Left => LEFT,
        SwitchDirection::Right => RIGHT,
        SwitchDirection::None => return ScreenSwitchAction::Nothing,
    };

    // We want to jump in the direction of the other computer
    if ctx.screen_pos != dir_val {
        if ctx.screen_index == 1 {
            // At the border — switch outputs, but not while mouse button held
            if ctx.mouse_buttons != 0 {
                return ScreenSwitchAction::Nothing;
            }
            return ScreenSwitchAction::SwitchToOtherPc;
        }
        // Multiple desktops, not on main — go toward main
        return ScreenSwitchAction::SwitchVirtualDesktop {
            new_index: ctx.screen_index - 1,
        };
    }

    // We want to jump away from the other computer — only if more screens
    if ctx.screen_index < ctx.screen_count {
        return ScreenSwitchAction::SwitchVirtualDesktop {
            new_index: ctx.screen_index + 1,
        };
    }

    ScreenSwitchAction::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_mouse_position_normal() {
        let values = MouseValues {
            move_x: 10,
            move_y: 5,
            ..Default::default()
        };

        let (x, y, dir) = update_mouse_position(
            16000, 16000, &values, 16, 28, false, false, 0,
        );

        assert!(x > 16000); // moved right
        assert!(y > 16000); // moved down
        assert_eq!(dir, SwitchDirection::None);
    }

    #[test]
    fn test_update_mouse_position_switch_left() {
        let values = MouseValues {
            move_x: -100,
            move_y: 0,
            ..Default::default()
        };

        let (_, _, dir) = update_mouse_position(
            100, 16000, &values, 16, 28, false, false, 0,
        );

        assert_eq!(dir, SwitchDirection::Left);
    }

    #[test]
    fn test_create_report_absolute() {
        let values = MouseValues {
            move_x: 10,
            buttons: 1,
            wheel: 2,
            ..Default::default()
        };

        let report = create_mouse_report(1000, 2000, &values, false, false);
        assert_eq!(report.x, 1000);
        assert_eq!(report.y, 2000);
        assert_eq!(report.mode, ABSOLUTE);
        assert_eq!(report.buttons, 1);
    }

    #[test]
    fn test_create_report_relative_gaming() {
        let values = MouseValues {
            move_x: 42,
            move_y: -10,
            ..Default::default()
        };

        let report = create_mouse_report(1000, 2000, &values, false, true);
        assert_eq!(report.x, 42);
        assert_eq!(report.y, -10);
        assert_eq!(report.mode, RELATIVE);
    }

    #[test]
    fn test_decide_switch_to_other_pc() {
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: false,
            mouse_buttons: 0,
            screen_pos: 2,  // RIGHT
            screen_index: 1,
            screen_count: 1,
        };

        // Moving LEFT (toward other PC, since our pos is RIGHT)
        assert_eq!(
            decide_screen_switch(SwitchDirection::Left, &ctx),
            ScreenSwitchAction::SwitchToOtherPc,
        );
    }

    #[test]
    fn test_decide_switch_blocked_by_button() {
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: false,
            mouse_buttons: 1, // button held
            screen_pos: 2,
            screen_index: 1,
            screen_count: 1,
        };

        assert_eq!(
            decide_screen_switch(SwitchDirection::Left, &ctx),
            ScreenSwitchAction::Nothing,
        );
    }

    #[test]
    fn test_decide_switch_locked() {
        let ctx = SwitchContext {
            switch_lock: true,
            gaming_mode: false,
            mouse_buttons: 0,
            screen_pos: 2,
            screen_index: 1,
            screen_count: 1,
        };

        assert_eq!(
            decide_screen_switch(SwitchDirection::Left, &ctx),
            ScreenSwitchAction::Nothing,
        );
    }

    #[test]
    fn test_decide_virtual_desktop_switch() {
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: false,
            mouse_buttons: 0,
            screen_pos: 2, // RIGHT
            screen_index: 1,
            screen_count: 3,
        };

        // Moving RIGHT (away from other PC) with more screens available
        assert_eq!(
            decide_screen_switch(SwitchDirection::Right, &ctx),
            ScreenSwitchAction::SwitchVirtualDesktop { new_index: 2 },
        );
    }

    #[test]
    fn test_decide_gaming_mode_blocks_switch() {
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: true,
            mouse_buttons: 0,
            screen_pos: 2,
            screen_index: 1,
            screen_count: 1,
        };

        assert_eq!(
            decide_screen_switch(SwitchDirection::Left, &ctx),
            ScreenSwitchAction::Nothing,
        );
    }

    #[test]
    fn test_decide_virtual_desktop_backward() {
        // On screen_index=2, moving toward other PC should go to index 1
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: false,
            mouse_buttons: 0,
            screen_pos: 2, // RIGHT — other PC is LEFT
            screen_index: 2,
            screen_count: 3,
        };

        assert_eq!(
            decide_screen_switch(SwitchDirection::Left, &ctx),
            ScreenSwitchAction::SwitchVirtualDesktop { new_index: 1 },
        );
    }

    #[test]
    fn test_decide_at_last_screen_no_more() {
        // At last screen, trying to go further should do nothing
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: false,
            mouse_buttons: 0,
            screen_pos: 2, // RIGHT
            screen_index: 3,
            screen_count: 3,
        };

        assert_eq!(
            decide_screen_switch(SwitchDirection::Right, &ctx),
            ScreenSwitchAction::Nothing,
        );
    }

    #[test]
    fn test_update_mouse_position_zoom() {
        let values = MouseValues {
            move_x: 100,
            move_y: 0,
            ..Default::default()
        };

        // With zoom enabled, speed is halved (shift right by 2)
        let (x_zoom, _, _) = update_mouse_position(
            16000, 16000, &values, 16, 28, true, false, 0,
        );
        let (x_normal, _, _) = update_mouse_position(
            16000, 16000, &values, 16, 28, false, false, 0,
        );

        assert!(x_zoom < x_normal); // zoom should move less
    }

    #[test]
    fn test_create_report_relative_mode() {
        let values = MouseValues {
            move_x: 5,
            move_y: -3,
            ..Default::default()
        };

        let report = create_mouse_report(1000, 2000, &values, true, false);
        assert_eq!(report.mode, RELATIVE);
        assert_eq!(report.x, 5);
        assert_eq!(report.y, -3);
    }

    #[test]
    fn test_decide_switch_none_direction() {
        let ctx = SwitchContext {
            switch_lock: false,
            gaming_mode: false,
            mouse_buttons: 0,
            screen_pos: 2,
            screen_index: 1,
            screen_count: 1,
        };

        assert_eq!(
            decide_screen_switch(SwitchDirection::None, &ctx),
            ScreenSwitchAction::Nothing,
        );
    }
}

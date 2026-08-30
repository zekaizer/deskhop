use crate::domain::constants::{MAX_SCREEN_COORD, MIN_SCREEN_COORD};

const ACCEL_POINTS: usize = 7;

// Fixed-point acceleration factors (×256 scale, 8.8 format)
struct AccelPoint {
    value: i32,
    factor_fp: i32, // factor * 256
}

const ACCELERATION_CURVE: [AccelPoint; ACCEL_POINTS] = [
    AccelPoint { value: 2, factor_fp: 256 },   // 1.0
    AccelPoint { value: 5, factor_fp: 282 },   // 1.1
    AccelPoint { value: 15, factor_fp: 358 },  // 1.4
    AccelPoint { value: 30, factor_fp: 486 },  // 1.9
    AccelPoint { value: 45, factor_fp: 666 },  // 2.6
    AccelPoint { value: 60, factor_fp: 870 },  // 3.4
    AccelPoint { value: 70, factor_fp: 1024 }, // 4.0
];

/// Move coordinate by offset, clamping to screen bounds [0, 32767].
pub fn move_and_keep_on_screen(position: i32, offset: i32) -> i32 {
    let result = position + offset;

    if result < MIN_SCREEN_COORD as i32 {
        MIN_SCREEN_COORD as i32
    } else if result > MAX_SCREEN_COORD as i32 {
        MAX_SCREEN_COORD as i32
    } else {
        result
    }
}

/// Detect if screen switch threshold is reached.
/// Returns: -1 (switch left), 0 (no switch), 1 (switch right)
// WORKAROUND(c-compat): Returns i32 (-1/0/1) for C FFI compatibility.
pub fn is_screen_switch_needed(position: i32, offset: i32, threshold: u16) -> i32 {
    let new_pos = position + offset;

    if new_pos < MIN_SCREEN_COORD as i32 - threshold as i32 {
        -1 // Switch left
    } else if new_pos > MAX_SCREEN_COORD as i32 + threshold as i32 {
        1 // Switch right
    } else {
        0
    }
}

/// Screen-position values in config output.pos (matches C enum screen_pos_e).
pub const SCREEN_POS_LEFT: u8 = 1;
pub const SCREEN_POS_RIGHT: u8 = 2;

/// Threshold to apply for a movement in `direction` (SCREEN_POS_LEFT/RIGHT).
/// Local switches (virtual desktop changes) get no gap; only cross-output
/// jumps use the configured threshold (upstream v0.78, 58664dd).
pub fn get_jump_threshold(
    screen_pos: u8,
    screen_index: u32,
    direction: u8,
    config_threshold: u16,
) -> u16 {
    // On a non-main local screen every possible switch is local.
    if screen_index > 1 {
        return 0;
    }
    // On the main screen moving away from the border the switch is local too.
    if screen_pos == direction && screen_index == 1 {
        return 0;
    }
    // Everything else is a cross-output jump.
    config_threshold
}

/// Calculate mouse acceleration factor (fixed-point ×256).
/// Returns factor_fp where actual factor = factor_fp / 256.
pub fn calculate_mouse_acceleration_factor_fp(
    offset_x: i32,
    offset_y: i32,
    acceleration_enabled: bool,
) -> i32 {
    if offset_x == 0 && offset_y == 0 {
        return 256; // 1.0
    }

    if !acceleration_enabled {
        return 256; // 1.0
    }

    // Alpha-max-beta-min approximation for magnitude (no sqrt/float needed)
    let ax = offset_x.unsigned_abs();
    let ay = offset_y.unsigned_abs();
    let (max_v, min_v) = if ax > ay { (ax, ay) } else { (ay, ax) };
    // magnitude ≈ max + 0.4*min = max + (min * 102) / 256
    let magnitude = max_v + ((min_v * 102) >> 8);

    let curve = &ACCELERATION_CURVE;

    if magnitude <= curve[0].value as u32 {
        return curve[0].factor_fp;
    }

    if magnitude >= curve[ACCEL_POINTS - 1].value as u32 {
        return curve[ACCEL_POINTS - 1].factor_fp;
    }

    for i in 0..ACCEL_POINTS - 1 {
        if magnitude < curve[i + 1].value as u32 {
            let lower = &curve[i];
            let upper = &curve[i + 1];
            // Linear interpolation in fixed-point:
            // t = (mag - lower.val) * 256 / (upper.val - lower.val)
            let range = (upper.value - lower.value) as u32;
            if range == 0 { return lower.factor_fp; }
            let t = ((magnitude - lower.value as u32) * 256) / range;
            let factor_range = upper.factor_fp - lower.factor_fp;
            return lower.factor_fp + ((factor_range * t as i32) >> 8);
        }
    }

    256 // 1.0
}

/// Scale Y coordinate when switching between screens with different borders.
pub fn scale_y_coordinate(y: i16, from: (i32, i32), to: (i32, i32)) -> i16 {
    let (from_top, from_bottom) = from;
    let (to_top, to_bottom) = to;

    let from_range = MAX_SCREEN_COORD as i32 - from_top - from_bottom;
    let to_range = MAX_SCREEN_COORD as i32 - to_top - to_bottom;

    if from_range <= 0 || to_range <= 0 {
        return y;
    }

    let relative = y as i32 - from_top;
    let scaled = (relative as i64 * to_range as i64 / from_range as i64) as i32;
    let result = scaled + to_top;

    result.clamp(MIN_SCREEN_COORD as i32, MAX_SCREEN_COORD as i32) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_and_keep_on_screen_normal() {
        assert_eq!(move_and_keep_on_screen(1000, 10), 1010);
    }

    #[test]
    fn test_move_and_keep_on_screen_clamp_max() {
        assert_eq!(move_and_keep_on_screen(32760, 100), MAX_SCREEN_COORD as i32);
    }

    #[test]
    fn test_move_and_keep_on_screen_clamp_min() {
        assert_eq!(move_and_keep_on_screen(5, -100), MIN_SCREEN_COORD as i32);
    }

    #[test]
    fn test_screen_switch_no_switch() {
        assert_eq!(is_screen_switch_needed(16000, 10, 0), 0);
    }

    #[test]
    fn test_screen_switch_left() {
        assert_eq!(is_screen_switch_needed(0, -100, 0), -1);
    }

    #[test]
    fn test_screen_switch_right() {
        assert_eq!(is_screen_switch_needed(32767, 100, 0), 1);
    }

    #[test]
    fn test_jump_threshold_local_screen_is_zero() {
        // On a non-main local screen (index > 1) every switch is local — no gap.
        assert_eq!(get_jump_threshold(SCREEN_POS_LEFT, 2, SCREEN_POS_LEFT, 479), 0);
        assert_eq!(get_jump_threshold(SCREEN_POS_RIGHT, 3, SCREEN_POS_LEFT, 479), 0);
    }

    #[test]
    fn test_jump_threshold_main_screen_away_from_border_is_zero() {
        // On the main screen (index 1) moving away from the border (direction
        // == own pos) the switch is a local virtual-desktop change — no gap.
        assert_eq!(get_jump_threshold(SCREEN_POS_LEFT, 1, SCREEN_POS_LEFT, 479), 0);
        assert_eq!(get_jump_threshold(SCREEN_POS_RIGHT, 1, SCREEN_POS_RIGHT, 479), 0);
    }

    #[test]
    fn test_jump_threshold_cross_output_uses_config() {
        // Main screen, moving toward the border — the real cross-PC jump.
        assert_eq!(get_jump_threshold(SCREEN_POS_LEFT, 1, SCREEN_POS_RIGHT, 479), 479);
        assert_eq!(get_jump_threshold(SCREEN_POS_RIGHT, 1, SCREEN_POS_LEFT, 479), 479);
    }

    #[test]
    fn test_acceleration_disabled() {
        assert_eq!(calculate_mouse_acceleration_factor_fp(10, 5, false), 256);
    }

    #[test]
    fn test_acceleration_zero_movement() {
        assert_eq!(calculate_mouse_acceleration_factor_fp(0, 0, true), 256);
    }

    #[test]
    fn test_acceleration_small_movement() {
        // magnitude ≈ 1, below curve[0].value=2 → factor=1.0 (256)
        assert_eq!(calculate_mouse_acceleration_factor_fp(1, 0, true), 256);
    }

    #[test]
    fn test_acceleration_large_movement() {
        // magnitude = 100, above curve max (70) → factor=4.0 (1024)
        assert_eq!(calculate_mouse_acceleration_factor_fp(100, 0, true), 1024);
    }

    #[test]
    fn test_acceleration_mid_movement() {
        // magnitude ≈ 10, between curve[1](5) and curve[2](15)
        let factor = calculate_mouse_acceleration_factor_fp(10, 0, true);
        assert!(factor > 282 && factor < 358, "factor_fp={factor}"); // between 1.1 and 1.4
    }

    #[test]
    fn test_scale_y_same_borders() {
        assert_eq!(scale_y_coordinate(16000, (0, 0), (0, 0)), 16000);
    }

    #[test]
    fn test_scale_y_with_borders() {
        let result = scale_y_coordinate(16000, (1000, 1000), (2000, 2000));
        assert!(result > 0 && result < MAX_SCREEN_COORD);
    }
}

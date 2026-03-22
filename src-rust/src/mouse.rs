use crate::constants::{MAX_SCREEN_COORD, MIN_SCREEN_COORD};

const ACCEL_POINTS: usize = 7;

struct AccelPoint {
    value: i32,
    factor: f32,
}

const ACCELERATION_CURVE: [AccelPoint; ACCEL_POINTS] = [
    AccelPoint { value: 2, factor: 1.0 },
    AccelPoint { value: 5, factor: 1.1 },
    AccelPoint { value: 15, factor: 1.4 },
    AccelPoint { value: 30, factor: 1.9 },
    AccelPoint { value: 45, factor: 2.6 },
    AccelPoint { value: 60, factor: 3.4 },
    AccelPoint { value: 70, factor: 4.0 },
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

/// Check if movement would cross screen boundary.
// WORKAROUND(c-compat): Returns i32 (-1/0/1) instead of an enum to match
// C's screen_pos_e. Can be replaced with a proper Rust enum later.
pub fn is_screen_switch_needed(position: i32, offset: i32, jump_threshold: u16) -> i32 {
    if position + offset < MIN_SCREEN_COORD as i32 - jump_threshold as i32 {
        return -1; // LEFT
    }
    if position + offset > MAX_SCREEN_COORD as i32 + jump_threshold as i32 {
        return 1; // RIGHT
    }
    0 // NONE
}

/// Calculate mouse acceleration factor based on 2D movement magnitude.
/// Uses piecewise linear interpolation over a predefined curve.
pub fn calculate_mouse_acceleration_factor(
    offset_x: i32,
    offset_y: i32,
    acceleration_enabled: bool,
) -> f32 {
    if offset_x == 0 && offset_y == 0 {
        return 1.0;
    }

    if !acceleration_enabled {
        return 1.0;
    }

    // WORKAROUND(c-compat): C uses sqrtf() from libm. We use alpha-max-beta-min
    // approximation (max + 0.4*min) to avoid libm dependency on no_std.
    // Can switch to libm crate or core::intrinsics when available.
    let ax = (offset_x as i64).unsigned_abs();
    let ay = (offset_y as i64).unsigned_abs();
    let (max_v, min_v) = if ax > ay { (ax, ay) } else { (ay, ax) };
    let magnitude = (max_v as f32) + 0.4 * (min_v as f32);

    let curve = &ACCELERATION_CURVE;

    if magnitude <= curve[0].value as f32 {
        return curve[0].factor;
    }

    if magnitude >= curve[ACCEL_POINTS - 1].value as f32 {
        return curve[ACCEL_POINTS - 1].factor;
    }

    for i in 0..ACCEL_POINTS - 1 {
        if magnitude < curve[i + 1].value as f32 {
            let lower = &curve[i];
            let upper = &curve[i + 1];
            let t = (magnitude - lower.value as f32) / (upper.value as f32 - lower.value as f32);
            return lower.factor + t * (upper.factor - lower.factor);
        }
    }

    1.0
}

/// Scale Y coordinate when switching between screens of different sizes.
/// border_from/border_to are (top, bottom) tuples.
pub fn scale_y_coordinate(
    pointer_y: i16,
    border_from: (i32, i32),
    border_to: (i32, i32),
) -> i16 {
    let size_from = border_from.1 - border_from.0;
    let size_to = border_to.1 - border_to.0;

    if size_from == size_to {
        return pointer_y;
    }

    // Moving from smaller to bigger screen
    if size_from > size_to {
        return (border_to.0 + (size_to * pointer_y as i32) / MAX_SCREEN_COORD as i32) as i16;
    }

    // Moving from bigger to smaller screen
    if (pointer_y as i32) < border_from.0 {
        return MIN_SCREEN_COORD;
    }

    if (pointer_y as i32) > border_from.1 {
        return MAX_SCREEN_COORD;
    }

    (((pointer_y as i32 - border_from.0) * MAX_SCREEN_COORD as i32) / size_from) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_and_keep_on_screen_normal() {
        assert_eq!(move_and_keep_on_screen(100, 50), 150);
        assert_eq!(move_and_keep_on_screen(16000, 16000), 32000);
    }

    #[test]
    fn test_move_and_keep_on_screen_clamp_low() {
        assert_eq!(move_and_keep_on_screen(10, -100), 0);
        assert_eq!(move_and_keep_on_screen(0, -1), 0);
    }

    #[test]
    fn test_move_and_keep_on_screen_clamp_high() {
        assert_eq!(move_and_keep_on_screen(32000, 1000), 32767);
        assert_eq!(move_and_keep_on_screen(32767, 1), 32767);
    }

    #[test]
    fn test_is_screen_switch_needed() {
        assert_eq!(is_screen_switch_needed(100, -200, 0), -1); // LEFT
        assert_eq!(is_screen_switch_needed(32700, 200, 0), 1); // RIGHT
        assert_eq!(is_screen_switch_needed(16000, 100, 0), 0); // NONE
    }

    #[test]
    fn test_is_screen_switch_with_threshold() {
        // With threshold=100, need to go past -100 or 32867
        assert_eq!(is_screen_switch_needed(50, -50, 100), 0); // within threshold
        assert_eq!(is_screen_switch_needed(50, -200, 100), -1); // past threshold
    }

    #[test]
    fn test_acceleration_disabled() {
        assert_eq!(calculate_mouse_acceleration_factor(100, 100, false), 1.0);
    }

    #[test]
    fn test_acceleration_zero_movement() {
        assert_eq!(calculate_mouse_acceleration_factor(0, 0, true), 1.0);
    }

    #[test]
    fn test_acceleration_small_movement() {
        let factor = calculate_mouse_acceleration_factor(1, 0, true);
        assert!((factor - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_acceleration_large_movement() {
        let factor = calculate_mouse_acceleration_factor(70, 0, true);
        assert!(factor >= 3.0);
    }

    #[test]
    fn test_scale_y_same_size() {
        assert_eq!(scale_y_coordinate(16000, (0, 32767), (0, 32767)), 16000);
    }

    #[test]
    fn test_scale_y_smaller_to_bigger() {
        // from has bigger range (0..32767), to has smaller (8000..24000)
        let y = scale_y_coordinate(16383, (0, 32767), (8000, 24000));
        assert!(y > 8000 && y < 24000);
    }

    #[test]
    fn test_scale_y_out_of_bounds() {
        // pointer below from.top
        assert_eq!(scale_y_coordinate(-10, (100, 200), (0, 32767)), 0);
        // pointer above from.bottom
        assert_eq!(scale_y_coordinate(250, (100, 200), (0, 32767)), 32767);
    }
}

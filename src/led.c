/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, version 3.
 *
 * See the file LICENSE for the full license text.
 */

#include "main.h"

/* ==================================================== *
 * ========== Update pico and keyboard LEDs  ========== *
 * ==================================================== */

/* set_keyboard_leds and restore_leds are now Rust #[export_name] in callbacks.rs */

uint8_t toggle_led(void) {
    uint8_t new_led_state = gpio_get(GPIO_LED_PIN) ^ 1;
    gpio_put(GPIO_LED_PIN, new_led_state);

    return new_led_state;
}

/* blink_led() and led_blinking_task() are now in Rust:
   - blink_led: Indicator::blink() → C blink_led in hal_shim (sets blinks_left)
   - led_blinking_task: #[export_name] in hal/ffi/tasks.rs → service::tasks::led_blink_tick */

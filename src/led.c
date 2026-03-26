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

void set_keyboard_leds(uint8_t requested_led_state, device_t *state) {
    static uint8_t new_led_value;

    new_led_value = requested_led_state;
    if (state->keyboard_connected) {
        tuh_hid_set_report(state->kbd_dev_addr,
                           state->kbd_instance,
                           0,
                           HID_REPORT_TYPE_OUTPUT,
                           &new_led_value,
                           sizeof(uint8_t));
    }
}

void restore_leds(device_t *state) {
    /* Light up on-board LED if current board is active output */
    state->onboard_led_state = (state->active_output == BOARD_ROLE);
    gpio_put(GPIO_LED_PIN, state->onboard_led_state);

    /* Light up appropriate keyboard leds (if it's connected locally) */
    if (state->keyboard_connected) {
        uint8_t leds = state->keyboard_leds[state->active_output];
        set_keyboard_leds(leds, state);
    }
}

uint8_t toggle_led(void) {
    uint8_t new_led_state = gpio_get(GPIO_LED_PIN) ^ 1;
    gpio_put(GPIO_LED_PIN, new_led_state);

    return new_led_state;
}

/* blink_led() and led_blinking_task() are now in Rust:
   - blink_led: Indicator::blink() → C blink_led in hal_shim (sets blinks_left)
   - led_blinking_task: #[export_name] in hal/ffi/tasks.rs → service::tasks::led_blink_tick */

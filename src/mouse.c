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
#include <math.h>

#define MACOS_SWITCH_MOVE_X 10
#define MACOS_SWITCH_MOVE_COUNT 5
#define ACCEL_POINTS 7

/* Now implemented in Rust (src-rust/src/app/mouse.rs) */
extern int32_t rust_move_and_keep_on_screen(int32_t position, int32_t offset);
extern int32_t rust_is_screen_switch_needed(int32_t position, int32_t offset, uint16_t threshold);
extern float rust_calculate_mouse_acceleration_factor(int32_t offset_x, int32_t offset_y, bool enabled);
extern int16_t rust_scale_y_coordinate(int16_t pointer_y, int32_t from_top, int32_t from_bottom, int32_t to_top, int32_t to_bottom);

enum screen_pos_e is_screen_switch_needed(int position, int offset) {
    int32_t result = rust_is_screen_switch_needed(position, offset, global_state.config.jump_threshold);
    return (result == -1) ? LEFT : (result == 1) ? RIGHT : NONE;
}

int32_t move_and_keep_on_screen(int position, int offset) {
    return rust_move_and_keep_on_screen(position, offset);
}

float calculate_mouse_acceleration_factor(int32_t offset_x, int32_t offset_y) {
    return rust_calculate_mouse_acceleration_factor(offset_x, offset_y, global_state.config.enable_acceleration);
}

/* Returns LEFT if need to jump left, RIGHT if right, NONE otherwise */
enum screen_pos_e update_mouse_position(device_t *state, mouse_values_t *values) {
    output_t *current    = &state->config.output[state->active_output];
    uint8_t reduce_speed = 0;

    /* Check if we are configured to move slowly */
    if (state->mouse_zoom)
        reduce_speed = MOUSE_ZOOM_SCALING_FACTOR;

    /* Calculate movement */
    float acceleration_factor = calculate_mouse_acceleration_factor(values->move_x, values->move_y);
    int offset_x = round(values->move_x * acceleration_factor * (current->speed_x >> reduce_speed));
    int offset_y = round(values->move_y * acceleration_factor * (current->speed_y >> reduce_speed));

    /* Determine if our upcoming movement would stay within the screen */
    enum screen_pos_e switch_direction = is_screen_switch_needed(state->pointer_x, offset_x);

    /* Update movement */
    state->pointer_x = move_and_keep_on_screen(state->pointer_x, offset_x);
    state->pointer_y = move_and_keep_on_screen(state->pointer_y, offset_y);

    /* Update buttons state */
    state->mouse_buttons = values->buttons;

    return switch_direction;
}

extern void rust_output_mouse_report(device_t *dev, const uint8_t *report);

void output_mouse_report(mouse_report_t *report, device_t *state) {
    rust_output_mouse_report(state, (const uint8_t *)report);
}

int16_t scale_y_coordinate(int screen_from, int screen_to, device_t *state) {
    output_t *from = &state->config.output[screen_from];
    output_t *to   = &state->config.output[screen_to];
    return rust_scale_y_coordinate(state->pointer_y,
        from->border.top, from->border.bottom,
        to->border.top, to->border.bottom);
}

extern void rust_switch_to_another_pc(device_t *dev, uint32_t output_number, int output_to, int direction);

void switch_to_another_pc(
    device_t *state, output_t *output, int output_to, int direction) {
    rust_switch_to_another_pc(state, output->number, output_to, direction);
}

extern void rust_switch_virtual_desktop(device_t *dev, uint8_t os, int new_index, int direction);

void switch_virtual_desktop_macos(device_t *state, int direction) {
    /* Delegated to Rust */
}

void switch_virtual_desktop(device_t *state, output_t *output, int new_index, int direction) {
    rust_switch_virtual_desktop(state, output->os, new_index, direction);
    output->screen_index = new_index;
}

/*                               BORDER
                                   |
       .---------.    .---------.  |  .---------.    .---------.    .---------.
      ||    B2   ||  ||    B1   || | ||    A1   ||  ||    A2   ||  ||    A3   ||   (output, index)
      ||  extra  ||  ||   main  || | ||   main  ||  ||  extra  ||  ||  extra  ||   (main or extra)
       '---------'    '---------'  |  '---------'    '---------'    '---------'
          )___(          )___(     |     )___(          )___(          )___(
*/
void do_screen_switch(device_t *state, int direction) {
    output_t *output = &state->config.output[state->active_output];

    /* No switching allowed if explicitly disabled or in gaming mode */
    if (state->switch_lock || state->gaming_mode)
        return;

    /* We want to jump in the direction of the other computer */
    if (output->pos != direction) {
        if (output->screen_index == 1) { /* We are at the border -> switch outputs */
            /* No switching allowed if mouse button is held. Should only apply to the border! */
            if (state->mouse_buttons)
                return;

            switch_to_another_pc(state, output, 1 - state->active_output, direction);
        }
        /* If here, this output has multiple desktops and we are not on the main one */
        else
            switch_virtual_desktop(state, output, output->screen_index - 1, direction);
    }

    /* We want to jump away from the other computer, only possible if there is another screen to jump to */
    else if (output->screen_index < output->screen_count)
        switch_virtual_desktop(state, output, output->screen_index + 1, direction);
}

static inline bool extract_value(bool uses_id, int32_t *dst, report_val_t *src, uint8_t *raw_report, int len) {
    /* If HID Report ID is used, the report is prefixed by the report ID so we have to move by 1 byte */
    if (uses_id && (*raw_report++ != src->report_id))
        return false;

    *dst = get_report_value(raw_report, len, src);
    return true;
}

void extract_report_values(uint8_t *raw_report, int len, device_t *state, mouse_values_t *values, hid_interface_t *iface) {
    /* Interpret values depending on the current protocol used. */
    if (iface->protocol == HID_PROTOCOL_BOOT) {
        hid_mouse_report_t *mouse_report = (hid_mouse_report_t *)raw_report;

        values->move_x  = mouse_report->x;
        values->move_y  = mouse_report->y;
        values->wheel   = mouse_report->wheel;
        values->pan     = mouse_report->pan;
        values->buttons = mouse_report->buttons;
        return;
    }
    mouse_t *mouse = &iface->mouse;
    bool uses_id = iface->uses_report_id;

    extract_value(uses_id, &values->move_x, &mouse->move_x, raw_report, len);
    extract_value(uses_id, &values->move_y, &mouse->move_y, raw_report, len);
    extract_value(uses_id, &values->wheel, &mouse->wheel, raw_report, len);
    extract_value(uses_id, &values->pan, &mouse->pan, raw_report, len);

    if (!extract_value(uses_id, &values->buttons, &mouse->buttons, raw_report, len)) {
        values->buttons = state->mouse_buttons;
    }
}

mouse_report_t create_mouse_report(device_t *state, mouse_values_t *values) {
    mouse_report_t mouse_report = {
        .buttons = values->buttons,
        .x       = state->pointer_x,
        .y       = state->pointer_y,
        .wheel   = values->wheel,
        .pan     = values->pan,
        .mode    = ABSOLUTE,
    };

    /* Workaround for Windows multiple desktops */
    if (state->relative_mouse || state->gaming_mode) {
        mouse_report.x = values->move_x;
        mouse_report.y = values->move_y;
        mouse_report.mode = RELATIVE;
    }

    return mouse_report;
}

/* Now implemented in Rust (src-rust/src/hal/ffi/mouse_process.rs) */
extern void rust_process_mouse_report(uint8_t *raw_report, int len, uint8_t itf,
                                       void *iface, void *dev);

void process_mouse_report(uint8_t *raw_report, int len, uint8_t itf, hid_interface_t *iface) {
    rust_process_mouse_report(raw_report, len, itf, (void *)iface, (void *)&global_state);
}

/* ==================================================== *
 * Mouse Queue Section
 * ==================================================== */

void process_mouse_queue_task(device_t *state) {
    mouse_report_t report = {0};

    /* We need to be connected to the host to send messages */
    if (!state->tud_connected)
        return;

    /* Peek first, if there is anything there... */
    if (!queue_try_peek(&state->mouse_queue, &report))
        return;

    /* If we are suspended, let's wake the host up */
    if (tud_suspended())
        tud_remote_wakeup();

    /* If it's not ready, we'll try on the next pass */
    if (!tud_hid_n_ready(ITF_NUM_HID))
        return;

    /* Try sending it to the host, if it's successful */
    bool succeeded
        = tud_mouse_report(report.mode, report.buttons, report.x, report.y, report.wheel, report.pan);

    /* ... then we can remove it from the queue */
    if (succeeded)
        queue_try_remove(&state->mouse_queue, &report);
}

void queue_mouse_report(mouse_report_t *report, device_t *state) {
    /* It wouldn't be fun to queue up a bunch of messages and then dump them all on host */
    if (!state->tud_connected)
        return;

    queue_try_add(&state->mouse_queue, report);
}

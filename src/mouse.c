/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * Mouse logic has been ported to Rust (src-rust/src/app/mouse*.rs).
 * This file contains thin C wrappers and HAL-dependent functions.
 */

#include "main.h"

/* Rust-implemented mouse math functions */
extern int32_t rust_move_and_keep_on_screen(int32_t position, int32_t offset);
extern int32_t rust_is_screen_switch_needed(int32_t position, int32_t offset, uint16_t threshold);
extern float rust_calculate_mouse_acceleration_factor(int32_t offset_x, int32_t offset_y, bool enabled);
extern int16_t rust_scale_y_coordinate(int16_t pointer_y, int32_t from_top, int32_t from_bottom, int32_t to_top, int32_t to_bottom);
extern void rust_output_mouse_report(device_t *dev, const uint8_t *report);
extern void rust_switch_to_another_pc(device_t *dev, uint32_t output_number, int output_to, int direction);
extern void rust_switch_virtual_desktop(device_t *dev, uint8_t os, int new_index, int direction);
extern void rust_do_screen_switch(device_t *dev, int direction);
extern void rust_process_mouse_report(uint8_t *raw_report, int len, uint8_t itf, void *iface, void *dev);

/* Thin wrappers delegating to Rust */
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

void output_mouse_report(mouse_report_t *report, device_t *state) {
    rust_output_mouse_report(state, (const uint8_t *)report);
}

int16_t scale_y_coordinate(int screen_from, int screen_to, device_t *state) {
    output_t *from = &state->config.output[screen_from];
    output_t *to   = &state->config.output[screen_to];
    return rust_scale_y_coordinate(state->pointer_y,
        from->border.top, from->border.bottom, to->border.top, to->border.bottom);
}

void switch_to_another_pc(device_t *state, output_t *output, int output_to, int direction) {
    rust_switch_to_another_pc(state, output->number, output_to, direction);
}

void switch_virtual_desktop(device_t *state, output_t *output, int new_index, int direction) {
    rust_switch_virtual_desktop(state, output->os, new_index, direction);
    output->screen_index = new_index;
}

void do_screen_switch(device_t *state, int direction) {
    rust_do_screen_switch(state, direction);
}

void process_mouse_report(uint8_t *raw_report, int len, uint8_t itf, hid_interface_t *iface) {
    rust_process_mouse_report(raw_report, len, itf, (void *)iface, (void *)&global_state);
}

/* HAL-dependent: hid_interface_t access for mouse value extraction */
static inline bool extract_value(bool uses_id, int32_t *dst, report_val_t *src, uint8_t *raw_report, int len) {
    if (uses_id && (*raw_report++ != src->report_id))
        return false;
    *dst = get_report_value(raw_report, len, src);
    return true;
}

void extract_report_values(uint8_t *raw_report, int len, device_t *state, mouse_values_t *values, hid_interface_t *iface) {
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
    if (!extract_value(uses_id, &values->buttons, &mouse->buttons, raw_report, len))
        values->buttons = state->mouse_buttons;
}

/* HAL-dependent: queue_t + TinyUSB */
void process_mouse_queue_task(device_t *state) {
    mouse_report_t report = {0};
    if (!state->tud_connected) return;
    if (!queue_try_peek(&state->mouse_queue, &report)) return;
    if (tud_suspended()) tud_remote_wakeup();
    if (!tud_hid_n_ready(ITF_NUM_HID)) return;
    bool ok = tud_mouse_report(report.mode, report.buttons, report.x, report.y, report.wheel, report.pan);
    if (ok) queue_try_remove(&state->mouse_queue, &report);
}

void queue_mouse_report(mouse_report_t *report, device_t *state) {
    if (!state->tud_connected) return;
    queue_try_add(&state->mouse_queue, report);
}

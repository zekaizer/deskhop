/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * Keyboard logic ported to Rust. This file contains hotkey data,
 * thin wrappers, and HAL-dependent queue functions.
 */
#include "main.h"

/* Rust FFI */
extern bool rust_key_in_report(uint8_t, const uint8_t *);
extern bool rust_check_specific_hotkey(uint8_t, const uint8_t *, uint8_t, const uint8_t *);
extern void rust_update_kbd_state(const uint8_t *, uint8_t);
extern void rust_update_remote_kbd_state(const uint8_t *);
extern void rust_combine_kbd_states(uint8_t *);
extern void rust_send_key(device_t *);
extern void rust_release_all_keys_state(device_t *);
extern void rust_process_keyboard_report(uint8_t *, int, uint8_t, void *, void *);
extern void rust_process_consumer_report(const uint8_t *, int, uint8_t, void *, void *);
extern void rust_process_system_report(const uint8_t *, int, uint8_t, void *, void *);

/* ---- Hotkey definitions (C function pointers required) ---- */
hotkey_combo_t hotkeys[] = {
    {.modifier = HOTKEY_MODIFIER, .keys = {HOTKEY_TOGGLE}, .key_count = 1,
     .pass_to_os = false, .action_handler = &output_toggle_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_RIGHTALT | KEYBOARD_MODIFIER_RIGHTCTRL,
     .keys = {}, .key_count = 0, .pass_to_os = true, .acknowledge = true,
     .action_handler = &mouse_zoom_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_RIGHTCTRL, .keys = {HID_KEY_K}, .key_count = 1,
     .acknowledge = true, .action_handler = &switchlock_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_RIGHTCTRL, .keys = {HID_KEY_L}, .key_count = 1,
     .acknowledge = true, .action_handler = &screenlock_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT,
     .keys = {HID_KEY_G}, .key_count = 1, .acknowledge = true,
     .action_handler = &toggle_gaming_mode_handler},
    {.modifier = KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT,
     .keys = {HID_KEY_S}, .key_count = 1, .acknowledge = true,
     .action_handler = &enable_screensaver_pong_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT,
     .keys = {HID_KEY_J}, .key_count = 1, .acknowledge = true,
     .action_handler = &enable_screensaver_jitter_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT,
     .keys = {HID_KEY_X}, .key_count = 1, .acknowledge = true,
     .action_handler = &disable_screensaver_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_RIGHTSHIFT, .keys = {HID_KEY_F12, HID_KEY_D},
     .key_count = 2, .acknowledge = true, .action_handler = &wipe_config_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_RIGHTSHIFT, .keys = {HID_KEY_F12, HID_KEY_Y},
     .key_count = 2, .acknowledge = true, .action_handler = &screen_border_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT,
     .keys = {HID_KEY_C, HID_KEY_O}, .key_count = 2, .acknowledge = true,
     .action_handler = &config_enable_hotkey_handler},
    {.modifier = KEYBOARD_MODIFIER_RIGHTSHIFT | KEYBOARD_MODIFIER_LEFTSHIFT,
     .keys = {HID_KEY_A}, .key_count = 1, .acknowledge = true,
     .action_handler = &fw_upgrade_hotkey_handler_A},
    {.modifier = KEYBOARD_MODIFIER_RIGHTSHIFT | KEYBOARD_MODIFIER_LEFTSHIFT,
     .keys = {HID_KEY_B}, .key_count = 1, .acknowledge = true,
     .action_handler = &fw_upgrade_hotkey_handler_B},
};

/* ---- Wrappers ---- */
bool key_in_report(uint8_t k, const hid_keyboard_report_t *r) { return rust_key_in_report(k, (const uint8_t *)r); }
bool check_specific_hotkey(hotkey_combo_t h, const hid_keyboard_report_t *r) {
    return rust_check_specific_hotkey(h.modifier, h.keys, h.key_count, (const uint8_t *)r);
}

hotkey_combo_t *check_all_hotkeys(hid_keyboard_report_t *report, device_t *state) {
    for (int n = 0; n < ARRAY_SIZE(hotkeys); n++)
        if (check_specific_hotkey(hotkeys[n], report)) return &hotkeys[n];
    return NULL;
}

void update_kbd_state(device_t *s, hid_keyboard_report_t *r, uint8_t i) { rust_update_kbd_state((const uint8_t *)r, i); }
void update_remote_kbd_state(device_t *s, hid_keyboard_report_t *r) { rust_update_remote_kbd_state((const uint8_t *)r); }
void release_all_keys(device_t *s) { rust_release_all_keys_state(s); }
void combine_kbd_states(device_t *s, hid_keyboard_report_t *o) { rust_combine_kbd_states((uint8_t *)o); }
void send_key(hid_keyboard_report_t *r, device_t *s) { rust_send_key(s); }
void process_keyboard_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_keyboard_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_consumer_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_consumer_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_system_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_system_report(r, l, i, (void *)f, (void *)&global_state);
}

/* HAL-dependent: hid_interface_t access */
keyboard_t *get_keyboard(hid_interface_t *iface, uint8_t report_id) {
    if (iface->num_keyboards == 1 || !iface->uses_report_id)
        return &iface->keyboards[PRIMARY_KEYBOARD];
    for (int i = 0; i < iface->num_keyboards && i < MAX_KEYBOARDS; i++)
        if (iface->keyboards[i].report_id == report_id) return &iface->keyboards[i];
    return &iface->keyboards[PRIMARY_KEYBOARD];
}

/* HAL-dependent: queue_t + TinyUSB */
void process_kbd_queue_task(device_t *state) {
    hid_keyboard_report_t report;
    if (!state->tud_connected) return;
    if (!queue_try_peek(&state->kbd_queue, &report)) return;
    if (tud_suspended()) tud_remote_wakeup();
    if (!tud_hid_n_ready(ITF_NUM_HID)) return;
    bool ok = tud_hid_keyboard_report(REPORT_ID_KEYBOARD, report.modifier, report.keycode);
    if (ok) queue_try_remove(&state->kbd_queue, &report);
}

void queue_kbd_report(hid_keyboard_report_t *report, device_t *state) {
    if (!state->tud_connected) return;
    queue_try_add(&state->kbd_queue, report);
}

extern void rust_send_consumer_control(device_t *, const uint8_t *);
extern void rust_send_system_control(device_t *, const uint8_t *);
void send_consumer_control(uint8_t *r, device_t *s) { rust_send_consumer_control(s, r); }
void send_system_control(uint8_t *r, device_t *s) { rust_send_system_control(s, r); }

/* DeskHop keyboard — logic in Rust. Hotkey data + check_all + queue here. */
#include "main.h"

extern bool rust_check_specific_hotkey(uint8_t, const uint8_t *, uint8_t, const uint8_t *);
extern void rust_release_all_keys_state(device_t *),
    rust_process_keyboard_report(uint8_t *, int, uint8_t, void *, void *),
    rust_process_kbd_queue_task(device_t *), rust_queue_kbd_report(device_t *, const uint8_t *);

hotkey_combo_t hotkeys[] = {
    {.modifier=HOTKEY_MODIFIER, .keys={HOTKEY_TOGGLE}, .key_count=1, .action_handler=&output_toggle_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_RIGHTALT|KEYBOARD_MODIFIER_RIGHTCTRL, .key_count=0, .pass_to_os=true, .acknowledge=true, .action_handler=&mouse_zoom_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_RIGHTCTRL, .keys={HID_KEY_K}, .key_count=1, .acknowledge=true, .action_handler=&switchlock_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_RIGHTCTRL, .keys={HID_KEY_L}, .key_count=1, .acknowledge=true, .action_handler=&screenlock_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_LEFTCTRL|KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_G}, .key_count=1, .acknowledge=true, .action_handler=&toggle_gaming_mode_handler},
    {.modifier=KEYBOARD_MODIFIER_LEFTCTRL|KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_S}, .key_count=1, .acknowledge=true, .action_handler=&enable_screensaver_pong_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_LEFTCTRL|KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_J}, .key_count=1, .acknowledge=true, .action_handler=&enable_screensaver_jitter_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_LEFTCTRL|KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_X}, .key_count=1, .acknowledge=true, .action_handler=&disable_screensaver_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_F12,HID_KEY_D}, .key_count=2, .acknowledge=true, .action_handler=&wipe_config_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_F12,HID_KEY_Y}, .key_count=2, .acknowledge=true, .action_handler=&screen_border_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_LEFTCTRL|KEYBOARD_MODIFIER_RIGHTSHIFT, .keys={HID_KEY_C,HID_KEY_O}, .key_count=2, .acknowledge=true, .action_handler=&config_enable_hotkey_handler},
    {.modifier=KEYBOARD_MODIFIER_RIGHTSHIFT|KEYBOARD_MODIFIER_LEFTSHIFT, .keys={HID_KEY_A}, .key_count=1, .acknowledge=true, .action_handler=&fw_upgrade_hotkey_handler_A},
    {.modifier=KEYBOARD_MODIFIER_RIGHTSHIFT|KEYBOARD_MODIFIER_LEFTSHIFT, .keys={HID_KEY_B}, .key_count=1, .acknowledge=true, .action_handler=&fw_upgrade_hotkey_handler_B},
};

bool check_specific_hotkey(hotkey_combo_t h, const hid_keyboard_report_t *r) {
    return rust_check_specific_hotkey(h.modifier, h.keys, h.key_count, (const uint8_t *)r);
}
hotkey_combo_t *check_all_hotkeys(hid_keyboard_report_t *r, device_t *s) {
    for (int n = 0; n < ARRAY_SIZE(hotkeys); n++)
        if (check_specific_hotkey(hotkeys[n], r)) return &hotkeys[n];
    return NULL;
}

/* These must remain — called from other C code */
void release_all_keys(device_t *s) { rust_release_all_keys_state(s); }
void process_keyboard_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_keyboard_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_kbd_queue_task(device_t *s) { rust_process_kbd_queue_task(s); }
void queue_kbd_report(hid_keyboard_report_t *r, device_t *s) { rust_queue_kbd_report(s, (const uint8_t *)r); }

/* These are referenced by hal_set_report_handler for TinyUSB callback routing */
extern void rust_process_consumer_report(const uint8_t *, int, uint8_t, void *, void *);
extern void rust_process_system_report(const uint8_t *, int, uint8_t, void *, void *);
void process_consumer_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_consumer_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_system_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_system_report(r, l, i, (void *)f, (void *)&global_state);
}

keyboard_t *get_keyboard(hid_interface_t *i, uint8_t rid) {
    if (i->num_keyboards == 1 || !i->uses_report_id) return &i->keyboards[PRIMARY_KEYBOARD];
    for (int n = 0; n < i->num_keyboards && n < MAX_KEYBOARDS; n++)
        if (i->keyboards[n].report_id == rid) return &i->keyboards[n];
    return &i->keyboards[PRIMARY_KEYBOARD];
}

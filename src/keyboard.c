/* DeskHop keyboard — hotkey matching + check_all moved to Rust.
   Only C wrappers for TinyUSB callback routing remain. */
#include "main.h"

extern void rust_release_all_keys_state(device_t *),
    rust_process_keyboard_report(uint8_t *, int, uint8_t, void *, void *),
    rust_process_kbd_queue_task(device_t *), rust_queue_kbd_report(device_t *, const uint8_t *);

/* These must remain — called from other C code */
void release_all_keys(device_t *s) { rust_release_all_keys_state(s); }
void process_keyboard_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_keyboard_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_kbd_queue_task(device_t *s) { rust_process_kbd_queue_task(s); }

/* These are referenced by hal_set_report_handler for TinyUSB callback routing */
extern void rust_process_consumer_report(const uint8_t *, int, uint8_t, void *, void *);
extern void rust_process_system_report(const uint8_t *, int, uint8_t, void *, void *);
void process_consumer_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_consumer_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_system_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_system_report(r, l, i, (void *)f, (void *)&global_state);
}

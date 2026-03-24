/* DeskHop mouse — all logic in Rust. C wrappers for TinyUSB callback routing. */
#include "main.h"

extern void rust_process_mouse_report(uint8_t *, int, uint8_t, void *, void *);
extern void rust_process_mouse_queue_task(device_t *);

void process_mouse_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_mouse_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_mouse_queue_task(device_t *s) { rust_process_mouse_queue_task(s); }

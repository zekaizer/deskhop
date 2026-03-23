/* DeskHop mouse — all logic in Rust. C wrappers for queue + process_mouse_report. */
#include "main.h"

extern void rust_process_mouse_report(uint8_t *, int, uint8_t, void *, void *);
extern void rust_process_mouse_queue_task(device_t *), rust_queue_mouse_report(device_t *, const uint8_t *);

void process_mouse_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_mouse_report(r, l, i, (void *)f, (void *)&global_state);
}
void process_mouse_queue_task(device_t *s) { rust_process_mouse_queue_task(s); }
void queue_mouse_report(mouse_report_t *r, device_t *s) { rust_queue_mouse_report(s, (const uint8_t *)r); }

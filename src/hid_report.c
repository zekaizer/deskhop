/* DeskHop HID report — all logic in Rust. Thin C wrappers remain for
   functions still called from other C code. */
#include "hid_report.h"
#include "main.h"

extern int32_t rust_get_report_value(const uint8_t *, int, const uint8_t *);
extern int32_t rust_extract_kbd_data(uint8_t *, int, uint8_t, void *, uint8_t *);
extern void rust_extract_data(void *, const uint8_t *);

int32_t get_report_value(uint8_t *r, int l, report_val_t *v) { return rust_get_report_value(r, l, (const uint8_t *)v); }
int32_t extract_kbd_data(uint8_t *r, int l, uint8_t i, hid_interface_t *f, hid_keyboard_report_t *o) {
    return rust_extract_kbd_data(r, l, i, (void *)f, (uint8_t *)o);
}
void extract_data(hid_interface_t *iface, report_val_t *val) {
    rust_extract_data((void *)iface, (const uint8_t *)val);
}

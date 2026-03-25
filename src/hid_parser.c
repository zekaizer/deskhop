/* DeskHop HID parser — ported to Rust. C wrapper only. */
#include "main.h"

extern void rust_parse_report_descriptor(void *, const uint8_t *, int);

void parse_report_descriptor(hid_interface_t *i, uint8_t const *r, int l) {
    rust_parse_report_descriptor((void *)i, r, l);
}

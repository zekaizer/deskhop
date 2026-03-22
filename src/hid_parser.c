/* DeskHop HID parser — ported to Rust. C wrappers only. */
#include "main.h"

extern uint32_t rust_get_descriptor_value(const uint8_t *, int);
extern void rust_parse_report_descriptor(void *, const uint8_t *, int);

uint32_t get_descriptor_value(uint8_t const *r, int s) { return rust_get_descriptor_value(r, s); }
void parse_report_descriptor(hid_interface_t *i, uint8_t const *r, int l) {
    rust_parse_report_descriptor((void *)i, r, l);
}

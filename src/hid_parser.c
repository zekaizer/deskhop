/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * HID descriptor parser has been ported to Rust (src-rust/src/app/hid_parser.rs).
 * This file contains only thin C wrappers that delegate to Rust via FFI.
 */
#include "main.h"

/* Rust-implemented HID parser functions */
extern uint32_t rust_get_descriptor_value(const uint8_t *report, int size);
extern void rust_parse_report_descriptor(void *iface, const uint8_t *report, int desc_len);

uint32_t get_descriptor_value(uint8_t const *report, int size) {
    return rust_get_descriptor_value(report, size);
}

void parse_report_descriptor(hid_interface_t *iface,
                            uint8_t const *report,
                            int desc_len) {
    rust_parse_report_descriptor((void *)iface, report, desc_len);
}

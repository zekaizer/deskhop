/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, version 3.
 *
 * See the file LICENSE for the full license text.
 */

#include "main.h"

_Static_assert(MAX_DEVICES <= CFG_TUH_DEVICE_MAX,
               "MAX_DEVICES must not exceed CFG_TUH_DEVICE_MAX");

/* ================================================== *
 * ===========  TinyUSB Device Callbacks  =========== *
 * ================================================== */

/* Invoked when we get GET_REPORT control request.
 * We are expected to fill buffer with the report content, update reqlen
 * and return its length. We return 0 to STALL the request. */
uint16_t tud_hid_get_report_cb(uint8_t instance,
                               uint8_t report_id,
                               hid_report_type_t report_type,
                               uint8_t *buffer,
                               uint16_t request_len) {
    return 0;
}

void tud_hid_set_report_cb(uint8_t instance,
                           uint8_t report_id,
                           hid_report_type_t report_type,
                           uint8_t const *buffer,
                           uint16_t bufsize) {
    rust_on_tud_set_report(instance, report_id, (uint8_t)report_type, buffer, bufsize);
}

/* Invoked when device is mounted */
void tud_mount_cb(void) {
    global_cfg.tud_connected = true;
}

/* Invoked when device is unmounted */
void tud_umount_cb(void) {
    global_cfg.tud_connected = false;
}

#ifdef DH_DEBUG_CDC_FLASH
void tud_cdc_rx_cb(uint8_t itf) {
    char buf[64];
    uint32_t count = tud_cdc_n_available(itf);

    if (count == 0)
        return;

    if (count > sizeof(buf))
        count = sizeof(buf);

    tud_cdc_n_read(itf, buf, count);

    if (count >= 5 && memcmp(buf, "flash", 5) == 0) {
        reset_usb_boot(0, 0);
    }
}
#endif

/* ================================================== *
 * ===============  USB HOST Section  =============== *
 * ================================================== */

/* Thin stubs — resolve opaque iface pointer, delegate all logic to Rust */

void tuh_hid_umount_cb(uint8_t dev_addr, uint8_t instance) {
    if (dev_addr > MAX_DEVICES || instance >= MAX_INTERFACES)
        return;
    hid_interface_t *iface = iface_from_opaque(&global_hw.iface[dev_addr-1][instance]);
    rust_on_hid_umount(dev_addr, instance, iface);
}

void tuh_hid_mount_cb(uint8_t dev_addr, uint8_t instance, uint8_t const *desc_report, uint16_t desc_len) {
    if (dev_addr > MAX_DEVICES || instance >= MAX_INTERFACES)
        return;
    hid_interface_t *iface = iface_from_opaque(&global_hw.iface[dev_addr-1][instance]);
    rust_on_hid_mount(dev_addr, instance, desc_report, desc_len, iface);
}

void tuh_hid_report_received_cb(uint8_t dev_addr, uint8_t instance, uint8_t const *report, uint16_t len) {
    if (dev_addr > MAX_DEVICES || instance >= MAX_INTERFACES)
        return;
    hid_interface_t *iface = iface_from_opaque(&global_hw.iface[dev_addr-1][instance]);
    rust_on_hid_report_received(dev_addr, instance, report, len, iface);
}

void tuh_hid_set_protocol_complete_cb(uint8_t dev_addr, uint8_t idx, uint8_t protocol) {
    if (dev_addr > MAX_DEVICES || idx > MAX_INTERFACES)
        return;
    hid_interface_t *iface = iface_from_opaque(&global_hw.iface[dev_addr-1][idx]);
    rust_on_hid_set_protocol_complete(iface, protocol);
}

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

extern void rust_on_tud_mount(void);
extern void rust_on_tud_umount(void);

void tud_mount_cb(void) { rust_on_tud_mount(); }
void tud_umount_cb(void) { rust_on_tud_umount(); }

#if defined(DH_DEBUG) || defined(DH_DEBUG_CDC_FLASH)
extern void peer_log_request_replay(void);
extern void rust_dbg_toggle_ptr_log(void);

/* Incrementally match a literal command in a byte stream (works whether the
 * terminal sends it at once or char by char). On full match, *matched resets
 * and the function returns true. */
static bool cdc_cmd_match(const char *cmd, uint8_t cmd_len, char c, uint8_t *matched) {
    if (c == cmd[*matched]) {
        if (++(*matched) == cmd_len) {
            *matched = 0;
            return true;
        }
    } else {
        *matched = (c == cmd[0]) ? 1 : 0;
    }
    return false;
}

void tud_cdc_rx_cb(uint8_t itf) {
    char buf[64];
    uint32_t count = tud_cdc_n_available(itf);

    if (count == 0)
        return;

    if (count > sizeof(buf))
        count = sizeof(buf);

    tud_cdc_n_read(itf, buf, count);

#ifdef DH_DEBUG_CDC_FLASH
    if (count >= 5 && memcmp(buf, "flash", 5) == 0) {
        reset_usb_boot(0, 0);
    }
#endif

#ifdef DH_DEBUG
    /* Deliberate CDC commands (matched incrementally so a stray byte can't
     * trigger them): "logdump" re-dumps the log scrollback; "ptr" toggles the
     * noisy pointer-stream summary. */
    {
        static const char LOGDUMP[] = "logdump";
        static const char PTR[] = "ptr";
        static uint8_t m_logdump = 0, m_ptr = 0;
        for (uint32_t i = 0; i < count; i++) {
            if (cdc_cmd_match(LOGDUMP, sizeof(LOGDUMP) - 1, buf[i], &m_logdump))
                peer_log_request_replay();
            if (cdc_cmd_match(PTR, sizeof(PTR) - 1, buf[i], &m_ptr))
                rust_dbg_toggle_ptr_log();
        }
    }
#endif
}
#endif

#ifdef DH_DEBUG
extern void rust_on_cdc_line_state(bool dtr, bool rts);

/* Logs DTR/RTS transitions so we can see whether a host terminal toggles DTR on
 * open — the edge the scrollback replay depends on. */
void tud_cdc_line_state_cb(uint8_t itf, bool dtr, bool rts) {
    (void)itf;
    rust_on_cdc_line_state(dtr, rts);
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

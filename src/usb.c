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

/**
 * Computer controls our LEDs by sending USB SetReport messages with a payload
 * of just 1 byte and report type output. It's type 0x21 (USB_REQ_DIR_OUT |
 * USB_REQ_TYP_CLASS | USB_REQ_REC_IFACE) Request code for SetReport is 0x09,
 * report type is 0x02 (HID_REPORT_TYPE_OUTPUT). We get a set_report callback
 * from TinyUSB device HID and then figure out what to do with the LEDs.
 */
void tud_hid_set_report_cb(uint8_t instance,
                           uint8_t report_id,
                           hid_report_type_t report_type,
                           uint8_t const *buffer,
                           uint16_t bufsize) {

    /* Passthrough: forward output reports from host (Win11/Options+) to receiver */
    passthrough_state_t *pt = passthrough_get_state();
    if (pt->active && !global_state.config_mode_active && instance >= ITF_NUM_PT_BASE) {
        int8_t idx = passthrough_device_to_host_index(pt, instance);
        if (idx >= 0 && bufsize <= sizeof(pt->out_queue.data)) {
            /* Queue for deferred send from main loop — SET_REPORT control
             * transfers fail when called from TinyUSB device callbacks. */
            pt->out_queue.dev_addr    = pt->ifaces[idx].dev_addr;
            pt->out_queue.instance    = pt->ifaces[idx].instance;
            pt->out_queue.report_id   = report_id;
            pt->out_queue.report_type = report_type;
            pt->out_queue.len         = bufsize;
            memcpy(pt->out_queue.data, buffer, bufsize);
            pt->out_queue.pending     = true;

            /* Passive sniffing: learn feature indices from host software commands.
             * Options+ uses FeatureSet (not IRoot) for discovery, then sends
             * characteristic setup commands to each feature. We detect these:
             *   - FeatureSet fn=1 queries: track index → match response for fid
             *   - setWheelMode(fn=2, param=0x03): identifies HiResScroll fi
             *   - setThumbwheelReporting(fn=2, param=0x01): identifies Thumbwheel fi */
            if (report_id == HIDPP_REPORT_ID_SHORT && bufsize >= 4
                && buffer[0] >= 1 && buffer[0] <= 6
                && (buffer[2] & 0x0F) != 0) {
                hidpp_discovery_t *d = &pt->hidpp_disc;
                uint8_t fi  = buffer[1];
                uint8_t fn  = (buffer[2] >> 4) & 0x0F;
                uint8_t p0  = bufsize > 3 ? buffer[3] : 0;

                /* Feature discovery (skip if already known) */
                if (d->fi_hires_scroll == 0 || d->fi_thumbwheel == 0) {
                    if (fn == 2 && p0 == 0x03 && d->fi_hires_scroll == 0) {
                        d->fi_hires_scroll = fi;
                        d->device_idx = buffer[0];
                        dh_debug_printf("[SNIFF] HiResScroll → fi=0x%02X dev=%d\n",
                                        fi, buffer[0]);
                    }
                    if (fn == 2 && p0 == 0x01
                        && d->fi_thumbwheel == 0 && d->fi_hires_scroll != 0
                        && fi != d->fi_hires_scroll
                        && buffer[0] == d->device_idx) {
                        d->fi_thumbwheel = fi;
                        dh_debug_printf("[SNIFF] Thumbwheel → fi=0x%02X\n", fi);
                    }
                }

            }
        }
        return;
    }

    /* We received a report on the config report ID */
    if (instance == ITF_NUM_HID_VENDOR && report_id == REPORT_ID_VENDOR) {
        /* Security - only if config mode is enabled are we allowed to do anything. While the report_id
           isn't even advertised when not in config mode, security must always be explicit and never assume */
        if (!global_state.config_mode_active)
            return;

        /* We insist on a fixed size packet. No overflows. */
        if (bufsize != RAW_PACKET_LENGTH)
            return;

        uart_packet_t *packet = (uart_packet_t *) (buffer + START_LENGTH);

        /* Only a certain packet types are accepted */
        if (!validate_packet(packet))
            return;

        process_packet(packet, &global_state);
    }

    /* Only other set report we care about is LED state change, and that's exactly 1 byte long */
    if (report_id != REPORT_ID_KEYBOARD || bufsize != 1 || report_type != HID_REPORT_TYPE_OUTPUT)
        return;

    uint8_t leds = buffer[0];

    /* If we are using caps lock LED to indicate the chosen output, that has priority */
    if (global_state.config.kbd_led_as_indicator) {
        leds = leds & 0xFD; /* 1111 1101 (Clear Caps Lock bit) */

        if (global_state.active_output)
            leds |= KEYBOARD_LED_CAPSLOCK;
    }

    global_state.keyboard_leds[BOARD_ROLE] = leds;

    /* If the board has a keyboard connected directly, restore those leds. */
    if (global_state.keyboard_connected && CURRENT_BOARD_IS_ACTIVE_OUTPUT)
        restore_leds(&global_state);

    /* Always send to the other one, so it is aware of the change */
    send_value(leds, KBD_SET_REPORT_MSG);
}

/* Invoked when device is mounted */
void tud_mount_cb(void) {
    global_state.tud_connected = true;
}

/* Invoked when device is unmounted */
void tud_umount_cb(void) {
    global_state.tud_connected = false;
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

/* SET_REPORT completion callback — verify control transfer actually reached the device */
void tuh_hid_set_report_complete_cb(uint8_t dev_addr, uint8_t idx,
                                     uint8_t report_id, uint8_t report_type,
                                     uint16_t len) {
    if (!len)
        dh_debug_printf("[PT] SET_REPORT FAIL dev=%d idx=%d rid=0x%02X\n",
                        dev_addr, idx, report_id);
}

/* ================================================== *
 * ===============  USB HOST Section  =============== *
 * ================================================== */

void tuh_hid_umount_cb(uint8_t dev_addr, uint8_t instance) {
    uint8_t itf_protocol = tuh_hid_interface_protocol(dev_addr, instance);

    if (dev_addr > MAX_DEVICES || instance >= MAX_INTERFACES)
        return;

    hid_interface_t *iface = &global_state.iface[dev_addr-1][instance];

    switch (itf_protocol) {
        case HID_ITF_PROTOCOL_KEYBOARD:
            global_state.keyboard_connected = false;
            break;

        case HID_ITF_PROTOCOL_MOUSE:
            global_state.mouse_connected = false;
            break;
    }

    /* Also clear the interface structure, otherwise plugging something else later
       might be a fun (and confusing) experience */
    memset(iface, 0, sizeof(hid_interface_t));

    /* Clean up passthrough state for this device */
    passthrough_state_t *pt = passthrough_get_state();
    passthrough_remove_device(pt, dev_addr);

    /* Re-enumerate as DeskHop when all passthrough interfaces are gone (FR-PT-010) */
    if (pt->active && pt->iface_count == 0) {
        dh_debug_printf("[PT] All interfaces removed, reverting to DeskHop identity\n");
        pt->active = false;
        pt->config_desc_len = 0;
        tud_disconnect();
        pt->reconnect_at_us = time_us_64() + _MS(200);
    }
}

void tuh_hid_mount_cb(uint8_t dev_addr, uint8_t instance, uint8_t const *desc_report, uint16_t desc_len) {
    uint8_t itf_protocol = tuh_hid_interface_protocol(dev_addr, instance);

    if (dev_addr > MAX_DEVICES || instance >= MAX_INTERFACES)
        return;

    /* Get interface information */
    hid_interface_t *iface = &global_state.iface[dev_addr-1][instance];

    iface->protocol = tuh_hid_get_protocol(dev_addr, instance);

    /* Capture raw descriptor for Semi-DDM passthrough */
    passthrough_state_t *pt = passthrough_get_state();
    passthrough_capture_descriptor(pt, dev_addr, instance, itf_protocol, desc_report, desc_len);
    pt->last_capture_us = time_us_64();

    /* Capture upstream VID/PID once per device (FR-PT-009) */
    if (pt->upstream_vid == 0) {
        tuh_vid_pid_get(dev_addr, &pt->upstream_vid, &pt->upstream_pid);
        dh_debug_printf("[PT] Upstream device VID=%04X PID=%04X\n",
                        pt->upstream_vid, pt->upstream_pid);
    }

    dh_debug_printf("[PT] Mount dev=%d inst=%d proto=%d\n",
                    dev_addr, instance, itf_protocol);

    /* Parse the report descriptor into our internal structure. */
    parse_report_descriptor(iface, desc_report, desc_len);

    switch (itf_protocol) {
        case HID_ITF_PROTOCOL_KEYBOARD:
            if (global_state.config.enforce_ports && BOARD_ROLE == OUTPUT_B)
                return;

            if (global_state.config.force_kbd_boot_protocol)
                tuh_hid_set_protocol(dev_addr, instance, HID_PROTOCOL_BOOT);

            /* Keeping this is required for setting leds from device set_report callback */
            global_state.kbd_dev_addr       = dev_addr;
            global_state.kbd_instance       = instance;
            global_state.keyboard_connected = true;
            break;

        case HID_ITF_PROTOCOL_MOUSE:
            if (global_state.config.enforce_ports && BOARD_ROLE == OUTPUT_A)
                return;

            if (global_state.config.force_mouse_boot_mode) {
                /* User requested boot mode - simpler protocol for compatibility.
                   Note: many mice still send wheel data even in boot mode. */
                tuh_hid_set_protocol(dev_addr, instance, HID_PROTOCOL_BOOT);
            } else {
                /* Switch to using report protocol instead of boot, it's more complicated but
                   at least we get all the information we need (looking at you, mouse wheel) */
                if (tuh_hid_get_protocol(dev_addr, instance) == HID_PROTOCOL_BOOT) {
                    tuh_hid_set_protocol(dev_addr, instance, HID_PROTOCOL_REPORT);
                }
            }
            global_state.mouse_connected = true;
            break;

        case HID_ITF_PROTOCOL_NONE:
            break;
    }

    /* Also set mouse_connected if report descriptor contains mouse, even if interface
       protocol says keyboard. This handles composite devices like QMK. */
    if (iface->mouse.is_found) {
        global_state.mouse_connected = true;
    }

    /* Flash local led to indicate a device was connected */
    blink_led(&global_state);

    /* Also signal the other board to flash LED, to enable easy verification if serial works */
    send_value(ENABLE, FLASH_LED_MSG);

    /* Kick off the report querying */
    tuh_hid_receive_report(dev_addr, instance);
}

/* Invoked when received report from device via interrupt endpoint */
void tuh_hid_report_received_cb(uint8_t dev_addr, uint8_t instance, uint8_t const *report, uint16_t len) {
    uint8_t const itf_protocol = tuh_hid_interface_protocol(dev_addr, instance);

    if (dev_addr > MAX_DEVICES || instance >= MAX_INTERFACES)
        return;

    /* Passthrough: forward raw non-keyboard reports to device side.
     * Keyboard always goes through DeskHop parsing for hotkey/remap (FR-PT-007).
     * When active output: raw passthrough replaces DeskHop mouse processing.
     * When not active: fall through to DeskHop processing (mouse → UART → other board). */
    passthrough_state_t *pt = passthrough_get_state();
    if (pt->active && !global_state.config_mode_active
        && itf_protocol != HID_ITF_PROTOCOL_KEYBOARD) {
        int8_t dev_inst = passthrough_host_to_device_instance(pt, dev_addr, instance);
        if (dev_inst >= 0) {
            int8_t idx = dev_inst - ITF_NUM_PT_BASE;

            /* HID++ vendor interfaces: route by message type (sw_id classification).
             * Protocol responses (sw_id!=0) always go to A for Options+ etc.
             * Input events (sw_id==0) only go to active output; dropped on inactive
             * (P3 will convert these to standard mouse reports via UART). */
            bool handled = false;

            if (pt->ifaces[idx].always_passthrough) {
                /* Intercept DeskHop sw_id responses (scan only) */
                if (len >= 7 && (report[3] & 0x0F) == HIDPP_SWID_DESKHOP) {
                    if (pt->hidpp_scan.state == SCAN_QUERY_IROOT)
                        passthrough_handle_scan_response(pt, report, len);
                    tuh_hid_receive_report(dev_addr, instance);
                    return;
                }

                bool is_input = passthrough_is_hidpp_input_event(report, len);

                /* Intercept SmartShift button (CID 0xC4) for A/B output switch.
                 * Don't consume — let Options+ see it so it sends SetMode
                 * (which we rewrite to ratchet if force_ratchet is set). */
                if (is_input && len >= 7) {
                    uint8_t fn_chk = (report[3] >> 4) & 0x0F;
                    if (fn_chk == 2 && report[4] == 0x00 && report[5] == 0xC4 && report[6])
                        global_state.switch_requested = true;
                }

                if (pt->hidpp_scan.pipe_debug_enabled && is_input
                    && report[2] == pt->hidpp_disc.fi_reprog_controls)
                    dh_debug_printf("[P1] dev=%d fi=0x%02X fn=%d sw=%d\n",
                                    report[1], report[2], (report[3]>>4)&0xF,
                                    report[3]&0xF);

                if (!is_input || CURRENT_BOARD_IS_ACTIVE_OUTPUT) {
                    bool ok = tud_hid_n_report(dev_inst, 0, report, len);
                    if (!ok)
                        dh_debug_printf("[PT] IN fwd FAILED\n");
                } else {
                    /* B active: raw dump if enabled */
                    if (pt->hidpp_scan.raw_dump_enabled) {
                        dh_debug_printf("[RAW] dev=%d fi=0x%02X fn=%d [",
                                        report[1], report[2], (report[3] >> 4) & 0x0F);
                        for (uint16_t i = 4; i < len && i < 20; i++)
                            dh_debug_printf("%02X ", report[i]);
                        dh_debug_printf("]\n");
                    }

                    /* HID++ → mouse conversion. Button events only update
                     * button_state (merged into every output_mouse_report).
                     * Send immediate report on button change for responsiveness. */
                    uint8_t prev_btn = pt->hidpp_disc.button_state;
                    mouse_report_t mouse = {0};
                    bool converted = passthrough_convert_hidpp_to_mouse(pt, report, len, &mouse);

                    if (pt->hidpp_disc.button_state != prev_btn) {
                        bool rel = global_state.relative_mouse || global_state.gaming_mode;
                        mouse_report_t btn_report = {0};
                        btn_report.mode = rel ? RELATIVE : ABSOLUTE;
                        if (!rel) {
                            btn_report.x = global_state.pointer_x;
                            btn_report.y = global_state.pointer_y;
                        }
                        if (pt->hidpp_scan.pipe_debug_enabled)
                            dh_debug_printf("[P3] btn 0x%02X→0x%02X x=%d y=%d\n",
                                            prev_btn, pt->hidpp_disc.button_state,
                                            btn_report.x, btn_report.y);
                        output_mouse_report(&btn_report, &global_state);
                    } else if (converted) {
                        /* Non-button event (scroll etc).
                         * Set mode to match gaming/relative state — without this,
                         * wheel reports go to ABSOLUTE interface which hosts like
                         * Android ignore when gaming_mode is active. */
                        mouse.mode = (global_state.relative_mouse
                                      || global_state.gaming_mode)
                                   ? RELATIVE : ABSOLUTE;
                        output_mouse_report(&mouse, &global_state);
                    }
                }
                handled = true;
            } else if (CURRENT_BOARD_IS_ACTIVE_OUTPUT) {
                tud_hid_n_report(dev_inst, 0, report, len);
                handled = true;
            }

            tuh_hid_receive_report(dev_addr, instance);
            if (handled)
                return;
            /* Not active output: fall through to DeskHop processing */
        }
    }

    hid_interface_t *iface = &global_state.iface[dev_addr-1][instance];

    /* Calculate a device index that distinguishes between different devices
       while staying within the bounds of MAX_DEVICES.

       Device index assignment:
       - 0: Primary keyboard (the one set in tuh_hid_mount_cb)
       - 1: Mouse devices
       - MAX_DEVICES-2: Secondary keyboards (e.g., wireless keyboard through unified dongle)
       - (dev_addr-1) % (MAX_DEVICES-1): Other devices

       Note: Slot MAX_DEVICES-1 is reserved for the remote device (used in handle_keyboard_uart_msg) */
    uint8_t device_idx;

    if (itf_protocol == HID_ITF_PROTOCOL_KEYBOARD) {
        if (dev_addr == global_state.kbd_dev_addr && instance == global_state.kbd_instance) {
            /* Primary keyboard */
            device_idx = 0;
        } else {
            /* Secondary keyboard (e.g., wireless keyboard through unified dongle) */
            device_idx = (MAX_DEVICES - 2);
        }
    } else if (itf_protocol == HID_ITF_PROTOCOL_MOUSE) {
        /* Mouse devices */
        device_idx = 1;
    } else {
        /* Other devices */
        device_idx = (dev_addr - 1) % (MAX_DEVICES - 1);
    }

    if (iface->uses_report_id || itf_protocol == HID_ITF_PROTOCOL_NONE) {
        uint8_t report_id = 0;

        if (iface->uses_report_id)
            report_id = report[0];

        if (report_id < MAX_REPORTS) {
            process_report_f receiver = iface->report_handler[report_id];

            if (receiver != NULL)
                receiver((uint8_t *)report, len, device_idx, iface);
        }
    }
    else if (itf_protocol == HID_ITF_PROTOCOL_KEYBOARD) {
        process_keyboard_report((uint8_t *)report, len, device_idx, iface);
    }
    else if (itf_protocol == HID_ITF_PROTOCOL_MOUSE) {
        process_mouse_report((uint8_t *)report, len, device_idx, iface);
    }

    /* Continue requesting reports */
    tuh_hid_receive_report(dev_addr, instance);
}

/* Set protocol in a callback. This is tied to an interface, not a specific report ID */
void tuh_hid_set_protocol_complete_cb(uint8_t dev_addr, uint8_t idx, uint8_t protocol) {
    if (dev_addr > MAX_DEVICES || idx > MAX_INTERFACES)
        return;

    hid_interface_t *iface = &global_state.iface[dev_addr-1][idx];
    iface->protocol = protocol;
}

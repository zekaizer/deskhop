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

#include "usb_descriptors.h"
#include "main.h"
#include "tusb.h"

//--------------------------------------------------------------------+
// Device Descriptors
//--------------------------------------------------------------------+

                                        // https://github.com/raspberrypi/usb-pid
tusb_desc_device_t const desc_device_config = DEVICE_DESCRIPTOR(0x2e8a, 0x107c);

                                        // https://pid.codes/1209/C000/
tusb_desc_device_t const desc_device = DEVICE_DESCRIPTOR(0x1209, 0xc000);

extern uint8_t const *rust_get_device_descriptor(void);
extern uint8_t const *rust_get_hid_report_descriptor(uint8_t instance);
extern uint8_t const *rust_get_configuration_descriptor(void);
extern uint16_t const *rust_get_string_descriptor(uint8_t index, uint16_t langid);

uint8_t const *tud_descriptor_device_cb(void) {
    return rust_get_device_descriptor();
}

//--------------------------------------------------------------------+
// HID Report Descriptor
//--------------------------------------------------------------------+

// Relative mouse is used to overcome limitations of multiple desktops on MacOS and Windows

uint8_t const desc_hid_report[] = {TUD_HID_REPORT_DESC_KEYBOARD(HID_REPORT_ID(REPORT_ID_KEYBOARD)),
                                   TUD_HID_REPORT_DESC_ABS_MOUSE(HID_REPORT_ID(REPORT_ID_MOUSE)),
                                   TUD_HID_REPORT_DESC_CONSUMER_CTRL(HID_REPORT_ID(REPORT_ID_CONSUMER)),
                                   TUD_HID_REPORT_DESC_SYSTEM_CONTROL(HID_REPORT_ID(REPORT_ID_SYSTEM))
                                   };

uint8_t const desc_hid_report_relmouse[] = {TUD_HID_REPORT_DESC_MOUSEHELP(HID_REPORT_ID(REPORT_ID_RELMOUSE))};

uint8_t const desc_hid_report_vendor[] = {TUD_HID_REPORT_DESC_VENDOR_CTRL(HID_REPORT_ID(REPORT_ID_VENDOR))};

/* Sizes exported for hal_shim.c passthrough config descriptor builder */
const uint16_t desc_hid_report_size = sizeof(desc_hid_report);
const uint16_t desc_hid_report_relmouse_size = sizeof(desc_hid_report_relmouse);


// Invoked when received GET HID REPORT DESCRIPTOR
// Application return pointer to descriptor
// Descriptor contents must exist long enough for transfer to complete
uint8_t const *tud_hid_descriptor_report_cb(uint8_t instance) {
    return rust_get_hid_report_descriptor(instance);
}

bool tud_mouse_report(uint8_t mode, uint8_t buttons, int16_t x, int16_t y, int8_t wheel, int8_t pan) {
    mouse_report_t report = {.buttons = buttons, .wheel = wheel, .x = x, .y = y, .mode = mode, .pan = pan};
    uint8_t instance = ITF_NUM_HID;
    uint8_t report_id = REPORT_ID_MOUSE;

    if (mode == RELATIVE) {
        instance = ITF_NUM_HID_REL_M;
        report_id = REPORT_ID_RELMOUSE;
    }

    return tud_hid_n_report(instance, report_id, &report, sizeof(report));
}


//--------------------------------------------------------------------+
// String Descriptors
//--------------------------------------------------------------------+

// String Descriptor Index — referenced by the configuration descriptors below.
// The descriptor bytes themselves are assembled in Rust
// (rust_get_string_descriptor).
enum {
    STRID_LANGID = 0,
    STRID_MANUFACTURER,
    STRID_PRODUCT,
    STRID_SERIAL,
    STRID_MOUSE,
    STRID_VENDOR,
    STRID_DISK,
    STRID_DEBUG,
};

// Invoked when received GET STRING DESCRIPTOR request
// Application return pointer to descriptor, whose contents must exist long enough for transfer to
// complete
uint16_t const *tud_descriptor_string_cb(uint8_t index, uint16_t langid) {
    return rust_get_string_descriptor(index, langid);
}

//--------------------------------------------------------------------+
// Configuration Descriptor
//--------------------------------------------------------------------+

#define EPNUM_HID        0x81
#define EPNUM_HID_REL_M  0x82
#define EPNUM_HID_VENDOR 0x83

#define EPNUM_MSC_OUT    0x04
#define EPNUM_MSC_IN     0x84

#ifndef DH_DEBUG

#define ITF_NUM_TOTAL 2
#define ITF_NUM_TOTAL_CONFIG 4
#define CONFIG_TOTAL_LEN (TUD_CONFIG_DESC_LEN + 2 * TUD_HID_DESC_LEN)
#define CONFIG_TOTAL_LEN_CFG (TUD_CONFIG_DESC_LEN + 3 * TUD_HID_DESC_LEN + TUD_MSC_DESC_LEN)

#else
#define ITF_NUM_CDC 4
#define ITF_NUM_TOTAL 3
#define ITF_NUM_TOTAL_CONFIG 5

#define CONFIG_TOTAL_LEN (TUD_CONFIG_DESC_LEN + 2 * TUD_HID_DESC_LEN + TUD_CDC_DESC_LEN)
#define CONFIG_TOTAL_LEN_CFG (TUD_CONFIG_DESC_LEN + 3 * TUD_HID_DESC_LEN + TUD_MSC_DESC_LEN + TUD_CDC_DESC_LEN)

#define EPNUM_CDC_NOTIF  0x85
#define EPNUM_CDC_OUT    0x06
#define EPNUM_CDC_IN     0x86

#endif


uint8_t const desc_configuration[] = {
    // Config number, interface count, string index, total length, attribute, power in mA
    TUD_CONFIG_DESCRIPTOR(1, ITF_NUM_TOTAL, 0, CONFIG_TOTAL_LEN, TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP, 500),

    // Interface number, string index, protocol, report descriptor len, EP In address, size & polling interval
    TUD_HID_DESCRIPTOR(ITF_NUM_HID,
                       STRID_PRODUCT,
                       HID_ITF_PROTOCOL_NONE,
                       sizeof(desc_hid_report),
                       EPNUM_HID,
                       CFG_TUD_HID_EP_BUFSIZE,
                       1),

    TUD_HID_DESCRIPTOR(ITF_NUM_HID_REL_M,
                       STRID_MOUSE,
                       HID_ITF_PROTOCOL_NONE,
                       sizeof(desc_hid_report_relmouse),
                       EPNUM_HID_REL_M,
                       CFG_TUD_HID_EP_BUFSIZE,
                       1),
#ifdef DH_DEBUG
    // Interface number, string index, EP notification address and size, EP data address (out, in) and size.
    TUD_CDC_DESCRIPTOR(
        ITF_NUM_CDC, STRID_DEBUG, EPNUM_CDC_NOTIF, 8, EPNUM_CDC_OUT, EPNUM_CDC_IN, CFG_TUD_CDC_EP_BUFSIZE),
#endif
};

uint8_t const desc_configuration_config[] = {
    // Config number, interface count, string index, total length, attribute, power in mA
    TUD_CONFIG_DESCRIPTOR(1, ITF_NUM_TOTAL_CONFIG, 0, CONFIG_TOTAL_LEN_CFG, TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP, 500),

    // Interface number, string index, protocol, report descriptor len, EP In address, size & polling interval
    TUD_HID_DESCRIPTOR(ITF_NUM_HID,
                       STRID_PRODUCT,
                       HID_ITF_PROTOCOL_NONE,
                       sizeof(desc_hid_report),
                       EPNUM_HID,
                       CFG_TUD_HID_EP_BUFSIZE,
                       1),

    TUD_HID_DESCRIPTOR(ITF_NUM_HID_REL_M,
                       STRID_MOUSE,
                       HID_ITF_PROTOCOL_NONE,
                       sizeof(desc_hid_report_relmouse),
                       EPNUM_HID_REL_M,
                       CFG_TUD_HID_EP_BUFSIZE,
                       1),

    TUD_HID_DESCRIPTOR(ITF_NUM_HID_VENDOR,
                       STRID_VENDOR,
                       HID_ITF_PROTOCOL_NONE,
                       sizeof(desc_hid_report_vendor),
                       EPNUM_HID_VENDOR,
                       CFG_TUD_HID_EP_BUFSIZE,
                       1),

    TUD_MSC_DESCRIPTOR(ITF_NUM_MSC,
                       STRID_DISK,
                       EPNUM_MSC_OUT,
                       EPNUM_MSC_IN,
                       64),
#ifdef DH_DEBUG
    // Interface number, string index, EP notification address and size, EP data address (out, in) and size.
    TUD_CDC_DESCRIPTOR(
        ITF_NUM_CDC, STRID_DEBUG, EPNUM_CDC_NOTIF, 8, EPNUM_CDC_OUT, EPNUM_CDC_IN, CFG_TUD_CDC_EP_BUFSIZE),
#endif
};

uint8_t const *tud_descriptor_configuration_cb(uint8_t index) {
    (void)index;
    return rust_get_configuration_descriptor();
}

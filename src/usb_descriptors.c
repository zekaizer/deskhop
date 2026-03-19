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

// Invoked when received GET DEVICE DESCRIPTOR
// Application return pointer to descriptor
static tusb_desc_device_t desc_device_passthrough;

uint8_t const *tud_descriptor_device_cb(void) {
    if (global_state.config_mode_active)
        return (uint8_t const *)&desc_device_config;

    passthrough_state_t *pt = passthrough_get_state();
    if (pt->active && pt->upstream_vid != 0) {
        desc_device_passthrough = (tusb_desc_device_t)DEVICE_DESCRIPTOR(
            pt->upstream_vid, pt->upstream_pid);
        return (uint8_t const *)&desc_device_passthrough;
    }

    return (uint8_t const *)&desc_device;
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


// Invoked when received GET HID REPORT DESCRIPTOR
// Application return pointer to descriptor
// Descriptor contents must exist long enough for transfer to complete
uint8_t const *tud_hid_descriptor_report_cb(uint8_t instance) {
    if (global_state.config_mode_active)
        if (instance == ITF_NUM_HID_VENDOR)
            return desc_hid_report_vendor;

    /* Passthrough instances: return captured descriptor from host device */
    passthrough_state_t *pt = passthrough_get_state();
    if (pt->active && instance >= ITF_NUM_PT_BASE) {
        uint16_t len;
        const uint8_t *desc = passthrough_get_report_desc(pt, instance, &len);
        if (desc)
            return desc;
    }

    switch(instance) {
        case ITF_NUM_HID:
            return desc_hid_report;
        case ITF_NUM_HID_REL_M:
            return desc_hid_report_relmouse;
        default:
            return desc_hid_report;
    }
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

// array of pointer to string descriptors
char const *string_desc_arr[] = {
    (const char[]){0x09, 0x04}, // 0: is supported language is English (0x0409)
    "Hrvoje Cavrak",            // 1: Manufacturer
    "DeskHop Switch",           // 2: Product
    "0",                        // 3: Serials, should use chip ID
    "DeskHop Helper",           // 4: Mouse Helper Interface
    "DeskHop Config",           // 5: Vendor Interface
    "DeskHop Disk",             // 6: Disk Interface
#ifdef DH_DEBUG
    "DeskHop Debug",            // 7: Debug Interface
#endif
};

// String Descriptor Index
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

static uint16_t _desc_str[32];

// Invoked when received GET STRING DESCRIPTOR request
// Application return pointer to descriptor, whose contents must exist long enough for transfer to
// complete
uint16_t const *tud_descriptor_string_cb(uint8_t index, uint16_t langid) {
    (void)langid;

    uint8_t chr_count;

    // 2 (hex) characters for every byte + 1 '\0' for string end
    static char serial_number[PICO_UNIQUE_BOARD_ID_SIZE_BYTES * 2 + 1] = {0};

    if (!serial_number[0]) {
       pico_get_unique_board_id_string(serial_number, sizeof(serial_number));
    }

    if (index == 0) {
        memcpy(&_desc_str[1], string_desc_arr[0], 2);
        chr_count = 1;
    } else {
        const char *str = NULL;

        /* Override manufacturer/product strings for Logitech passthrough (FR-PT-009) */
        passthrough_state_t *pt = passthrough_get_state();
        if (!global_state.config_mode_active && pt->active && pt->upstream_vid == 0x046D) {
            if (index == STRID_MANUFACTURER) str = "Logitech";
            else if (index == STRID_PRODUCT)  str = "USB Receiver";
        }

        if (!str) {
            if (!(index < sizeof(string_desc_arr) / sizeof(string_desc_arr[0])))
                return NULL;
            str = (index == STRID_SERIAL) ? serial_number : string_desc_arr[index];
        }

        // Cap at max char
        chr_count = strlen(str);
        if (chr_count > 31)
            chr_count = 31;

        // Convert ASCII string into UTF-16
        for (uint8_t i = 0; i < chr_count; i++) {
            _desc_str[1 + i] = str[i];
        }
    }

    // first byte is length (including header), second byte is string type
    _desc_str[0] = (TUSB_DESC_STRING << 8) | (2 * chr_count + 2);

    return _desc_str;
}

//--------------------------------------------------------------------+
// Passthrough Configuration Descriptor Builder (P2)
//--------------------------------------------------------------------+

/* Append one HID interface descriptor block (25 bytes) to buffer */
static uint16_t append_hid_itf(uint8_t *buf, uint8_t itf_num, uint8_t str_idx,
                                uint8_t protocol, uint16_t report_desc_len,
                                uint8_t ep_addr, uint16_t ep_size, uint8_t ep_interval) {
    /* Interface descriptor (9 bytes) */
    buf[0] = 9;
    buf[1] = TUSB_DESC_INTERFACE;
    buf[2] = itf_num;
    buf[3] = 0;                                            /* bAlternateSetting */
    buf[4] = 1;                                            /* bNumEndpoints */
    buf[5] = TUSB_CLASS_HID;
    buf[6] = (protocol != 0) ? HID_SUBCLASS_BOOT : 0;     /* bInterfaceSubClass */
    buf[7] = protocol;                                     /* bInterfaceProtocol */
    buf[8] = str_idx;

    /* HID class descriptor (9 bytes) */
    buf[9]  = 9;
    buf[10] = HID_DESC_TYPE_HID;
    buf[11] = 0x11;                                        /* bcdHID 1.11 lo */
    buf[12] = 0x01;                                        /* bcdHID 1.11 hi */
    buf[13] = 0;                                           /* bCountryCode */
    buf[14] = 1;                                           /* bNumDescriptors */
    buf[15] = HID_DESC_TYPE_REPORT;
    buf[16] = (uint8_t)(report_desc_len);                  /* wDescriptorLength lo */
    buf[17] = (uint8_t)(report_desc_len >> 8);             /* wDescriptorLength hi */

    /* Endpoint descriptor (7 bytes) */
    buf[18] = 7;
    buf[19] = TUSB_DESC_ENDPOINT;
    buf[20] = ep_addr;
    buf[21] = TUSB_XFER_INTERRUPT;
    buf[22] = (uint8_t)(ep_size);                          /* wMaxPacketSize lo */
    buf[23] = (uint8_t)(ep_size >> 8);                     /* wMaxPacketSize hi */
    buf[24] = ep_interval;

    return 25; /* TUD_HID_DESC_LEN */
}

/* Build full configuration descriptor: DeskHop interfaces + passthrough interfaces.
 * Stores result in state->config_desc / config_desc_len. */
void passthrough_build_config_desc(passthrough_state_t *state) {
    uint8_t *buf = state->config_desc;
    uint16_t off = 0;
    uint8_t num_pt = state->iface_count;
    uint8_t num_itf = 2 + num_pt;

#ifdef DH_DEBUG
    num_itf += 2; /* CDC Communication + Data interfaces */
#endif

    /* Configuration descriptor header (9 bytes) */
    buf[off++] = 9;
    buf[off++] = TUSB_DESC_CONFIGURATION;
    off += 2; /* wTotalLength — filled at end */
    buf[off++] = num_itf;
    buf[off++] = 1;    /* bConfigurationValue */
    buf[off++] = 0;    /* iConfiguration */
    buf[off++] = 0x80 | TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP;
    buf[off++] = 250;  /* bMaxPower: 500mA / 2 */

    /* DeskHop ITF 0: main HID (keyboard + abs mouse + consumer + system) */
    off += append_hid_itf(buf + off, ITF_NUM_HID, STRID_PRODUCT,
                          HID_ITF_PROTOCOL_NONE, sizeof(desc_hid_report),
                          0x81, CFG_TUD_HID_EP_BUFSIZE, 1);

    /* DeskHop ITF 1: relative mouse helper */
    off += append_hid_itf(buf + off, ITF_NUM_HID_REL_M, STRID_MOUSE,
                          HID_ITF_PROTOCOL_NONE, sizeof(desc_hid_report_relmouse),
                          0x82, CFG_TUD_HID_EP_BUFSIZE, 1);

    /* Passthrough interfaces (captured from host device) */
    for (uint8_t i = 0; i < num_pt; i++) {
        off += append_hid_itf(buf + off, ITF_NUM_PT_BASE + i, 0,
                              state->ifaces[i].itf_protocol,
                              state->ifaces[i].desc_len,
                              EPNUM_PT_BASE + i,
                              CFG_TUD_HID_EP_BUFSIZE, 1);
    }

#ifdef DH_DEBUG
    {
        /* CDC descriptor (66 bytes): dynamically assigned after passthrough EPs */
        uint8_t cdc_itf   = ITF_NUM_PT_BASE + num_pt;
        uint8_t ep_notif   = 0x80 | (3 + num_pt);     /* IN */
        uint8_t ep_out     = (uint8_t)(3 + num_pt + 1);
        uint8_t ep_in      = 0x80 | (3 + num_pt + 1); /* IN */

        /* Interface Association (8) */
        buf[off++] = 8;  buf[off++] = TUSB_DESC_INTERFACE_ASSOCIATION;
        buf[off++] = cdc_itf; buf[off++] = 2;
        buf[off++] = TUSB_CLASS_CDC;
        buf[off++] = CDC_COMM_SUBCLASS_ABSTRACT_CONTROL_MODEL;
        buf[off++] = CDC_COMM_PROTOCOL_NONE; buf[off++] = 0;

        /* CDC Control Interface (9) */
        buf[off++] = 9;  buf[off++] = TUSB_DESC_INTERFACE;
        buf[off++] = cdc_itf; buf[off++] = 0; buf[off++] = 1;
        buf[off++] = TUSB_CLASS_CDC;
        buf[off++] = CDC_COMM_SUBCLASS_ABSTRACT_CONTROL_MODEL;
        buf[off++] = CDC_COMM_PROTOCOL_NONE; buf[off++] = STRID_DEBUG;

        /* Header Functional Descriptor (5) */
        buf[off++] = 5;  buf[off++] = 0x24; /* CS_INTERFACE */
        buf[off++] = 0x00; /* Header */
        buf[off++] = 0x20; buf[off++] = 0x01; /* bcdCDC 1.20 */

        /* Call Management (5) */
        buf[off++] = 5;  buf[off++] = 0x24;
        buf[off++] = 0x01; /* Call Management */
        buf[off++] = 0;  buf[off++] = (uint8_t)(cdc_itf + 1);

        /* ACM (4) */
        buf[off++] = 4;  buf[off++] = 0x24;
        buf[off++] = 0x02; /* ACM */ buf[off++] = 0x02;

        /* Union (5) */
        buf[off++] = 5;  buf[off++] = 0x24;
        buf[off++] = 0x06; /* Union */
        buf[off++] = cdc_itf; buf[off++] = (uint8_t)(cdc_itf + 1);

        /* Notification EP (7) */
        buf[off++] = 7;  buf[off++] = TUSB_DESC_ENDPOINT;
        buf[off++] = ep_notif; buf[off++] = TUSB_XFER_INTERRUPT;
        buf[off++] = 8;  buf[off++] = 0;  /* wMaxPacketSize */
        buf[off++] = 16; /* bInterval */

        /* CDC Data Interface (9) */
        buf[off++] = 9;  buf[off++] = TUSB_DESC_INTERFACE;
        buf[off++] = (uint8_t)(cdc_itf + 1);
        buf[off++] = 0; buf[off++] = 2;
        buf[off++] = TUSB_CLASS_CDC_DATA;
        buf[off++] = 0; buf[off++] = 0; buf[off++] = 0;

        /* Data EP OUT (7) */
        buf[off++] = 7;  buf[off++] = TUSB_DESC_ENDPOINT;
        buf[off++] = ep_out; buf[off++] = TUSB_XFER_BULK;
        buf[off++] = 64; buf[off++] = 0;  /* wMaxPacketSize */
        buf[off++] = 0;

        /* Data EP IN (7) */
        buf[off++] = 7;  buf[off++] = TUSB_DESC_ENDPOINT;
        buf[off++] = ep_in; buf[off++] = TUSB_XFER_BULK;
        buf[off++] = 64; buf[off++] = 0;
        buf[off++] = 0;
    }
#endif

    /* Fill wTotalLength (bytes 2-3 of config header) */
    buf[2] = (uint8_t)(off);
    buf[3] = (uint8_t)(off >> 8);

    state->config_desc_len = off;
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
    (void)index; // for multiple configurations

    if (global_state.config_mode_active)
        return desc_configuration_config;

    /* When passthrough is active, return dynamically built descriptor */
    passthrough_state_t *pt = passthrough_get_state();
    if (pt->active && pt->config_desc_len > 0)
        return pt->config_desc;

    return desc_configuration;
}

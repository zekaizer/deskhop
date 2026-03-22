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
#include "hid_report.h"
#include "main.h"

/* Now implemented in Rust (src-rust/src/app/hid_report.rs) */
extern int32_t rust_get_report_value(const uint8_t *report, int len, const uint8_t *val);

int32_t get_report_value(uint8_t *report, int len, report_val_t *val) {
    return rust_get_report_value(report, len, (const uint8_t *)val);
}

/* After processing the descriptor, assign the values so we can later use them to interpret reports */
void handle_consumer_control_values(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    keyboard_t *keyboard = get_keyboard(iface, src->report_id);

    if (src->offset > MAX_CC_BUTTONS) {
        return;
    }

    if (src->data_type == VARIABLE) {
        keyboard->cc_array[src->offset] = src->usage;
        iface->consumer.is_variable = true;
    }

    iface->consumer.is_array |= (src->data_type == ARRAY);
}

/* After processing the descriptor, assign the values so we can later use them to interpret reports */
void handle_system_control_values(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    keyboard_t *keyboard = get_keyboard(iface, src->report_id);

    if (src->offset > MAX_SYS_BUTTONS) {
        return;
    }

    if (src->data_type == VARIABLE) {
        keyboard->sys_array[src->offset] = src->usage;
        iface->system.is_variable = true;
    }

    iface->system.is_array |= (src->data_type == ARRAY);
}

/* After processing the descriptor, assign the values so we can later use them to interpret reports */
void handle_keyboard_descriptor_values(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    const int LEFT_CTRL = 0xE0;
    keyboard_t *keyboard = get_keyboard(iface, src->report_id);

    /* Constants are normally used for padding, so skip'em */
    if (src->item_type == CONSTANT)
        return;

    /* Prevent overwriting more memory than we have */
    if (iface->num_keyboards >= MAX_KEYBOARDS)
        return;

    /* Detect and handle modifier keys. <= if modifier is less + constant padding? */
    if (src->size <= MODIFIER_BIT_LENGTH && src->data_type == VARIABLE) {
        /* To make sure this really is the modifier key, we expect e.g. left control to be
           within the usage interval */
        if (LEFT_CTRL >= src->usage_min && LEFT_CTRL <= src->usage_max)
            keyboard->modifier = *src;
    }

    /* If we have an array member, that's most likely a key (0x00 - 0xFF, 1 byte) */
    if (src->offset_idx < MAX_KEYS) {
        keyboard->key_array[src->offset_idx] = (src->data_type == ARRAY);
    }

    /* Handle NKRO, normally size = 1, count = 240 or so, but they are swapped. */
    if (src->size > 32 && src->data_type == VARIABLE) {
        keyboard->is_nkro = true;
        keyboard->nkro    = *src;
    }

    /* We found a keyboard on this interface for a specific report id. */
    if (!keyboard->is_found) {
        keyboard->is_found = true;
        iface->num_keyboards++;
    }
}

void handle_buttons(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    /* Constant is normally used for padding with mouse buttons, aggregate to simplify things */
    if (src->item_type == CONSTANT) {
        iface->mouse.buttons.size += src->size;
        return;
    }

    iface->mouse.buttons = *src;

    /* We found a mouse on this interface. */
    iface->mouse.is_found = true;
}

void _store(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    if (src->item_type != CONSTANT)
        *dst = *src;
}

static uint8_t *get_mouse_id(hid_interface_t *iface) {
    return &iface->mouse.report_id;
}

static uint8_t *get_consumer_id(hid_interface_t *iface) {
    return &iface->consumer.report_id;
}

static uint8_t *get_system_id(hid_interface_t *iface) {
    return &iface->system.report_id;
}

static uint8_t *get_next_keyboard_id(hid_interface_t *iface) {
    if (iface->num_keyboards < MAX_KEYBOARDS)
        return &iface->keyboards[iface->num_keyboards].report_id;

    /* In case we are out of bounds, return the last keyboard's ID */
    return &iface->keyboards[MAX_KEYBOARDS - 1].report_id;
}


void extract_data(hid_interface_t *iface, report_val_t *val) {
    const usage_map_t map[] = {
        {.usage_page   = HID_USAGE_PAGE_BUTTON,
         .global_usage = HID_USAGE_DESKTOP_MOUSE,
         .handler      = handle_buttons,
         .receiver     = process_mouse_report,
         .dst          = &iface->mouse.buttons,
         .get_id       = get_mouse_id},

        {.usage_page   = HID_USAGE_PAGE_DESKTOP,
         .global_usage = HID_USAGE_DESKTOP_MOUSE,
         .usage        = HID_USAGE_DESKTOP_X,
         .handler      = _store,
         .receiver     = process_mouse_report,
         .dst          = &iface->mouse.move_x,
         .get_id       = get_mouse_id},

        {.usage_page   = HID_USAGE_PAGE_DESKTOP,
         .global_usage = HID_USAGE_DESKTOP_MOUSE,
         .usage        = HID_USAGE_DESKTOP_Y,
         .handler      = _store,
         .receiver     = process_mouse_report,
         .dst          = &iface->mouse.move_y,
         .get_id       = get_mouse_id},

        {.usage_page   = HID_USAGE_PAGE_DESKTOP,
         .global_usage = HID_USAGE_DESKTOP_MOUSE,
         .usage        = HID_USAGE_DESKTOP_WHEEL,
         .handler      = _store,
         .receiver     = process_mouse_report,
         .dst          = &iface->mouse.wheel,
         .get_id       = get_mouse_id},

        {.usage_page   = HID_USAGE_PAGE_CONSUMER,
         .global_usage = HID_USAGE_DESKTOP_MOUSE,
         .usage        = HID_USAGE_CONSUMER_AC_PAN,
         .handler      = _store,
         .receiver     = process_mouse_report,
         .dst          = &iface->mouse.pan,
         .get_id       = get_mouse_id},

        {.usage_page   = HID_USAGE_PAGE_KEYBOARD,
         .global_usage = HID_USAGE_DESKTOP_KEYBOARD,
         .handler      = handle_keyboard_descriptor_values,
         .receiver     = process_keyboard_report,
         .get_id       = get_next_keyboard_id},

        {.usage_page   = HID_USAGE_PAGE_CONSUMER,
         .global_usage = HID_USAGE_CONSUMER_CONTROL,
         .handler      = handle_consumer_control_values,
         .receiver     = process_consumer_report,
         .dst          = &iface->consumer.val,
         .get_id       = get_consumer_id},

        {.usage_page   = HID_USAGE_PAGE_DESKTOP,
         .global_usage = HID_USAGE_DESKTOP_SYSTEM_CONTROL,
         .handler      = _store,
         .receiver     = process_system_report,
         .dst          = &iface->system.val,
         .get_id       = get_system_id},
    };

    /* We extracted all we could find in the descriptor to report_values, now go through them and
       match them up with the values in the table above, then store those values for later reference */

    for (const usage_map_t *hay = map; hay != &map[ARRAY_SIZE(map)]; hay++) {
        /* ---> If any condition is not defined, we consider it as matched <--- */
        bool global_usages_match = (val->global_usage == hay->global_usage) || (hay->global_usage == 0);
        bool usages_match        = (val->usage == hay->usage) || (hay->usage == 0);
        bool usage_pages_match   = (val->usage_page == hay->usage_page) || (hay->usage_page == 0);

        if (global_usages_match && usages_match && usage_pages_match) {
            *(hay->get_id(iface)) = val->report_id;

            hay->handler(val, hay->dst, iface);

            if (val->report_id < MAX_REPORTS)
                iface->report_handler[val->report_id] = hay->receiver;
        }
    }
}

extern int32_t rust_extract_bit_variable(const uint8_t *kbd, const uint8_t *raw_report, int len, uint8_t *dst);

int32_t extract_bit_variable(report_val_t *kbd, uint8_t *raw_report, int len, uint8_t *dst) {
    return rust_extract_bit_variable((const uint8_t *)kbd, raw_report, len, dst);
}

extern int32_t rust_extract_kbd_boot(const uint8_t *raw_report, int len, uint8_t *out);

int32_t _extract_kbd_boot(uint8_t *raw_report, int len, hid_keyboard_report_t *report) {
    return rust_extract_kbd_boot(raw_report, len, (uint8_t *)report);
}

int32_t _extract_kbd_other(uint8_t *raw_report, int len, hid_interface_t *iface, hid_keyboard_report_t *report) {
    keyboard_t *kb = get_keyboard(iface, raw_report[0]);
    uint8_t *src = raw_report;

    if (iface->uses_report_id)
        src++;

    report->modifier = src[kb->modifier.offset_idx];
    for (int i=0, j=0; i < MAX_KEYS && j < KEYS_IN_USB_REPORT; i++) {
        if(kb->key_array[i])
            report->keycode[j++] = src[i];
    }

    return KBD_REPORT_LENGTH;
}

int32_t _extract_kbd_nkro(uint8_t *raw_report, int len, hid_interface_t *iface, hid_keyboard_report_t *report) {
    keyboard_t *kb = get_keyboard(iface, raw_report[0]);
    uint8_t *ptr = raw_report;

    /* Skip report ID */
    if (iface->uses_report_id)
        ptr++;

    /* We expect array of bits mapping 1:1 from usage_min to usage_max, otherwise panic */
    if ((kb->nkro.usage_max - kb->nkro.usage_min + 1) != kb->nkro.size)
        return -1;

    /* We expect modifier to be 8 bits long, otherwise we'll fallback to boot mode */
    if (kb->modifier.size == MODIFIER_BIT_LENGTH) {
        report->modifier = ptr[kb->modifier.offset_idx];
    } else
        return -1;

    /* Move the pointer to the nkro offset's byte index */
    ptr = &ptr[kb->nkro.offset_idx];

    return extract_bit_variable(&kb->nkro, ptr, KEYS_IN_USB_REPORT, report->keycode);
}

int32_t extract_kbd_data(
    uint8_t *raw_report, int len, uint8_t itf, hid_interface_t *iface, hid_keyboard_report_t *report) {
    keyboard_t *keyboard = get_keyboard(iface, raw_report[0]);

    /* Clear the report to start fresh */
    memset(report, 0, KBD_REPORT_LENGTH);

    /* If we're in boot protocol mode, then it's easy to decide. */
    if (iface->protocol == HID_PROTOCOL_BOOT)
        return _extract_kbd_boot(raw_report, len, report);

    /* NKRO is a special case */
    if (keyboard->is_nkro)
        return _extract_kbd_nkro(raw_report, len, iface, report);

    /* If we're getting 8 bytes of report, it's safe to assume standard modifier + reserved + keys */
    if (!iface->uses_report_id && (len == KBD_REPORT_LENGTH || len == KBD_REPORT_LENGTH + 1))
        return _extract_kbd_boot(raw_report, len, report);

    /* This is something completely different, look at the report  */
    return _extract_kbd_other(raw_report, len, iface, report);
}

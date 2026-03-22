/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * HID report extraction ported to Rust. This file contains:
 * - get_report_value wrapper (Rust FFI)
 * - extract_data + descriptor handler functions (hid_interface_t fn ptrs)
 * - extract_kbd_data wrapper (Rust FFI)
 */
#include "hid_report.h"
#include "main.h"

/* Rust wrapper */
extern int32_t rust_get_report_value(const uint8_t *, int, const uint8_t *);
int32_t get_report_value(uint8_t *report, int len, report_val_t *val) {
    return rust_get_report_value(report, len, (const uint8_t *)val);
}

/* ---- Descriptor handler functions (populate hid_interface_t) ---- */

void handle_consumer_control_values(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    keyboard_t *kb = get_keyboard(iface, src->report_id);
    if (src->offset > MAX_CC_BUTTONS) return;
    if (src->data_type == VARIABLE) { kb->cc_array[src->offset] = src->usage; iface->consumer.is_variable = true; }
    iface->consumer.is_array |= (src->data_type == ARRAY);
}

void handle_system_control_values(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    keyboard_t *kb = get_keyboard(iface, src->report_id);
    if (src->offset > MAX_SYS_BUTTONS) return;
    if (src->data_type == VARIABLE) { kb->sys_array[src->offset] = src->usage; iface->system.is_variable = true; }
    iface->system.is_array |= (src->data_type == ARRAY);
}

void handle_keyboard_descriptor_values(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    const int LEFT_CTRL = 0xE0;
    keyboard_t *kb = get_keyboard(iface, src->report_id);
    if (src->item_type == CONSTANT) return;
    if (iface->num_keyboards >= MAX_KEYBOARDS) return;
    if (src->size <= MODIFIER_BIT_LENGTH && src->data_type == VARIABLE)
        if (LEFT_CTRL >= src->usage_min && LEFT_CTRL <= src->usage_max) kb->modifier = *src;
    if (src->offset_idx < MAX_KEYS) kb->key_array[src->offset_idx] = (src->data_type == ARRAY);
    if (src->size > 32 && src->data_type == VARIABLE) { kb->is_nkro = true; kb->nkro = *src; }
    if (!kb->is_found) { kb->is_found = true; iface->num_keyboards++; }
}

void handle_buttons(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    if (src->item_type == CONSTANT) { iface->mouse.buttons.size += src->size; return; }
    iface->mouse.buttons = *src;
    iface->mouse.is_found = true;
}

void _store(report_val_t *src, report_val_t *dst, hid_interface_t *iface) {
    if (src->item_type != CONSTANT) *dst = *src;
}

static uint8_t *get_mouse_id(hid_interface_t *i) { return &i->mouse.report_id; }
static uint8_t *get_consumer_id(hid_interface_t *i) { return &i->consumer.report_id; }
static uint8_t *get_system_id(hid_interface_t *i) { return &i->system.report_id; }
static uint8_t *get_next_keyboard_id(hid_interface_t *i) {
    return (i->num_keyboards < MAX_KEYBOARDS)
        ? &i->keyboards[i->num_keyboards].report_id
        : &i->keyboards[MAX_KEYBOARDS - 1].report_id;
}

/* ---- extract_data: usage map matching + hid_interface_t population ---- */

void extract_data(hid_interface_t *iface, report_val_t *val) {
    const usage_map_t map[] = {
        {HID_USAGE_PAGE_BUTTON,   HID_USAGE_DESKTOP_MOUSE, 0, handle_buttons, process_mouse_report, &iface->mouse.buttons, get_mouse_id},
        {HID_USAGE_PAGE_DESKTOP,  HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_X, _store, process_mouse_report, &iface->mouse.move_x, get_mouse_id},
        {HID_USAGE_PAGE_DESKTOP,  HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_Y, _store, process_mouse_report, &iface->mouse.move_y, get_mouse_id},
        {HID_USAGE_PAGE_DESKTOP,  HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_WHEEL, _store, process_mouse_report, &iface->mouse.wheel, get_mouse_id},
        {HID_USAGE_PAGE_CONSUMER, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_CONSUMER_AC_PAN, _store, process_mouse_report, &iface->mouse.pan, get_mouse_id},
        {HID_USAGE_PAGE_KEYBOARD, HID_USAGE_DESKTOP_KEYBOARD, 0, handle_keyboard_descriptor_values, process_keyboard_report, NULL, get_next_keyboard_id},
        {HID_USAGE_PAGE_CONSUMER, HID_USAGE_CONSUMER_CONTROL, 0, handle_consumer_control_values, process_consumer_report, &iface->consumer.val, get_consumer_id},
        {HID_USAGE_PAGE_DESKTOP,  HID_USAGE_DESKTOP_SYSTEM_CONTROL, 0, _store, process_system_report, &iface->system.val, get_system_id},
    };

    for (const usage_map_t *h = map; h != &map[ARRAY_SIZE(map)]; h++) {
        bool gu = (val->global_usage == h->global_usage) || !h->global_usage;
        bool u  = (val->usage == h->usage) || !h->usage;
        bool up = (val->usage_page == h->usage_page) || !h->usage_page;
        if (gu && u && up) {
            *(h->get_id(iface)) = val->report_id;
            h->handler(val, h->dst, iface);
            if (val->report_id < MAX_REPORTS) iface->report_handler[val->report_id] = h->receiver;
        }
    }
}

/* Rust wrapper */
extern int32_t rust_extract_kbd_data(uint8_t *, int, uint8_t, void *, uint8_t *);
int32_t extract_kbd_data(uint8_t *r, int l, uint8_t i, hid_interface_t *f, hid_keyboard_report_t *o) {
    return rust_extract_kbd_data(r, l, i, (void *)f, (uint8_t *)o);
}

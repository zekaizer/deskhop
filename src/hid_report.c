/* DeskHop HID report — extract_data (fn ptrs, HAL) + wrappers. */
#include "hid_report.h"
#include "main.h"

extern int32_t rust_get_report_value(const uint8_t *, int, const uint8_t *);
extern int32_t rust_extract_kbd_data(uint8_t *, int, uint8_t, void *, uint8_t *);

int32_t get_report_value(uint8_t *r, int l, report_val_t *v) { return rust_get_report_value(r, l, (const uint8_t *)v); }
int32_t extract_kbd_data(uint8_t *r, int l, uint8_t i, hid_interface_t *f, hid_keyboard_report_t *o) {
    return rust_extract_kbd_data(r, l, i, (void *)f, (uint8_t *)o);
}

/* Descriptor handlers — populate hid_interface_t (C fn ptrs required) */
void handle_consumer_control_values(report_val_t *s, report_val_t *d, hid_interface_t *i) {
    keyboard_t *k = get_keyboard(i, s->report_id);
    if (s->offset > MAX_CC_BUTTONS) return;
    if (s->data_type == VARIABLE) { k->cc_array[s->offset] = s->usage; i->consumer.is_variable = true; }
    i->consumer.is_array |= (s->data_type == ARRAY);
}
void handle_system_control_values(report_val_t *s, report_val_t *d, hid_interface_t *i) {
    keyboard_t *k = get_keyboard(i, s->report_id);
    if (s->offset > MAX_SYS_BUTTONS) return;
    if (s->data_type == VARIABLE) { k->sys_array[s->offset] = s->usage; i->system.is_variable = true; }
    i->system.is_array |= (s->data_type == ARRAY);
}
void handle_keyboard_descriptor_values(report_val_t *s, report_val_t *d, hid_interface_t *i) {
    keyboard_t *k = get_keyboard(i, s->report_id);
    if (s->item_type == CONSTANT || i->num_keyboards >= MAX_KEYBOARDS) return;
    if (s->size <= MODIFIER_BIT_LENGTH && s->data_type == VARIABLE && 0xE0 >= s->usage_min && 0xE0 <= s->usage_max) k->modifier = *s;
    if (s->offset_idx < MAX_KEYS) k->key_array[s->offset_idx] = (s->data_type == ARRAY);
    if (s->size > 32 && s->data_type == VARIABLE) { k->is_nkro = true; k->nkro = *s; }
    if (!k->is_found) { k->is_found = true; i->num_keyboards++; }
}
void handle_buttons(report_val_t *s, report_val_t *d, hid_interface_t *i) {
    if (s->item_type == CONSTANT) { i->mouse.buttons.size += s->size; return; }
    i->mouse.buttons = *s; i->mouse.is_found = true;
}
void _store(report_val_t *s, report_val_t *d, hid_interface_t *i) { if (s->item_type != CONSTANT) *d = *s; }

static uint8_t *get_mouse_id(hid_interface_t *i) { return &i->mouse.report_id; }
static uint8_t *get_consumer_id(hid_interface_t *i) { return &i->consumer.report_id; }
static uint8_t *get_system_id(hid_interface_t *i) { return &i->system.report_id; }
static uint8_t *get_next_keyboard_id(hid_interface_t *i) {
    return (i->num_keyboards < MAX_KEYBOARDS) ? &i->keyboards[i->num_keyboards].report_id : &i->keyboards[MAX_KEYBOARDS-1].report_id;
}

void extract_data(hid_interface_t *iface, report_val_t *val) {
    const usage_map_t m[] = {
        {HID_USAGE_PAGE_BUTTON,HID_USAGE_DESKTOP_MOUSE,0,handle_buttons,process_mouse_report,&iface->mouse.buttons,get_mouse_id},
        {HID_USAGE_PAGE_DESKTOP,HID_USAGE_DESKTOP_MOUSE,HID_USAGE_DESKTOP_X,_store,process_mouse_report,&iface->mouse.move_x,get_mouse_id},
        {HID_USAGE_PAGE_DESKTOP,HID_USAGE_DESKTOP_MOUSE,HID_USAGE_DESKTOP_Y,_store,process_mouse_report,&iface->mouse.move_y,get_mouse_id},
        {HID_USAGE_PAGE_DESKTOP,HID_USAGE_DESKTOP_MOUSE,HID_USAGE_DESKTOP_WHEEL,_store,process_mouse_report,&iface->mouse.wheel,get_mouse_id},
        {HID_USAGE_PAGE_CONSUMER,HID_USAGE_DESKTOP_MOUSE,HID_USAGE_CONSUMER_AC_PAN,_store,process_mouse_report,&iface->mouse.pan,get_mouse_id},
        {HID_USAGE_PAGE_KEYBOARD,HID_USAGE_DESKTOP_KEYBOARD,0,handle_keyboard_descriptor_values,process_keyboard_report,NULL,get_next_keyboard_id},
        {HID_USAGE_PAGE_CONSUMER,HID_USAGE_CONSUMER_CONTROL,0,handle_consumer_control_values,process_consumer_report,&iface->consumer.val,get_consumer_id},
        {HID_USAGE_PAGE_DESKTOP,HID_USAGE_DESKTOP_SYSTEM_CONTROL,0,_store,process_system_report,&iface->system.val,get_system_id},
    };
    for (const usage_map_t *h = m; h != &m[ARRAY_SIZE(m)]; h++) {
        if (((val->global_usage == h->global_usage) || !h->global_usage) &&
            ((val->usage == h->usage) || !h->usage) &&
            ((val->usage_page == h->usage_page) || !h->usage_page)) {
            *(h->get_id(iface)) = val->report_id;
            h->handler(val, h->dst, iface);
            if (val->report_id < MAX_REPORTS) iface->report_handler[val->report_id] = h->receiver;
        }
    }
}

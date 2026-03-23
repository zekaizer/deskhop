/* DeskHop HID report — most logic in Rust. C wrappers + kbd descriptor handler. */
#include "hid_report.h"
#include "main.h"

extern int32_t rust_get_report_value(const uint8_t *, int, const uint8_t *);
extern int32_t rust_extract_kbd_data(uint8_t *, int, uint8_t, void *, uint8_t *);
extern void rust_extract_data(void *, const uint8_t *);

int32_t get_report_value(uint8_t *r, int l, report_val_t *v) { return rust_get_report_value(r, l, (const uint8_t *)v); }
int32_t extract_kbd_data(uint8_t *r, int l, uint8_t i, hid_interface_t *f, hid_keyboard_report_t *o) {
    return rust_extract_kbd_data(r, l, i, (void *)f, (uint8_t *)o);
}
void extract_data(hid_interface_t *iface, report_val_t *val) {
    rust_extract_data((void *)iface, (const uint8_t *)val);
}

/* These must stay in C — they directly manipulate keyboard_t fields */
void handle_keyboard_descriptor_values(report_val_t *s, report_val_t *d, hid_interface_t *i) {
    keyboard_t *k = get_keyboard(i, s->report_id);
    if (s->item_type == CONSTANT || i->num_keyboards >= MAX_KEYBOARDS) return;
    if (s->size <= MODIFIER_BIT_LENGTH && s->data_type == VARIABLE && 0xE0 >= s->usage_min && 0xE0 <= s->usage_max) k->modifier = *s;
    if (s->offset_idx < MAX_KEYS) k->key_array[s->offset_idx] = (s->data_type == ARRAY);
    if (s->size > 32 && s->data_type == VARIABLE) { k->is_nkro = true; k->nkro = *s; }
    if (!k->is_found) { k->is_found = true; i->num_keyboards++; }
}
void handle_consumer_control_values(report_val_t *s, report_val_t *d, hid_interface_t *i) {
    keyboard_t *k = get_keyboard(i, s->report_id);
    if (s->offset > MAX_CC_BUTTONS) return;
    if (s->data_type == VARIABLE) { k->cc_array[s->offset] = s->usage; i->consumer.is_variable = true; }
    i->consumer.is_array |= (s->data_type == ARRAY);
}

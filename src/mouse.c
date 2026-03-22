/* DeskHop mouse — logic in Rust, C wrappers + HAL queue/extract. */
#include "main.h"

extern int32_t rust_move_and_keep_on_screen(int32_t, int32_t);
extern int32_t rust_is_screen_switch_needed(int32_t, int32_t, uint16_t);
extern float rust_calculate_mouse_acceleration_factor(int32_t, int32_t, bool);
extern int16_t rust_scale_y_coordinate(int16_t, int32_t, int32_t, int32_t, int32_t);
extern void rust_output_mouse_report(device_t *, const uint8_t *);
extern void rust_switch_to_another_pc(device_t *, uint32_t, int, int);
extern void rust_switch_virtual_desktop(device_t *, uint8_t, int, int);
extern void rust_do_screen_switch(device_t *, int);
extern void rust_process_mouse_report(uint8_t *, int, uint8_t, void *, void *);

/* Wrappers */
enum screen_pos_e is_screen_switch_needed(int p, int o) {
    int32_t r = rust_is_screen_switch_needed(p, o, global_state.config.jump_threshold);
    return (r == -1) ? LEFT : (r == 1) ? RIGHT : NONE;
}
int32_t move_and_keep_on_screen(int p, int o) { return rust_move_and_keep_on_screen(p, o); }
float calculate_mouse_acceleration_factor(int32_t x, int32_t y) {
    return rust_calculate_mouse_acceleration_factor(x, y, global_state.config.enable_acceleration);
}
void output_mouse_report(mouse_report_t *r, device_t *s) { rust_output_mouse_report(s, (const uint8_t *)r); }
int16_t scale_y_coordinate(int f, int t, device_t *s) {
    return rust_scale_y_coordinate(s->pointer_y,
        s->config.output[f].border.top, s->config.output[f].border.bottom,
        s->config.output[t].border.top, s->config.output[t].border.bottom);
}
void switch_to_another_pc(device_t *s, output_t *o, int t, int d) { rust_switch_to_another_pc(s, o->number, t, d); }
void switch_virtual_desktop(device_t *s, output_t *o, int n, int d) { rust_switch_virtual_desktop(s, o->os, n, d); o->screen_index = n; }
void do_screen_switch(device_t *s, int d) { rust_do_screen_switch(s, d); }
void process_mouse_report(uint8_t *r, int l, uint8_t i, hid_interface_t *f) {
    rust_process_mouse_report(r, l, i, (void *)f, (void *)&global_state);
}

/* HAL: hid_interface_t mouse extraction */
static inline bool extract_value(bool uid, int32_t *dst, report_val_t *src, uint8_t *r, int l) {
    if (uid && (*r++ != src->report_id)) return false;
    *dst = get_report_value(r, l, src); return true;
}
void extract_report_values(uint8_t *r, int l, device_t *s, mouse_values_t *v, hid_interface_t *i) {
    if (i->protocol == HID_PROTOCOL_BOOT) {
        hid_mouse_report_t *m = (hid_mouse_report_t *)r;
        v->move_x=m->x; v->move_y=m->y; v->wheel=m->wheel; v->pan=m->pan; v->buttons=m->buttons; return;
    }
    mouse_t *m = &i->mouse; bool uid = i->uses_report_id;
    extract_value(uid,&v->move_x,&m->move_x,r,l); extract_value(uid,&v->move_y,&m->move_y,r,l);
    extract_value(uid,&v->wheel,&m->wheel,r,l); extract_value(uid,&v->pan,&m->pan,r,l);
    if (!extract_value(uid,&v->buttons,&m->buttons,r,l)) v->buttons = s->mouse_buttons;
}

/* HAL: queue_t + TinyUSB */
void process_mouse_queue_task(device_t *s) {
    mouse_report_t r = {0};
    if (!s->tud_connected || !queue_try_peek(&s->mouse_queue, &r)) return;
    if (tud_suspended()) tud_remote_wakeup();
    if (!tud_hid_n_ready(ITF_NUM_HID)) return;
    if (tud_mouse_report(r.mode, r.buttons, r.x, r.y, r.wheel, r.pan))
        queue_try_remove(&s->mouse_queue, &r);
}
void queue_mouse_report(mouse_report_t *r, device_t *s) {
    if (s->tud_connected) queue_try_add(&s->mouse_queue, r);
}

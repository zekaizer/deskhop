/* DeskHop mouse — logic in Rust, C wrappers + HAL queue. */
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

extern void rust_process_mouse_queue_task(device_t *), rust_queue_mouse_report(device_t *, const uint8_t *);
void process_mouse_queue_task(device_t *s) { rust_process_mouse_queue_task(s); }
void queue_mouse_report(mouse_report_t *r, device_t *s) { rust_queue_mouse_report(s, (const uint8_t *)r); }

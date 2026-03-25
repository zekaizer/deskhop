/* DeskHop protocol — API field map (offsetof) + queue helpers. HAL-dependent. */
#include "main.h"

const field_map_t api_field_map[] = {
    { 0,  true,  UINT8,  1, offsetof(device_t, active_output) },
    { 1,  true,  INT16,  2, offsetof(device_t, pointer_x) },
    { 2,  true,  INT16,  2, offsetof(device_t, pointer_y) },
    { 3,  true,  INT16,  2, offsetof(device_t, mouse_buttons) },
    { 10, false, UINT32, 4, offsetof(device_t, config.output[0].number) },
    { 11, false, UINT32, 4, offsetof(device_t, config.output[0].screen_count) },
    { 12, false, INT32,  4, offsetof(device_t, config.output[0].speed_x) },
    { 13, false, INT32,  4, offsetof(device_t, config.output[0].speed_y) },
    { 14, false, INT32,  4, offsetof(device_t, config.output[0].border.top) },
    { 15, false, INT32,  4, offsetof(device_t, config.output[0].border.bottom) },
    { 16, false, UINT8,  1, offsetof(device_t, config.output[0].os) },
    { 17, false, UINT8,  1, offsetof(device_t, config.output[0].pos) },
    { 18, false, UINT8,  1, offsetof(device_t, config.output[0].mouse_park_pos) },
    { 19, false, UINT8,  1, offsetof(device_t, config.output[0].screensaver.mode) },
    { 20, false, UINT8,  1, offsetof(device_t, config.output[0].screensaver.only_if_inactive) },
    { 21, false, UINT64, 7, offsetof(device_t, config.output[0].screensaver.idle_time_us) },
    { 22, false, UINT64, 7, offsetof(device_t, config.output[0].screensaver.max_time_us) },
    { 40, false, UINT32, 4, offsetof(device_t, config.output[1].number) },
    { 41, false, UINT32, 4, offsetof(device_t, config.output[1].screen_count) },
    { 42, false, INT32,  4, offsetof(device_t, config.output[1].speed_x) },
    { 43, false, INT32,  4, offsetof(device_t, config.output[1].speed_y) },
    { 44, false, INT32,  4, offsetof(device_t, config.output[1].border.top) },
    { 45, false, INT32,  4, offsetof(device_t, config.output[1].border.bottom) },
    { 46, false, UINT8,  1, offsetof(device_t, config.output[1].os) },
    { 47, false, UINT8,  1, offsetof(device_t, config.output[1].pos) },
    { 48, false, UINT8,  1, offsetof(device_t, config.output[1].mouse_park_pos) },
    { 49, false, UINT8,  1, offsetof(device_t, config.output[1].screensaver.mode) },
    { 50, false, UINT8,  1, offsetof(device_t, config.output[1].screensaver.only_if_inactive) },
    { 51, false, UINT64, 7, offsetof(device_t, config.output[1].screensaver.idle_time_us) },
    { 52, false, UINT64, 7, offsetof(device_t, config.output[1].screensaver.max_time_us) },
    { 70, false, UINT32, 4, offsetof(device_t, config.version) },
    { 71, false, UINT8,  1, offsetof(device_t, config.force_mouse_boot_mode) },
    { 72, false, UINT8,  1, offsetof(device_t, config.force_kbd_boot_protocol) },
    { 73, false, UINT8,  1, offsetof(device_t, config.kbd_led_as_indicator) },
    { 74, false, UINT8,  1, offsetof(device_t, config.hotkey_toggle) },
    { 75, false, UINT8,  1, offsetof(device_t, config.enable_acceleration) },
    { 76, false, UINT8,  1, offsetof(device_t, config.enforce_ports) },
    { 77, false, UINT16, 2, offsetof(device_t, config.jump_threshold) },
    { 78, true,  UINT16, 2, offsetof(device_t, _running_fw.version) },
    { 79, true,  UINT32, 4, offsetof(device_t, _running_fw.checksum) },
    { 80, true,  UINT8,  1, offsetof(device_t, keyboard_connected) },
    { 81, true,  UINT8,  1, offsetof(device_t, switch_lock) },
    { 82, true,  UINT8,  1, offsetof(device_t, relative_mouse) },
};

const field_map_t* get_field_map_entry(uint32_t idx) {
    for (unsigned i = 0; i < ARRAY_SIZE(api_field_map); i++)
        if (api_field_map[i].idx == idx) return &api_field_map[i];
    return NULL;
}

const field_map_t* get_field_map_index(uint32_t i) {
    return &api_field_map[i >= ARRAY_SIZE(api_field_map) ? ARRAY_SIZE(api_field_map) - 1 : i];
}

size_t get_field_map_length(void) { return ARRAY_SIZE(api_field_map); }

void _queue_packet(uint8_t *p, device_t *s, uint8_t t, uint8_t l, uint8_t id, uint8_t inst) {
    hid_generic_pkt_t g = { .instance=inst, .report_id=id, .type=t, .len=l };
    memcpy(g.data, p, l);
    queue_try_add(&s->hid_queue_out, &g);
}

void queue_cfg_packet(uart_packet_t *p, device_t *s) {
    uint8_t r[RAW_PACKET_LENGTH]; write_raw_packet(r, p);
    _queue_packet(r, s, 0, RAW_PACKET_LENGTH, REPORT_ID_VENDOR, ITF_NUM_HID_VENDOR);
}

void queue_cc_packet(uint8_t *p, device_t *s) { _queue_packet(p, s, 1, CONSUMER_CONTROL_LENGTH, REPORT_ID_CONSUMER, ITF_NUM_HID); }
void queue_system_packet(uint8_t *p, device_t *s) { _queue_packet(p, s, 2, SYSTEM_CONTROL_LENGTH, REPORT_ID_SYSTEM, ITF_NUM_HID); }

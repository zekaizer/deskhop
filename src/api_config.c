/* DeskHop API config — queue helpers only.
   Field map + lookup moved to Rust (hal/ffi/api_config.rs). */
#include "main.h"

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

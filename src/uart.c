/* DeskHop UART — packet dispatch + queue helpers.
   write_raw_packet, process_uart_tx_task, verify_checksum — Rust #[export_name] */
#include "main.h"

/* Output control */
void set_active_output(device_t *s, uint8_t o) {
    s->active_output=o; restore_leds(s); send_value(o, OUTPUT_SELECT_MSG); release_all_keys(s);
}

/* Packet queue helpers */
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

void queue_packet(const uint8_t *d, enum packet_type_e t, int l) {
    uart_packet_t p = {.type = t}; memcpy(p.data, d, l);
    queue_try_add(&global_state.uart_tx_queue, &p);
}
void send_value(const uint8_t v, enum packet_type_e t) { queue_packet(&v, t, sizeof(uint8_t)); }

/* Packet dispatcher — now implemented in Rust (service::packet_dispatch).
   process_packet is exported from Rust via #[export_name = "process_packet"]. */

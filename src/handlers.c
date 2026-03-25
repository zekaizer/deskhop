/* DeskHop handlers — HAL-only handlers + set_active_output.
   Hotkey handler wrappers removed — Rust handles hotkey dispatch directly. */
#include "main.h"

/* HAL-only handlers (still called from process_packet) */
void handle_fw_upgrade_msg(uart_packet_t *p, device_t *s) { reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0); }
void handle_consumer_control_msg(uart_packet_t *p, device_t *s) { queue_cc_packet(p->data, s); }
void handle_proxy_msg(uart_packet_t *p, device_t *s) { hal_queue_packet(&p->data[1], p->data[0], PACKET_DATA_LENGTH-1); }

void set_active_output(device_t *s, uint8_t o) {
    s->active_output=o; restore_leds(s); send_value(o, OUTPUT_SELECT_MSG); release_all_keys(s);
}

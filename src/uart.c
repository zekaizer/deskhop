/* DeskHop UART — encoding in Rust, queue/dispatch here. */
#include "main.h"

extern void rust_write_raw_packet(uint8_t *, const uint8_t *);
extern uint8_t rust_handle_simple_msg(uint8_t, const uint8_t *, device_t *);
extern void rust_process_uart_tx_task(device_t *);
extern void rust_handle_keyboard_uart_full(device_t *, const uint8_t *);
extern void rust_handle_mouse_uart_full(device_t *, const uint8_t *);
extern void rust_handle_output_select(device_t *, uint8_t);
extern void rust_handle_set_report(device_t *, uint8_t);
extern void rust_handle_sync_borders(device_t *, const uint8_t *);
extern void rust_handle_response_byte(const uint8_t *, device_t *);
extern void rust_handle_api_msgs(uint8_t, const uint8_t *, device_t *);
extern void rust_handle_api_read_all_msgs(device_t *);
extern void rust_handle_request_byte(uint8_t *);
extern bool rust_verify_checksum(const uint8_t *);

void write_raw_packet(uint8_t *d, uart_packet_t *p) { rust_write_raw_packet(d, (const uint8_t *)p); }
void process_uart_tx_task(device_t *s) { rust_process_uart_tx_task(s); }

void queue_packet(const uint8_t *d, enum packet_type_e t, int l) {
    uart_packet_t p = {.type = t}; memcpy(p.data, d, l);
    queue_try_add(&global_state.uart_tx_queue, &p);
}
void send_value(const uint8_t v, enum packet_type_e t) { queue_packet(&v, t, sizeof(uint8_t)); }

void process_packet(uart_packet_t *p, device_t *s) {
    if (!rust_verify_checksum((const uint8_t *)p)) return;
    switch (p->type) {
        case CONSUMER_CONTROL_MSG: handle_consumer_control_msg(p,s); return;
        case SYNC_BORDERS_MSG:     rust_handle_sync_borders(s, p->data); return;
        case GET_VAL_MSG: case SET_VAL_MSG: rust_handle_api_msgs(p->type, p->data, s); return;
        case GET_ALL_VALS_MSG:     rust_handle_api_read_all_msgs(s); return;
        case REQUEST_BYTE_MSG:     rust_handle_request_byte(p->data); return;
        case RESPONSE_BYTE_MSG:    rust_handle_response_byte(p->data, s); return;
        case FIRMWARE_UPGRADE_MSG: handle_fw_upgrade_msg(p,s); return;
        case PROXY_PACKET_MSG:     handle_proxy_msg(p,s); return;
        case KEYBOARD_REPORT_MSG:  rust_handle_keyboard_uart_full(s, p->data); return;
        case MOUSE_REPORT_MSG:     rust_handle_mouse_uart_full(s, p->data); return;
    }
    uint8_t hal = rust_handle_simple_msg(p->type, p->data, s);
    if (hal) switch (p->type) {
        case OUTPUT_SELECT_MSG:  rust_handle_output_select(s, p->data[0]); break;
        case KBD_SET_REPORT_MSG: rust_handle_set_report(s, p->data[0]); break;
        case FLASH_LED_MSG:      hal_blink_led(s); break;
        case WIPE_CONFIG_MSG:    hal_wipe_config(); hal_load_config(s); break;
        case SAVE_CONFIG_MSG:    hal_save_config(s); break;
        case REBOOT_MSG:         hal_reboot(); break;
        case HEARTBEAT_MSG:      break;
    }
}

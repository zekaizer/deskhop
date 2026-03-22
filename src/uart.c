/* DeskHop UART — encoding in Rust, queue/dispatch here. */
#include "main.h"

extern void rust_write_raw_packet(uint8_t *, const uint8_t *);
extern uint8_t rust_handle_simple_msg(uint8_t, const uint8_t *);
extern void rust_process_uart_tx_task(device_t *);

void write_raw_packet(uint8_t *d, uart_packet_t *p) { rust_write_raw_packet(d, (const uint8_t *)p); }
void process_uart_tx_task(device_t *s) { rust_process_uart_tx_task(s); }

void queue_packet(const uint8_t *d, enum packet_type_e t, int l) {
    uart_packet_t p = {.type = t}; memcpy(p.data, d, l);
    queue_try_add(&global_state.uart_tx_queue, &p);
}
void send_value(const uint8_t v, enum packet_type_e t) { queue_packet(&v, t, sizeof(uint8_t)); }

void process_packet(uart_packet_t *p, device_t *s) {
    if (!verify_checksum(p)) return;
    switch (p->type) {
        case CONSUMER_CONTROL_MSG: handle_consumer_control_msg(p,s); return;
        case SYNC_BORDERS_MSG: handle_sync_borders_msg(p,s); return;
        case GET_VAL_MSG: case SET_VAL_MSG: handle_api_msgs(p,s); return;
        case GET_ALL_VALS_MSG: handle_api_read_all_msg(p,s); return;
        case REQUEST_BYTE_MSG: handle_request_byte_msg(p,s); return;
        case RESPONSE_BYTE_MSG: handle_response_byte_msg(p,s); return;
        case FIRMWARE_UPGRADE_MSG: handle_fw_upgrade_msg(p,s); return;
        case PROXY_PACKET_MSG: handle_proxy_msg(p,s); return;
        case KEYBOARD_REPORT_MSG: handle_keyboard_uart_msg(p,s); return;
        case MOUSE_REPORT_MSG: handle_mouse_abs_uart_msg(p,s); return;
    }
    uint8_t hal = rust_handle_simple_msg(p->type, p->data);
    if (hal) switch (p->type) {
        case OUTPUT_SELECT_MSG: handle_output_select_msg(p,s); break;
        case KBD_SET_REPORT_MSG: handle_set_report_msg(p,s); break;
        case FLASH_LED_MSG: hal_blink_led(s); break;
        case WIPE_CONFIG_MSG: hal_wipe_config(); hal_load_config(s); break;
        case SAVE_CONFIG_MSG: hal_save_config(s); break;
        case REBOOT_MSG: hal_reboot(); break;
        case HEARTBEAT_MSG: break;
    }
}

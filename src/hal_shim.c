/*
 * C HAL shim layer — thin wrappers for all hardware-dependent operations.
 * Rust calls these via extern "C" FFI. All application logic lives in Rust.
 */

#include "main.h"

/* Layout verification is now compile-time only:
 * - C side: sdk_verify.h (_Static_assert on opaque sizes/alignment)
 * - Rust side: build.rs (bindgen parses structs.h, generates const assert) */

/* ==================================================== *
 * Timestamp
 * ==================================================== */

uint64_t hal_time_us_64(void) { return time_us_64(); }
uint32_t hal_time_us_32(void) { return time_us_32(); }

/* ==================================================== *
 * Queue operations (Pico SDK queue_t)
 * ==================================================== */

void hal_queue_mouse_report(const uint8_t *report) {
    // Call queue_try_add directly — do NOT call queue_mouse_report
    // which routes to rust_queue_mouse_report, causing infinite recursion.
    queue_try_add(queue_from_opaque(&global_hw.mouse_queue), report);
}

void hal_queue_kbd_report(const uint8_t *report) {
    // Same: avoid queue_kbd_report → rust_queue_kbd_report → here recursion.
    queue_try_add(queue_from_opaque(&global_hw.kbd_queue), report);
}

void hal_queue_uart_packet(const uint8_t *packet) {
    queue_try_add(queue_from_opaque(&global_hw.uart_tx_queue), packet);
}

bool hal_queue_try_add_uart(const uint8_t *data) {
    return queue_try_add(queue_from_opaque(&global_hw.uart_tx_queue), data);
}

/* ==================================================== *
 * UART packet send helpers
 * ==================================================== */

/* Removed: hal_send_value, hal_queue_packet, hal_save/load/wipe_config,
   hal_set_active_output, hal_restore_leds, hal_release_all_keys,
   hal_blink_led, hal_reboot — Rust calls underlying C functions directly.
   hal_watchdog_update kept (Pico SDK function). */

void blink_led(void) {
    global_led.blinks_left = 5;
    global_led.last_led_change = time_us_32();
}

/* UART packet + output control — moved from uart.c */
void queue_packet(const uint8_t *d, enum packet_type_e t, int l) {
    uart_packet_t p = {.type = t}; memcpy(p.data, d, l);
    queue_try_add(queue_from_opaque(&global_hw.uart_tx_queue), &p);
}
void send_value(const uint8_t v, enum packet_type_e t) { queue_packet(&v, t, sizeof(uint8_t)); }

/* set_active_output is now Rust #[export_name] in callbacks.rs */

void hal_watchdog_update(void) { watchdog_update(); }

void hal_reset_usb_boot(void) {
    reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
}

/* ==================================================== *
 * TinyUSB state queries
 * ==================================================== */

/* TinyUSB wrappers — TinyUSB functions are inline/macro, must stay in C */
bool hal_tud_ready(void) { return tud_ready(); }
bool hal_tud_suspended(void) { return tud_suspended(); }
void hal_tud_remote_wakeup(void) { tud_remote_wakeup(); }
bool hal_tud_hid_n_ready(uint8_t instance) { return tud_hid_n_ready(instance); }

/* TinyUSB host wrappers — called from Rust USB callback logic */
uint8_t hal_tuh_hid_interface_protocol(uint8_t dev_addr, uint8_t instance) {
    return tuh_hid_interface_protocol(dev_addr, instance);
}
uint8_t hal_tuh_hid_get_protocol(uint8_t dev_addr, uint8_t instance) {
    return tuh_hid_get_protocol(dev_addr, instance);
}
void hal_tuh_hid_set_protocol(uint8_t dev_addr, uint8_t instance, uint8_t protocol) {
    tuh_hid_set_protocol(dev_addr, instance, protocol);
}
bool hal_tuh_hid_receive_report(uint8_t dev_addr, uint8_t instance) {
    return tuh_hid_receive_report(dev_addr, instance);
}

/* LED / HID host wrappers — called from Rust LED logic */
void hal_gpio_put_led(bool state) { gpio_put(GPIO_LED_PIN, state); }
void hal_tuh_hid_set_report(uint8_t dev_addr, uint8_t instance, const uint8_t *data, uint8_t len) {
    tuh_hid_set_report(dev_addr, instance, 0, HID_REPORT_TYPE_OUTPUT, (void *)data, len);
}

bool hal_tud_hid_keyboard_report(uint8_t report_id, uint8_t modifier, const uint8_t *keycode) {
    return tud_hid_keyboard_report(report_id, modifier, (uint8_t *)keycode);
}

bool hal_tud_mouse_report(uint8_t mode, uint8_t buttons, int16_t x, int16_t y, int8_t wheel, int8_t pan) {
    return tud_mouse_report(mode, buttons, x, y, wheel, pan);
}

/* ==================================================== *
 * DMA state
 * ==================================================== */

bool hal_dma_channel_is_busy(void) {
    return dma_channel_is_busy(global_hw.dma_tx_channel);
}

void hal_dma_tx_send(const uint8_t *buf, uint32_t len) {
    memcpy(uart_txbuf, buf, len);
    dma_channel_transfer_from_buffer_now(global_hw.dma_tx_channel, uart_txbuf, len);
}

uint32_t hal_dma_rx_remaining(void) {
    return (uint32_t)DMA_RX_BUFFER_SIZE - dma_channel_hw_addr(global_hw.dma_rx_channel)->transfer_count;
}

bool hal_is_start_of_packet(void) {
    return uart_rxbuf[global_hw.dma_ptr] == START1
        && uart_rxbuf[NEXT_RING_IDX(global_hw.dma_ptr)] == START2;
}

void hal_fetch_packet(void) {
    uint8_t *dst = (uint8_t *)&global_hw.in_packet;
    for (int i = 0; i < RAW_PACKET_LENGTH; i++) {
        if (i >= START_LENGTH) dst[i - START_LENGTH] = uart_rxbuf[global_hw.dma_ptr];
        global_hw.dma_ptr = NEXT_RING_IDX(global_hw.dma_ptr);
    }
}

/* ==================================================== *
 * HID report extraction
 * ==================================================== */

/* extract_report_values now handled directly by Rust mouse_process */

/* hid_interface_t field setters for Rust extract_data */
void hal_set_report_handler(void *iface, uint8_t report_id, uint8_t handler_type) {
    /* handler_type: 0=mouse, 1=keyboard, 2=consumer, 3=system */
    if (report_id >= MAX_REPORTS) return;
    hid_interface_t *i = (hid_interface_t *)iface;
    switch (handler_type) {
        case 0: i->report_handler[report_id] = process_mouse_report; break;
        case 1: i->report_handler[report_id] = process_keyboard_report; break;
        case 2: i->report_handler[report_id] = process_consumer_report; break;
        case 3: i->report_handler[report_id] = process_system_report; break;
    }
}
/* HID generic packet queuing — inlined from uart.c */
static void _queue_packet(const uint8_t *p, uint8_t t, uint8_t l, uint8_t id, uint8_t inst) {
    hid_generic_pkt_t g = { .instance=inst, .report_id=id, .type=t, .len=l };
    memcpy(g.data, p, l);
    queue_try_add(queue_from_opaque(&global_hw.hid_queue_out), &g);
}
void hal_queue_cc_packet(const uint8_t *payload) {
    _queue_packet(payload, 1, CONSUMER_CONTROL_LENGTH, REPORT_ID_CONSUMER, ITF_NUM_HID);
}
void hal_queue_system_packet(const uint8_t *payload) {
    _queue_packet(payload, 2, SYSTEM_CONTROL_LENGTH, REPORT_ID_SYSTEM, ITF_NUM_HID);
}


/* ==================================================== *
 * HID keyboard extraction
 * ==================================================== */

/* extract_kbd_data now handled directly by Rust kbd_extract */

/* ==================================================== *
 * Keyboard hotkey check (wraps keyboard.c)
 * ==================================================== */

uint8_t hal_toggle_led(void) { return toggle_led(); }

bool hal_is_bootsel_pressed(void) {
#ifdef DH_DEBUG
    return is_bootsel_pressed();
#else
    return false;
#endif
}

/* hal_debug_dump_state is now Rust #[no_mangle] in callbacks.rs */

void hal_debug_blink(int count, int delay_ms) {
    for (int i = 0; i < count; i++) {
        gpio_put(GPIO_LED_PIN, 1); sleep_ms(delay_ms);
        gpio_put(GPIO_LED_PIN, 0); sleep_ms(delay_ms);
    }
}

/* Read 4 bytes from firmware running image at given address */
uint32_t hal_read_fw_running_u32(uint32_t address) {
    return *(uint32_t *)&ADDR_FW_RUNNING[address];
}

void hal_queue_cfg_packet(const uint8_t *packet) {
    uint8_t r[RAW_PACKET_LENGTH];
    write_raw_packet(r, (uart_packet_t *)packet);
    _queue_packet(r, 0, RAW_PACKET_LENGTH, REPORT_ID_VENDOR, ITF_NUM_HID_VENDOR);
}

/* API field map + access moved to Rust (hal/ffi/api_config.rs) */

/* HID output queue — generic HID reports waiting to be sent via TinyUSB */
bool hal_hid_queue_peek(uint8_t *out) {
    return queue_try_peek(queue_from_opaque(&global_hw.hid_queue_out), out);
}
bool hal_hid_queue_remove(uint8_t *out) {
    return queue_try_remove(queue_from_opaque(&global_hw.hid_queue_out), out);
}
bool hal_tud_hid_n_report(uint8_t instance, uint8_t report_id, const uint8_t *data, uint8_t len) {
    return tud_hid_n_report(instance, report_id, data, len);
}

/* Queue peek/remove for kbd and mouse */
bool hal_kbd_queue_peek(uint8_t *out) {
    return queue_try_peek(queue_from_opaque(&global_hw.kbd_queue), out);
}
bool hal_kbd_queue_remove(uint8_t *out) {
    return queue_try_remove(queue_from_opaque(&global_hw.kbd_queue), out);
}
bool hal_mouse_queue_peek(uint8_t *out) {
    return queue_try_peek(queue_from_opaque(&global_hw.mouse_queue), out);
}
bool hal_mouse_queue_remove(uint8_t *out) {
    return queue_try_remove(queue_from_opaque(&global_hw.mouse_queue), out);
}

bool hal_uart_tx_queue_remove(uint8_t *out) {
    return queue_try_remove(queue_from_opaque(&global_hw.uart_tx_queue), out);
}

void hal_set_config_mode_scratch(void) {
    watchdog_hw->scratch[5] = MAGIC_WORD_1;
    watchdog_hw->scratch[6] = MAGIC_WORD_2;
}

/* ==================================================== *
 * Trace output
 * ==================================================== */

#ifdef DH_DEBUG
void hal_trace_write(const uint8_t *buf, uint32_t len) {
    dh_debug_printf("%.*s", (int)len, (const char *)buf);
}
#else
void hal_trace_write(const uint8_t *buf, uint32_t len) {
    (void)buf; (void)len;
}
#endif

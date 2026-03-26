/*
 * C HAL shim layer — thin wrappers for all hardware-dependent operations.
 * Rust calls these via extern "C" FFI. All application logic lives in Rust.
 */

#include "main.h"

_Static_assert(sizeof(queue_t) == 16,
    "queue_t size changed — update QUEUE_T_SIZE in src-rust/src/app/structs.rs");

/* Verify Rust Device struct matches C device_t — called from initial_setup */
extern const uint32_t RUST_SIZEOF_DEVICE;
extern const uint32_t RUST_OFFSET_TUD_CONNECTED;
extern const uint32_t RUST_OFFSET_ACTIVE_OUTPUT;
extern const uint32_t RUST_OFFSET_CORE1_TIMESTAMP;
extern const uint32_t RUST_OFFSET_REBOOT_REQUESTED;
extern const uint32_t RUST_OFFSET_BLINKS_LEFT;
extern const uint32_t RUST_SIZEOF_HID_INTERFACE;
extern const uint32_t RUST_SIZEOF_KEYBOARD_DESC;
extern const uint32_t RUST_SIZEOF_MOUSE_DESC;
extern const uint32_t RUST_SIZEOF_REPORT_VAL;

/* Returns 0=OK, 1=sizeof, 2=tud_connected, 3=active_output,
   4=core1_timestamp, 5=reboot_requested, 6=blinks_left */
int hal_verify_device_layout(void) {
    if (RUST_SIZEOF_DEVICE != sizeof(device_t)) return 1;
    if (RUST_OFFSET_TUD_CONNECTED != offsetof(device_t, tud_connected)) return 2;
    if (RUST_OFFSET_ACTIVE_OUTPUT != offsetof(device_t, active_output)) return 3;
    if (RUST_OFFSET_CORE1_TIMESTAMP != offsetof(device_t, core1_last_loop_pass)) return 4;
    if (RUST_OFFSET_REBOOT_REQUESTED != offsetof(device_t, reboot_requested)) return 5;
    if (RUST_OFFSET_BLINKS_LEFT != offsetof(device_t, blinks_left)) return 6;
    return 0;
}

/* Debug: output C sizeof/offsetof values via CDC */
void hal_dump_layout(void) {
    dh_debug_printf("C sizeof(device_t)=%u Rust=%u\n", (unsigned)sizeof(device_t), (unsigned)RUST_SIZEOF_DEVICE);
    dh_debug_printf("C tud_connected=%u Rust=%u\n", (unsigned)offsetof(device_t, tud_connected), (unsigned)RUST_OFFSET_TUD_CONNECTED);
    dh_debug_printf("C active_output=%u Rust=%u\n", (unsigned)offsetof(device_t, active_output), (unsigned)RUST_OFFSET_ACTIVE_OUTPUT);
    dh_debug_printf("C core1_ts=%u Rust=%u\n", (unsigned)offsetof(device_t, core1_last_loop_pass), (unsigned)RUST_OFFSET_CORE1_TIMESTAMP);
    dh_debug_printf("C reboot=%u Rust=%u\n", (unsigned)offsetof(device_t, reboot_requested), (unsigned)RUST_OFFSET_REBOOT_REQUESTED);
    dh_debug_printf("C blinks=%u Rust=%u\n", (unsigned)offsetof(device_t, blinks_left), (unsigned)RUST_OFFSET_BLINKS_LEFT);

    /* Also dump intermediate offsets to find where mismatch starts */
    dh_debug_printf("C kbd_leds=%u\n", (unsigned)offsetof(device_t, keyboard_leds));
    dh_debug_printf("C last_activity=%u\n", (unsigned)offsetof(device_t, last_activity));
    dh_debug_printf("C config=%u\n", (unsigned)offsetof(device_t, config));
    dh_debug_printf("C hid_queue_out=%u\n", (unsigned)offsetof(device_t, hid_queue_out));
    dh_debug_printf("C iface=%u\n", (unsigned)offsetof(device_t, iface));
    dh_debug_printf("C in_packet=%u\n", (unsigned)offsetof(device_t, in_packet));
    dh_debug_printf("C dma_ptr=%u\n", (unsigned)offsetof(device_t, dma_ptr));
    dh_debug_printf("C fw=%u\n", (unsigned)offsetof(device_t, fw));
    dh_debug_printf("C page_buffer=%u\n", (unsigned)offsetof(device_t, page_buffer));
    dh_debug_printf("sizeof queue_t=%u\n", (unsigned)sizeof(queue_t));
    dh_debug_printf("sizeof hid_interface_t=%u Rust=%u\n", (unsigned)sizeof(hid_interface_t), (unsigned)RUST_SIZEOF_HID_INTERFACE);
    dh_debug_printf("sizeof keyboard_t=%u Rust=%u\n", (unsigned)sizeof(keyboard_t), (unsigned)RUST_SIZEOF_KEYBOARD_DESC);
    dh_debug_printf("sizeof mouse_t=%u Rust=%u\n", (unsigned)sizeof(mouse_t), (unsigned)RUST_SIZEOF_MOUSE_DESC);
    dh_debug_printf("sizeof report_val_t=%u Rust=%u\n", (unsigned)sizeof(report_val_t), (unsigned)RUST_SIZEOF_REPORT_VAL);
    dh_debug_printf("sizeof config_t=%u\n", (unsigned)sizeof(config_t));
}

/* ==================================================== *
 * Timestamp
 * ==================================================== */

uint64_t hal_time_us_64(void) { return time_us_64(); }
uint32_t hal_time_us_32(void) { return time_us_32(); }

/* ==================================================== *
 * Queue operations (Pico SDK queue_t)
 * ==================================================== */

void hal_queue_mouse_report(device_t *dev, const uint8_t *report) {
    // Call queue_try_add directly — do NOT call queue_mouse_report
    // which routes to rust_queue_mouse_report, causing infinite recursion.
    queue_try_add(&dev->mouse_queue, report);
}

void hal_queue_kbd_report(device_t *dev, const uint8_t *report) {
    // Same: avoid queue_kbd_report → rust_queue_kbd_report → here recursion.
    queue_try_add(&dev->kbd_queue, report);
}

void hal_queue_uart_packet(device_t *dev, const uint8_t *packet) {
    queue_try_add(&dev->uart_tx_queue, packet);
}

bool hal_queue_try_add_uart(device_t *dev, const uint8_t *data) {
    return queue_try_add(&dev->uart_tx_queue, data);
}

/* ==================================================== *
 * UART packet send helpers
 * ==================================================== */

/* Removed: hal_send_value, hal_queue_packet, hal_save/load/wipe_config,
   hal_set_active_output, hal_restore_leds, hal_release_all_keys,
   hal_blink_led, hal_reboot — Rust calls underlying C functions directly.
   hal_watchdog_update kept (Pico SDK function). */

void blink_led(device_t *state) {
    state->blinks_left = 5;
    state->last_led_change = time_us_32();
}

/* UART packet + output control — moved from uart.c */
void queue_packet(const uint8_t *d, enum packet_type_e t, int l) {
    uart_packet_t p = {.type = t}; memcpy(p.data, d, l);
    queue_try_add(&global_state.uart_tx_queue, &p);
}
void send_value(const uint8_t v, enum packet_type_e t) { queue_packet(&v, t, sizeof(uint8_t)); }

void set_active_output(device_t *s, uint8_t o) {
    s->active_output = o; restore_leds(s); send_value(o, OUTPUT_SELECT_MSG); release_all_keys(s);
}

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

bool hal_tud_hid_keyboard_report(uint8_t report_id, uint8_t modifier, const uint8_t *keycode) {
    return tud_hid_keyboard_report(report_id, modifier, (uint8_t *)keycode);
}

bool hal_tud_mouse_report(uint8_t mode, uint8_t buttons, int16_t x, int16_t y, int8_t wheel, int8_t pan) {
    return tud_mouse_report(mode, buttons, x, y, wheel, pan);
}

/* ==================================================== *
 * DMA state
 * ==================================================== */

bool hal_dma_channel_is_busy(device_t *dev) {
    return dma_channel_is_busy(dev->dma_tx_channel);
}

void hal_dma_tx_send(device_t *dev, const uint8_t *buf, uint32_t len) {
    memcpy(uart_txbuf, buf, len);
    dma_channel_transfer_from_buffer_now(dev->dma_tx_channel, uart_txbuf, len);
}

uint32_t hal_dma_rx_remaining(device_t *dev) {
    return (uint32_t)DMA_RX_BUFFER_SIZE - dma_channel_hw_addr(dev->dma_rx_channel)->transfer_count;
}

bool hal_is_start_of_packet(device_t *dev) {
    return is_start_of_packet(dev);
}

void hal_fetch_packet(device_t *dev) {
    fetch_packet(dev);
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
static void _queue_packet(const uint8_t *p, device_t *s, uint8_t t, uint8_t l, uint8_t id, uint8_t inst) {
    hid_generic_pkt_t g = { .instance=inst, .report_id=id, .type=t, .len=l };
    memcpy(g.data, p, l);
    queue_try_add(&s->hid_queue_out, &g);
}
void hal_queue_cc_packet(device_t *dev, const uint8_t *payload) {
    _queue_packet(payload, dev, 1, CONSUMER_CONTROL_LENGTH, REPORT_ID_CONSUMER, ITF_NUM_HID);
}
void hal_queue_system_packet(device_t *dev, const uint8_t *payload) {
    _queue_packet(payload, dev, 2, SYSTEM_CONTROL_LENGTH, REPORT_ID_SYSTEM, ITF_NUM_HID);
}


/* ==================================================== *
 * HID keyboard extraction
 * ==================================================== */

/* extract_kbd_data now handled directly by Rust kbd_extract */

/* ==================================================== *
 * Keyboard hotkey check (wraps keyboard.c)
 * ==================================================== */

extern device_t *device;

uint8_t hal_toggle_led(void) { return toggle_led(); }

void hal_debug_dump_state(device_t *dev) {
    dh_debug_printf("tud=%d kbd=%d mse=%d role=%d out=%d c1=%llu\n",
        dev->tud_connected, dev->keyboard_connected, dev->mouse_connected,
        dev->board_role, dev->active_output, dev->core1_last_loop_pass);
}

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

void hal_queue_cfg_packet(device_t *dev, const uint8_t *packet) {
    uint8_t r[RAW_PACKET_LENGTH];
    write_raw_packet(r, (uart_packet_t *)packet);
    _queue_packet(r, dev, 0, RAW_PACKET_LENGTH, REPORT_ID_VENDOR, ITF_NUM_HID_VENDOR);
}

/* API field map + access moved to Rust (hal/ffi/api_config.rs) */

/* Queue peek/remove for kbd and mouse */
bool hal_kbd_queue_peek(device_t *dev, uint8_t *out) {
    return queue_try_peek(&dev->kbd_queue, out);
}
bool hal_kbd_queue_remove(device_t *dev, uint8_t *out) {
    return queue_try_remove(&dev->kbd_queue, out);
}
bool hal_mouse_queue_peek(device_t *dev, uint8_t *out) {
    return queue_try_peek(&dev->mouse_queue, out);
}
bool hal_mouse_queue_remove(device_t *dev, uint8_t *out) {
    return queue_try_remove(&dev->mouse_queue, out);
}

bool hal_uart_tx_queue_remove(device_t *dev, uint8_t *out) {
    return queue_try_remove(&dev->uart_tx_queue, out);
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

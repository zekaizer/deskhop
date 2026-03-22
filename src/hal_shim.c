/*
 * C HAL shim layer — thin wrappers for all hardware-dependent operations.
 * Rust calls these via extern "C" FFI. All application logic lives in Rust.
 */

#include "main.h"

_Static_assert(sizeof(queue_t) == 16,
    "queue_t size changed — update QUEUE_T_SIZE in src-rust/src/app/structs.rs");

/* ==================================================== *
 * Timestamp
 * ==================================================== */

uint64_t hal_time_us_64(void) { return time_us_64(); }
uint32_t hal_time_us_32(void) { return time_us_32(); }

/* ==================================================== *
 * Queue operations (Pico SDK queue_t)
 * ==================================================== */

void hal_queue_mouse_report(device_t *dev, const uint8_t *report) {
    queue_mouse_report((mouse_report_t *)report, dev);
}

void hal_queue_kbd_report(device_t *dev, const uint8_t *report) {
    queue_kbd_report((hid_keyboard_report_t *)report, dev);
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

void hal_send_value(uint8_t value, uint8_t packet_type) {
    send_value(value, (enum packet_type_e)packet_type);
}

void hal_queue_packet(const uint8_t *data, uint8_t packet_type, int length) {
    queue_packet(data, (enum packet_type_e)packet_type, length);
}

/* ==================================================== *
 * Config / Flash
 * ==================================================== */

void hal_save_config(device_t *dev) { save_config(dev); }
void hal_load_config(device_t *dev) { load_config(dev); }
void hal_wipe_config(void) { wipe_config(); }

/* ==================================================== *
 * Output switching / LEDs
 * ==================================================== */

void hal_set_active_output(device_t *dev, uint8_t output) {
    set_active_output(dev, output);
}

void hal_restore_leds(device_t *dev) { restore_leds(dev); }
void hal_release_all_keys(device_t *dev) { release_all_keys(dev); }

/* ==================================================== *
 * Hardware (GPIO, watchdog, reboot)
 * ==================================================== */

void hal_watchdog_update(void) { watchdog_update(); }
void hal_blink_led(device_t *dev) { blink_led(dev); }
void hal_reboot(void) { reboot(); }

void hal_reset_usb_boot(void) {
    reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
}

/* ==================================================== *
 * TinyUSB state queries
 * ==================================================== */

bool hal_tud_ready(void) { return tud_ready(); }
bool hal_tud_connected(void) { return tud_connected(); }
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
 * HID report extraction (wraps hid_report.c functions)
 * ==================================================== */

int32_t hal_extract_kbd_data(uint8_t *raw_report, int len, uint8_t itf,
                             void *iface, uint8_t *out_report) {
    return extract_kbd_data(raw_report, len, itf, (hid_interface_t *)iface,
                           (hid_keyboard_report_t *)out_report);
}

/* ==================================================== *
 * Keyboard hotkey check (wraps keyboard.c)
 * ==================================================== */

extern device_t *device;

int hal_check_all_hotkeys(const uint8_t *report, uint8_t *out_pass_to_os,
                          uint8_t *out_acknowledge) {
    hid_keyboard_report_t *kbd_report = (hid_keyboard_report_t *)report;
    hotkey_combo_t *hotkey = check_all_hotkeys(kbd_report, device);

    if (hotkey == NULL)
        return -1;

    *out_pass_to_os = hotkey->pass_to_os;
    *out_acknowledge = hotkey->acknowledge;

    /* Execute the handler */
    hotkey->action_handler(device, kbd_report);

    return 0;
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

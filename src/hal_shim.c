/*
 * C HAL shim layer — thin wrappers for all hardware-dependent operations.
 * Rust calls these via extern "C" FFI. All application logic lives in Rust.
 */

#include "main.h"

/* Layout verification is now compile-time only:
 * - C side: sdk_verify.h (_Static_assert on opaque sizes/alignment)
 * - Rust side: build.rs (bindgen parses structs.h, generates const assert) */

/* ==================================================== *
 * Boot-stage LED — non-blocking hardware-timer driver.
 * Drives the boot indicator during initial_setup without blocking Core0,
 * so the main loop (tud_task) is not stalled. See domain/boot_led.rs.
 * ==================================================== */

extern void rust_boot_led_tick(void);
static repeating_timer_t boot_led_timer;
static bool boot_led_running = false;

static bool boot_led_cb(repeating_timer_t *rt) {
    (void)rt;
    rust_boot_led_tick();
    return true; /* keep repeating */
}

void hal_boot_led_start(void) {
    if (boot_led_running)
        return;
    /* Negative period => fire every 100ms relative to the scheduled time, not
     * the callback return, keeping a steady cadence. */
    if (add_repeating_timer_ms(-100, boot_led_cb, NULL, &boot_led_timer))
        boot_led_running = true;
}

void hal_boot_led_stop(void) {
    if (!boot_led_running)
        return;
    cancel_repeating_timer(&boot_led_timer);
    boot_led_running = false;
}

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

/* Free slots in the UART TX queue — lets the peer-log forwarder drain only as
 * much as the queue can accept per tick, so a burst (e.g. the boot HID
 * descriptor dump) buffers in TX_RING instead of overflowing/garbling. */
uint32_t hal_uart_tx_free(void) {
    uint level = queue_get_level(queue_from_opaque(&global_hw.uart_tx_queue));
    return (level < UART_QUEUE_LENGTH) ? (UART_QUEUE_LENGTH - level) : 0;
}

/* ==================================================== *
 * UART packet send helpers
 * ==================================================== */

/* Removed: hal_send_value, hal_queue_packet, hal_save/load/wipe_config,
   hal_set_active_output, hal_restore_leds, hal_release_all_keys,
   hal_blink_led, hal_reboot — Rust calls underlying C functions directly.
   hal_watchdog_update kept (Pico SDK function). */

/* blink_led is now Rust #[export_name] in callbacks.rs */

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

/* Flash config wrappers — called from Rust config logic */
void hal_flash_read_config(uint8_t *buf, uint32_t len) {
    memcpy(buf, ADDR_CONFIG, len);
}
void hal_flash_write_config(const uint8_t *buf) {
    write_flash_page((uint32_t)ADDR_CONFIG - XIP_BASE, (uint8_t *)buf);
}

/* LED / HID host wrappers — called from Rust LED logic */
void hal_gpio_put_led(bool state) { gpio_put(GPIO_LED_PIN, state); }
bool hal_gpio_get_led(void) { return gpio_get(GPIO_LED_PIN); }
void hal_tuh_hid_set_report(uint8_t dev_addr, uint8_t instance, const uint8_t *data, uint8_t len) {
    tuh_hid_set_report(dev_addr, instance, 0, HID_REPORT_TYPE_OUTPUT, (void *)data, len);
}

/* True when running on Core1 (the USB host stack core). Used to gate host-stack
 * access so Core0 contexts defer instead of racing tuh_task. */
bool hal_is_core1(void) { return get_core_num() == 1; }

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

/* Used by Rust packet_receive_tick to scan past non-preamble bytes. */
void hal_dma_advance_one(void) {
    global_hw.dma_ptr = NEXT_RING_IDX(global_hw.dma_ptr);
}

uint32_t hal_dma_read_pos(void) {
    return global_hw.dma_ptr;
}

const uint8_t *hal_get_in_packet_ptr(void) {
    return (const uint8_t *)&global_hw.in_packet;
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

/* Queue a passthrough HID output report for the Core0 process_hid_queue task to
 * send via tud_hid_n_report. Lets Core1 (tuh callbacks) forward host reports to
 * the device side WITHOUT touching the device stack directly (cross-core race). */
void hal_queue_hid_report(uint8_t instance, uint8_t report_id, const uint8_t *data, uint8_t len) {
    if (len > HID_REPORT_DATA_MAX) len = HID_REPORT_DATA_MAX;
    _queue_packet(data, 0, len, report_id, instance);
}


/* ==================================================== *
 * HID keyboard extraction
 * ==================================================== */

/* extract_kbd_data now handled directly by Rust kbd_extract */

/* ==================================================== *
 * Keyboard hotkey check (wraps keyboard.c)
 * ==================================================== */

/* hal_toggle_led is now Rust #[no_mangle] in callbacks.rs */

bool hal_is_bootsel_pressed(void) {
#ifdef DH_DEBUG
    return is_bootsel_pressed();
#else
    return false;
#endif
}

/* hal_debug_dump_state is now Rust #[no_mangle] in callbacks.rs */

/* Heap high-water-mark for sizing the log ring reserve. mallinfo().arena is the
 * total memory sbrk'd from the system; since the heap never shrinks (sbrk is
 * one-way here) it equals the peak heap extent. uordblks is currently in-use. */
#include <malloc.h>
uint32_t hal_heap_arena(void) { return (uint32_t) mallinfo().arena; }
uint32_t hal_heap_inuse(void) { return (uint32_t) mallinfo().uordblks; }

/* Heap ceiling: bytes available between the heap base (__end__, which the log
 * ring pushes up) and __StackLimit (RAM end). This is the hard cap the ring's
 * __LOG_HEAP_RESERVE leaves for malloc — early-warn when arena nears it. */
uint32_t hal_heap_limit(void) {
    extern char __end__, __StackLimit;
    return (uint32_t)((uintptr_t)&__StackLimit - (uintptr_t)&__end__);
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

/* Enable the hardware watchdog. Config-mode entry relies on a watchdog-timeout
 * reset (it preserves the scratch flag, unlike a bare AIRCR reset); debug builds
 * leave the watchdog disabled at boot, so entering config mode must arm it here
 * or the reboot (and thus config mode) never happens. Idempotent in release. */
void hal_watchdog_enable(void) {
    watchdog_enable(WATCHDOG_TIMEOUT, WATCHDOG_PAUSE_ON_DEBUG);
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

/* ==================================================== *
 * Peer log spinlock — cross-core MPSC ring buffer guard.
 * Used by Rust service::peer_log to forward debug logs over UART
 * (DH_DEBUG only). Initialized once during initial_setup().
 * ==================================================== */

#ifdef DH_DEBUG
static spin_lock_t *peer_log_spin = NULL;

void peer_log_lock_init(void) {
    int n = spin_lock_claim_unused(true);
    peer_log_spin = spin_lock_init((uint)n);
}

uint32_t peer_log_lock_acquire(void) {
    return spin_lock_blocking(peer_log_spin);
}

void peer_log_lock_release(uint32_t saved_irq) {
    spin_unlock(peer_log_spin, saved_irq);
}

bool peer_log_cdc_connected(void) {
    return tud_cdc_connected();
}

uint32_t peer_log_cdc_write(const uint8_t *data, uint32_t len) {
    uint32_t avail = (uint32_t)tud_cdc_write_available();
    if (len > avail) len = avail;
    return (uint32_t)tud_cdc_write(data, len);
}

uint32_t peer_log_cdc_write_avail(void) {
    return (uint32_t)tud_cdc_write_available();
}

void peer_log_cdc_flush(void) {
    tud_cdc_write_flush();
}
#else
void peer_log_lock_init(void) {}
uint32_t peer_log_lock_acquire(void) { return 0; }
void peer_log_lock_release(uint32_t saved_irq) { (void)saved_irq; }
bool peer_log_cdc_connected(void) { return false; }
uint32_t peer_log_cdc_write(const uint8_t *data, uint32_t len) { (void)data; (void)len; return 0; }
void peer_log_cdc_flush(void) {}
#endif

/* ==================================================== *
 * Passthrough (Semi-DDM) FFI
 * ==================================================== */

void hal_tud_disconnect(void) { tud_disconnect(); }
void hal_tud_connect(void) { tud_connect(); }

void hal_tuh_vid_pid_get(uint8_t dev_addr, uint16_t *vid, uint16_t *pid) {
    tuh_vid_pid_get(dev_addr, vid, pid);
}

/* Rust FFI accessors for passthrough interface data */
extern uint8_t pt_iface_protocol(uint8_t idx);
extern uint16_t pt_iface_desc_len(uint8_t idx);

/* Report descriptor sizes from usb_descriptors.c */
extern const uint16_t desc_hid_report_size;
extern const uint16_t desc_hid_report_relmouse_size;

/* Append one HID interface descriptor block using TinyUSB macro.
 * Returns the number of bytes written, or 0 if the buffer would overflow. */
static uint16_t _append_hid_itf(uint8_t *buf, uint16_t off, uint16_t buf_size,
                                 uint8_t itf_num, uint8_t str_idx,
                                 uint8_t protocol, uint16_t report_desc_len,
                                 uint8_t ep_addr, uint16_t ep_size, uint8_t ep_interval) {
    const uint8_t desc[] = {
        TUD_HID_DESCRIPTOR(itf_num, str_idx, protocol, report_desc_len,
                           ep_addr, ep_size, ep_interval)
    };
    if (off + sizeof(desc) > buf_size) return 0;
    memcpy(buf + off, desc, sizeof(desc));
    return sizeof(desc);
}

/* Build passthrough config descriptor using TinyUSB macros.
 * Reads interface data from Rust via FFI accessors. Returns *config_desc_len
 * = 0 if the supplied buffer is too small for the resulting descriptor. */
void hal_passthrough_build_config_desc(uint8_t *config_desc, uint16_t buf_size,
                                        uint16_t *config_desc_len, uint8_t iface_count) {
    uint8_t *buf = config_desc;
    uint16_t off = 0;
    uint8_t num_itf = 2 + iface_count; /* DeskHop base + passthrough */
    uint16_t written;

#ifdef DH_DEBUG
    num_itf += 2; /* CDC Communication + Data interfaces */
#endif

    /* The device-side TinyUSB HID class pool has CFG_TUD_HID instances. The
     * composite declares 2 DeskHop base HID interfaces + iface_count passthrough
     * HID interfaces (CDC does not consume a HID slot). If that exceeds
     * CFG_TUD_HID, hidd_open() cannot claim every interface and the host's
     * SET_CONFIGURATION stalls on re-enumeration — the device never mounts.
     * Refuse to build such a descriptor so the caller keeps the default DeskHop
     * identity (recoverable via config-mode hotkey) instead of bricking. */
    if (2 + iface_count > CFG_TUD_HID) goto overflow;

    /* Configuration descriptor header (9 bytes) */
    if (off + 9 > buf_size) goto overflow;
    buf[off++] = 9;
    buf[off++] = TUSB_DESC_CONFIGURATION;
    off += 2; /* wTotalLength — filled at end */
    buf[off++] = num_itf;
    buf[off++] = 1;    /* bConfigurationValue */
    buf[off++] = 0;    /* iConfiguration */
    buf[off++] = 0x80 | TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP;
    buf[off++] = 250;  /* bMaxPower: 500mA / 2 */

    /* DeskHop ITF 0: main HID (keyboard + abs mouse + consumer + system) */
    written = _append_hid_itf(buf, off, buf_size, ITF_NUM_HID, 2 /* STRID_PRODUCT */,
                              HID_ITF_PROTOCOL_NONE, desc_hid_report_size,
                              0x81, CFG_TUD_HID_EP_BUFSIZE, 1);
    if (written == 0) goto overflow;
    off += written;

    /* DeskHop ITF 1: relative mouse helper */
    written = _append_hid_itf(buf, off, buf_size, ITF_NUM_HID_REL_M, 4 /* STRID_MOUSE */,
                              HID_ITF_PROTOCOL_NONE, desc_hid_report_relmouse_size,
                              0x82, CFG_TUD_HID_EP_BUFSIZE, 1);
    if (written == 0) goto overflow;
    off += written;

    /* Passthrough interfaces (data from Rust via FFI) */
    for (uint8_t i = 0; i < iface_count; i++) {
        written = _append_hid_itf(buf, off, buf_size, ITF_NUM_PT_BASE + i, 0,
                                  pt_iface_protocol(i),
                                  pt_iface_desc_len(i),
                                  EPNUM_PT_BASE + i,
                                  CFG_TUD_HID_EP_BUFSIZE, 1);
        if (written == 0) goto overflow;
        off += written;
    }

#ifdef DH_DEBUG
    {
        uint8_t cdc_itf  = ITF_NUM_PT_BASE + iface_count;
        uint8_t ep_notif = 0x80 | (3 + iface_count);
        uint8_t ep_out   = (uint8_t)(3 + iface_count + 1);
        uint8_t ep_in    = 0x80 | (3 + iface_count + 1);

        const uint8_t cdc[] = {
            TUD_CDC_DESCRIPTOR(cdc_itf, 7 /* STRID_DEBUG */, ep_notif, 8,
                               ep_out, ep_in, 64)
        };
        if (off + sizeof(cdc) > buf_size) goto overflow;
        memcpy(buf + off, cdc, sizeof(cdc));
        off += sizeof(cdc);
    }
#endif

    /* Fill wTotalLength (bytes 2-3 of config header) */
    buf[2] = (uint8_t)(off);
    buf[3] = (uint8_t)(off >> 8);

    *config_desc_len = off;
    return;

overflow:
    *config_desc_len = 0;
}

bool hal_tuh_set_report(uint8_t dev_addr, uint8_t itf_num,
                        uint8_t report_id, uint8_t report_type,
                        const uint8_t *data, uint16_t len) {
    static uint8_t buf[33];
    buf[0] = report_id;
    if (len > sizeof(buf) - 1) len = sizeof(buf) - 1;
    memcpy(buf + 1, data, len);
    uint16_t full_len = len + 1;

    static tusb_control_request_t request;
    request = (tusb_control_request_t){
        .bmRequestType_bit = {
            .recipient = TUSB_REQ_RCPT_INTERFACE,
            .type      = TUSB_REQ_TYPE_CLASS,
            .direction = TUSB_DIR_OUT
        },
        .bRequest = HID_REQ_CONTROL_SET_REPORT,
        .wValue   = tu_htole16((uint16_t)((report_type << 8) | report_id)),
        .wIndex   = tu_htole16((uint16_t)itf_num),
        .wLength  = tu_htole16(full_len)
    };

    tuh_xfer_t xfer = {
        .daddr       = dev_addr,
        .ep_addr     = 0,
        .setup       = &request,
        .buffer      = buf,
        .complete_cb = NULL,
        .user_data   = 0
    };

    return tuh_control_xfer(&xfer);
}

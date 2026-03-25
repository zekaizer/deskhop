/* DeskHop utils — CRC/checksum in Rust, HAL (flash/GPIO/config) here. */
#include "main.h"

extern uint8_t rust_calc_checksum(const uint8_t *, int);
extern uint32_t rust_calc_crc32(const uint8_t *, size_t), rust_crc32_iter(uint32_t, uint8_t),
    rust_get_ptr_delta(uint32_t, uint32_t, uint32_t);
extern bool rust_verify_checksum(const uint8_t *), rust_validate_packet(const uint8_t *);

/* calc_checksum: called only via header decl — linker resolves to rust_calc_checksum if needed */
uint8_t calc_checksum(const uint8_t *d, int l) { return rust_calc_checksum(d, l); }
bool verify_checksum(const uart_packet_t *p) { return rust_verify_checksum((const uint8_t *)p); }
uint32_t crc32_iter(uint32_t c, const uint8_t b) { return rust_crc32_iter(c, b); }
uint32_t calc_crc32(const uint8_t *s, size_t n) { return rust_calc_crc32(s, n); }
uint32_t get_ptr_delta(uint32_t cp, device_t *s) { return rust_get_ptr_delta(cp, s->dma_ptr, DMA_RX_BUFFER_SIZE); }
bool validate_packet(uart_packet_t *p) { return rust_validate_packet((const uint8_t *)p); }

/* HAL: flash */
uint32_t calculate_firmware_crc32(void) { return calc_crc32(ADDR_FW_RUNNING, STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE); }

void wipe_config(void) {
    uint32_t ints = save_and_disable_interrupts();
    flash_range_erase((uint32_t)ADDR_CONFIG - XIP_BASE, FLASH_SECTOR_SIZE);
    restore_interrupts(ints);
}

void write_flash_page(uint32_t addr, uint8_t *buf) {
    bool is_start = (addr & 0xf00) == 0;
    uint32_t ints = save_and_disable_interrupts();
    if (is_start) flash_range_erase(addr, FLASH_SECTOR_SIZE);
    flash_range_program(addr, buf, FLASH_PAGE_SIZE);
    restore_interrupts(ints);
}

void load_config(device_t *state) {
    const config_t *config = ADDR_CONFIG;
    config_t *rc = &state->config;
    memcpy(rc, config, sizeof(config_t));
    uint8_t cs = calc_crc32((uint8_t *)rc, sizeof(config_t) - sizeof(uint32_t));
    if (rc->magic_header != 0xB00B1E5 || rc->checksum != cs || rc->version != CURRENT_CONFIG_VERSION)
        memcpy(rc, &default_config, sizeof(config_t));
}

void save_config(device_t *state) {
    uint8_t *raw = (uint8_t *)&state->config;
    /* Truncate CRC32 to uint8_t — must match load_config's uint8_t comparison */
    uint8_t checksum = calc_crc32(raw, sizeof(config_t) - sizeof(uint32_t));
    state->config.checksum = checksum;
    memcpy(state->page_buffer, raw, sizeof(config_t));
    memset(state->page_buffer + sizeof(config_t), 0, FLASH_PAGE_SIZE - sizeof(config_t));
    write_flash_page((uint32_t)ADDR_CONFIG - XIP_BASE, state->page_buffer);
}

void reset_config_timer(device_t *s) { s->config_mode_timer = hal_time_us_64() + CONFIG_MODE_TIMEOUT; }

/* HAL: GPIO */
void _configure_flash_cs(enum gpio_override gpo, uint pin) {
    hw_write_masked(&ioqspi_hw->io[pin].ctrl, gpo << IO_QSPI_GPIO_QSPI_SS_CTRL_OEOVER_LSB,
                    IO_QSPI_GPIO_QSPI_SS_CTRL_OEOVER_BITS);
}

bool is_bootsel_pressed(void) {
    const uint CS = 1;
    uint32_t f = save_and_disable_interrupts();
    _configure_flash_cs(GPIO_OVERRIDE_LOW, CS);
    sleep_us(20);
    bool pressed = !(sio_hw->gpio_hi_in & (1u << CS));
    _configure_flash_cs(GPIO_OVERRIDE_NORMAL, CS);
    restore_interrupts(f);
    return pressed;
}

/* HAL: queue + UART */
void request_byte(device_t *state, uint32_t address) {
    uart_packet_t p = { .data32[0] = address, .type = REQUEST_BYTE_MSG };
    state->fw.byte_done = false;
    queue_try_add(&global_state.uart_tx_queue, &p);
}

void reboot(void) { *((volatile uint32_t*)(PPB_BASE + 0x0ED0C)) = 0x5FA0004; }

/* HAL: DMA buffer */
bool is_start_of_packet(device_t *s) {
    return uart_rxbuf[s->dma_ptr] == START1 && uart_rxbuf[NEXT_RING_IDX(s->dma_ptr)] == START2;
}

void fetch_packet(device_t *state) {
    uint8_t *dst = (uint8_t *)&state->in_packet;
    for (int i = 0; i < RAW_PACKET_LENGTH; i++) {
        if (i >= START_LENGTH) dst[i - START_LENGTH] = uart_rxbuf[state->dma_ptr];
        state->dma_ptr = NEXT_RING_IDX(state->dma_ptr);
    }
}

/* Debug */
#ifdef DH_DEBUG
static void cdc_write_str(const char *str) {
    int len = strlen(str);
    if (!tud_cdc_connected()) return;
    uint64_t t = time_us_64();
    for (int w = 0; w < len;) {
        int r = len - w, a = (int)tud_cdc_write_available();
        int c = (r < a) ? r : a;
        if (c > 0) { w += (int)tud_cdc_write(str + w, (uint32_t)c); tud_task(); tud_cdc_write_flush(); t = time_us_64(); }
        else { tud_task(); tud_cdc_write_flush(); if (!tud_cdc_connected() || time_us_64() > t + 1000) break; }
    }
}

int dh_debug_printf(const char *fmt, ...) {
    va_list a; va_start(a, fmt); char b[512];
    int l = vsnprintf(b, 512, fmt, a);
    /* Convert \n to \r\n for CDC terminal compatibility */
    char cr[1024]; int j = 0;
    for (int i = 0; i < l && j < 1022; i++) {
        if (b[i] == '\n' && (i == 0 || b[i-1] != '\r')) cr[j++] = '\r';
        cr[j++] = b[i];
    }
    cr[j] = '\0';
    cdc_write_str(cr); tud_cdc_write_flush(); va_end(a); return l;
}
#else
int dh_debug_printf(const char *fmt, ...) { return 0; }
#endif

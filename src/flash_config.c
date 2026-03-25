/* DeskHop flash config — save/load/wipe config + flash page write. */
#include "main.h"

extern uint8_t rust_calc_checksum(const uint8_t *, int);
extern uint32_t rust_calc_crc32(const uint8_t *, size_t), rust_crc32_iter(uint32_t, uint8_t);
extern bool rust_verify_checksum(const uint8_t *), rust_validate_packet(const uint8_t *);

/* CRC/checksum wrappers — delegate to Rust implementations */
uint8_t calc_checksum(const uint8_t *d, int l) { return rust_calc_checksum(d, l); }
bool verify_checksum(const uart_packet_t *p) { return rust_verify_checksum((const uint8_t *)p); }
uint32_t crc32_iter(uint32_t c, const uint8_t b) { return rust_crc32_iter(c, b); }
uint32_t calc_crc32(const uint8_t *s, size_t n) { return rust_calc_crc32(s, n); }
bool validate_packet(uart_packet_t *p) { return rust_validate_packet((const uint8_t *)p); }

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

/* DeskHop HAL utilities — flash config + debug output + BOOTSEL. */
#include "main.h"

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

/* load_config, save_config, reset_config_timer are now Rust #[export_name] in callbacks.rs */

/* GPIO: BOOTSEL button */
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

/* Debug output: forward formatted text to the Rust peer-log ring buffer.
 * The drain task in service::peer_log either packetizes to UART (board A)
 * or writes to local CDC (board B) — see Rust side for routing. */
#ifdef DH_DEBUG
extern void peer_log_push(const uint8_t *data, size_t len);

int dh_debug_printf(const char *fmt, ...) {
    va_list a; va_start(a, fmt); char b[512];
    int l = vsnprintf(b, 512, fmt, a);
    va_end(a);
    if (l <= 0) return l;
    if (l > 512) l = 512;
    /* Convert \n to \r\n for CDC terminal compatibility */
    char cr[1024]; int j = 0;
    for (int i = 0; i < l && j < 1022; i++) {
        if (b[i] == '\n' && (i == 0 || b[i-1] != '\r')) cr[j++] = '\r';
        cr[j++] = b[i];
    }
    peer_log_push((const uint8_t *)cr, (size_t)j);
    return l;
}
#else
int dh_debug_printf(const char *fmt, ...) { return 0; }
#endif

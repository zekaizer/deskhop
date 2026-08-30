/* DeskHop HAL utilities — flash config + debug output + BOOTSEL. */
#include "main.h"
#include "pico/flash.h"
#include "hardware/structs/psm.h"

uint32_t calculate_firmware_crc32(void) { return calc_crc32(ADDR_FW_RUNNING, STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE); }

/* CRC over the staging slot (same range/exclusion as the running CRC), used to
 * verify a fully-received staged image before promoting it. */
uint32_t calculate_staging_crc32(void) { return calc_crc32(ADDR_FW_STAGING, STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE); }

void wipe_config(void) {
    uint32_t ints = save_and_disable_interrupts();
    flash_range_erase((uint32_t)ADDR_CONFIG - XIP_BASE, FLASH_SECTOR_SIZE);
    restore_interrupts(ints);
}

void write_flash_page(uint32_t addr, uint8_t *buf) {
    bool is_start = (addr & 0xf00) == 0;
    /* The fw-upgrade receiver runs this on Core1. During flash_range_*, XIP is
     * offline, so the OTHER core must not execute from flash or it faults/hangs
     * (this was the auto-sync freeze). Park Core0 via the lockout it armed at
     * boot. Config save runs on Core0 (single page) and is left as-is — locking
     * out the PIO-USB Core1 there risks a deadlock and it has worked in place. */
    bool park_core0 = (get_core_num() == 1);
    if (park_core0) multicore_lockout_start_blocking();
    uint32_t ints = save_and_disable_interrupts();
    if (is_start) flash_range_erase(addr, FLASH_SECTOR_SIZE);
    flash_range_program(addr, buf, FLASH_PAGE_SIZE);
    restore_interrupts(ints);
    if (park_core0) multicore_lockout_end_blocking();
}

/* SRAM-resident copy of the verified staging image onto the running slot.
 *
 * CRITICAL: this overwrites the running image (where normal code lives), so the
 * function AND everything it touches during the copy must execute from SRAM, not
 * flash — hence __not_in_flash_func, an inline word copy (no memcpy, which is in
 * flash), and an inline watchdog reset at the end (no flash-resident reboot call;
 * see the reboot note below for why a watchdog reset, not SCB AIRCR).
 * flash_range_erase/program are SDK __not_in_flash_func, so they are RAM-safe.
 * Runs under flash_safe_execute(), which parks the other core in its RAM-resident
 * lockout handler. Interrupts are disabled for the whole copy so no flash-resident
 * ISR runs while RUNNING is mid-overwrite.
 *
 * Order: erase the whole slot first (boot2 now invalid), program pages 1..N, then
 * program page 0 (boot2) LAST. So an interrupted promote leaves boot2 invalid and
 * the bootrom auto-enters BOOTSEL — "boot2 valid" <=> "promote completed". */
static void __not_in_flash_func(promote_staging_cb)(void *param) {
    (void)param;
    /* Read staging via the cache-bypass alias (0x13...) so we never see stale
     * XIP-cache data for the just-written staging image. */
    const uint32_t src = XIP_NOCACHE_NOALLOC_BASE + ((uint32_t)ADDR_FW_STAGING - XIP_BASE);
    static uint8_t __attribute__((aligned(4))) page[FLASH_PAGE_SIZE];

    uint32_t ints = save_and_disable_interrupts();

    for (uint32_t off = 0; off < STAGING_IMAGE_SIZE; off += FLASH_SECTOR_SIZE)
        flash_range_erase(off, FLASH_SECTOR_SIZE);

    for (uint32_t off = FLASH_PAGE_SIZE; off < STAGING_IMAGE_SIZE; off += FLASH_PAGE_SIZE) {
        const volatile uint32_t *s = (const volatile uint32_t *)(src + off);
        uint32_t *d = (uint32_t *)page;
        for (uint32_t i = 0; i < FLASH_PAGE_SIZE / 4; i++) d[i] = s[i];
        flash_range_program(off, page, FLASH_PAGE_SIZE);
    }
    /* boot2 (page 0) last */
    {
        const volatile uint32_t *s = (const volatile uint32_t *)(src);
        uint32_t *d = (uint32_t *)page;
        for (uint32_t i = 0; i < FLASH_PAGE_SIZE / 4; i++) d[i] = s[i];
        flash_range_program(0, page, FLASH_PAGE_SIZE);
    }

    /* Reboot into the new RUNNING image. Use a WATCHDOG reset, not SCB AIRCR
     * SYSRESETREQ: on RP2040, AIRCR did NOT reboot here — it left the board hung
     * (frozen until a manual power-cycle) because it does not re-run the bootrom
     * the way a watchdog reset does. scratch[4]=0 tells the bootrom to do a normal
     * flash boot; TRIGGER forces an immediate watchdog reset. Inline register
     * writes only (no flash-resident call) so it stays RAM-safe after RUNNING was
     * overwritten. (watchdog_reboot() itself lives in flash and could be at a
     * different address in the just-written image, so it is NOT safe to call here.) */
    /* PSM WDSEL is set only by the SDK's _watchdog_enable(); DH_DEBUG builds
     * never call it, so WDSEL=0 and TRIGGER would reset nothing — the board
     * then spins in the loop below forever (interrupts off, total silence)
     * with RUNNING fully written. Select everything but ROSC/XOSC, exactly as
     * _watchdog_enable does. Direct register store: RAM-safe. */
    psm_hw->wdsel = PSM_WDSEL_BITS & ~(PSM_WDSEL_ROSC_BITS | PSM_WDSEL_XOSC_BITS);
    watchdog_hw->scratch[4] = 0;
    hw_set_bits(&watchdog_hw->ctrl, WATCHDOG_CTRL_TRIGGER_BITS);
    (void)ints;
    for (;;) tight_loop_contents();
}

void promote_staging_to_running(void) {
    /* The copy runs interrupts-off for seconds (can't kick) — disable the
     * watchdog first (harmless if already off, as in DH_DEBUG builds). */
    hw_clear_bits(&watchdog_hw->ctrl, WATCHDOG_CTRL_ENABLE_BITS);
    /* Run the RAM-resident copy with the other core safely parked. Only returns
     * if the other core could not be parked (timeout); RUNNING was never touched,
     * so it is safe to return and let the upgrade re-trigger on the next heartbeat. */
    flash_safe_execute(promote_staging_cb, NULL, 1000);
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

/* Runs on BOTH cores (Rust traceln on Core1 lands here); Core1's stack is
 * only 2KB, so keep this frame small — no second conversion buffer. */
int dh_debug_printf(const char *fmt, ...) {
    va_list a; va_start(a, fmt); char b[256];
    int l = vsnprintf(b, sizeof b, fmt, a);
    va_end(a);
    if (l <= 0) return l;
    /* On truncation vsnprintf NUL-terminates at b[255]; don't push the NUL */
    if (l >= (int)sizeof b) l = (int)sizeof b - 1;
    /* Convert \n to \r\n for CDC terminal compatibility: push the run up to
     * each bare \n, then "\r\n", instead of building a converted copy */
    int start = 0;
    for (int i = 0; i < l; i++) {
        if (b[i] == '\n' && (i == 0 || b[i-1] != '\r')) {
            if (i > start) peer_log_push((const uint8_t *)b + start, (size_t)(i - start));
            peer_log_push((const uint8_t *)"\r\n", 2);
            start = i + 1;
        }
    }
    if (l > start) peer_log_push((const uint8_t *)b + start, (size_t)(l - start));
    return l;
}
#else
int dh_debug_printf(const char *fmt, ...) { return 0; }
#endif

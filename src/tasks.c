/* DeskHop tasks — remaining HAL-bound C task functions.
   Most tasks are now in Rust (hal/ffi/tasks.rs). Only USB stubs,
   firmware upgrade (flash-dependent), and reboot remain in C. */
#include "main.h"

/* USB tasks — TinyUSB inline macros require C.
   Suffixed with _c to avoid collision with Rust wrapper names. */
void usb_device_task_c(void) { tud_task(); }
void usb_host_task_c(void) { if (tuh_inited()) tuh_task(); }

/* Firmware upgrade (flash + queue) — requires direct flash/SDK access */
void firmware_upgrade_task_c(void) {
    if (!global_fw.fw.upgrade_in_progress || !global_fw.fw.byte_done || queue_is_full(queue_from_opaque(&global_hw.uart_tx_queue))) return;
    /* The page-write / terminal / next-request decision is the unit-tested Rust
     * step machine (service::fw_upgrade::next_step) — keeps the off-by-one-prone
     * page/sector/terminal arithmetic out of untestable C. The C side only does
     * the flash/SDK side-effects. */
    fw_step_t s;
    rust_fw_next_step(global_fw.fw.address, &s);
    if (s.write_page)
        write_flash_page((uint32_t)ADDR_FW_RUNNING + s.page_offset - XIP_BASE, global_fw.page_buffer);
    if (s.finalize) {
        global_fw.fw.upgrade_in_progress = 0; global_fw.fw.checksum = ~global_fw.fw.checksum;
        if (calculate_firmware_crc32() != global_fw.fw.checksum) {
            flash_range_erase((uint32_t)ADDR_FW_RUNNING - XIP_BASE, FLASH_SECTOR_SIZE);
            dh_enter_bootloader();
        } else {
            global_fw._running_fw = _firmware_metadata; global_fw.reboot_requested = true;
            /* reboot_requested makes check_system_health stop kicking so a
             * watchdog timeout resets us into the new image — but DH_DEBUG leaves
             * the watchdog disabled at boot, so arm it here or the upgrade never
             * reboots (same gap the config-mode entry path handles). */
            watchdog_enable(WATCHDOG_TIMEOUT, WATCHDOG_PAUSE_ON_DEBUG);
        }
        return;
    }
    request_byte(s.request_address);
}

void request_byte(uint32_t address) {
    uart_packet_t p = { .data32[0] = address, .type = REQUEST_BYTE_MSG };
    /* Only clear byte_done if the REQUEST was actually enqueued. Clearing it
     * before an enqueue that then fails (transient full tx queue — shared with
     * the peer-log/passthrough/heartbeat producers on Core1) would wedge the
     * upgrade: byte_done stuck false, no request out, nothing re-issues it. A
     * failed enqueue is now a harmless no-op retried on the next tick. */
    if (queue_try_add(queue_from_opaque(&global_hw.uart_tx_queue), &p))
        global_fw.fw.byte_done = false;
}

void reboot(void) { *((volatile uint32_t*)(PPB_BASE + 0x0ED0C)) = 0x5FA0004; }

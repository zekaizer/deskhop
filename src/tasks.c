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
    if (!global_fw.fw.upgrade_in_progress) return;
    if (!global_fw.fw.byte_done) {
        /* Waiting on a ResponseByte. The transfer is strictly lock-step, so one
         * lost packet (e.g. a bad-checksum drop) would otherwise wedge it
         * forever — nothing re-requests, and heartbeats are silenced while
         * upgrade_in_progress. The Rust stall tracker re-requests after a
         * timeout and aborts after repeated failures so the heartbeat path can
         * restart the sync. */
        if (rust_fw_stall_tick())
            request_byte(global_fw.fw.address);
        return;
    }
    if (queue_is_full(queue_from_opaque(&global_hw.uart_tx_queue))) return;
    /* The page-write / terminal / next-request decision is the unit-tested Rust
     * step machine (service::fw_upgrade::next_step) — keeps the off-by-one-prone
     * page/sector/terminal arithmetic out of untestable C. The C side only does
     * the flash/SDK side-effects. */
    fw_step_t s;
    rust_fw_next_step(global_fw.fw.address, &s);
    if (s.write_page)
        /* Write to STAGING, never the running image: RUNNING stays intact and
         * bootable for the whole transfer. */
        write_flash_page((uint32_t)ADDR_FW_STAGING + s.page_offset - XIP_BASE, global_fw.page_buffer);
    if (s.finalize) {
        global_fw.fw.upgrade_in_progress = 0; global_fw.fw.checksum = ~global_fw.fw.checksum;
        /* Verify the fully-received STAGING image, then promote it to RUNNING.
         * Because RUNNING was never touched, a bad/aborted/interrupted transfer
         * needs no recovery — just clear the upgrade and the peer re-triggers on
         * the next heartbeat (no in-place corruption, no forced bootloader). */
        uint32_t calc = calculate_staging_crc32();
        if (calc == global_fw.fw.checksum) {
            dh_debug_printf("fw sync crc ok crc=0x%08lx -> promote\n", (unsigned long)global_fw.fw.checksum);
            promote_staging_to_running(); /* SRAM copy + reset; returns only on lockout failure */
            /* Reached only if the other core could not be parked: RUNNING is
             * intact, so the peer re-triggers the sync on the next heartbeat. */
            dh_debug_printf("fw sync promote FAILED (core lockout) -> retry next hb\n");
        } else {
            /* Mismatch leaves RUNNING untouched and silently re-downloads forever
             * without this line — name the bad CRC so the sync loop is visible. */
            dh_debug_printf("fw sync crc FAIL want=0x%08lx got=0x%08lx -> retry next hb\n",
                            (unsigned long)global_fw.fw.checksum, (unsigned long)calc);
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

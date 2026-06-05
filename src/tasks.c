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
    if (global_fw.fw.address > STAGING_IMAGE_SIZE) {
        global_fw.fw.upgrade_in_progress = 0; global_fw.fw.checksum = ~global_fw.fw.checksum;
        if (calculate_firmware_crc32() != global_fw.fw.checksum) {
            flash_range_erase((uint32_t)ADDR_FW_RUNNING - XIP_BASE, FLASH_SECTOR_SIZE);
            reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
        } else { global_fw._running_fw = _firmware_metadata; global_fw.reboot_requested = true; }
    }
    if (TU_U32_BYTE0(global_fw.fw.address) == 0x00)
        write_flash_page((uint32_t)ADDR_FW_RUNNING + ((global_fw.fw.address-1) & 0xFFFFFF00) - XIP_BASE, global_fw.page_buffer);
    request_byte(global_fw.fw.address);
}

void request_byte(uint32_t address) {
    uart_packet_t p = { .data32[0] = address, .type = REQUEST_BYTE_MSG };
    global_fw.fw.byte_done = false;
    queue_try_add(queue_from_opaque(&global_hw.uart_tx_queue), &p);
}

void reboot(void) { *((volatile uint32_t*)(PPB_BASE + 0x0ED0C)) = 0x5FA0004; }

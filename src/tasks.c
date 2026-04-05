/* DeskHop tasks — remaining HAL-bound C task functions.
   Most tasks are now in Rust (hal/ffi/tasks.rs). Only USB stubs,
   firmware upgrade (flash-dependent), and reboot remain in C. */
#include "main.h"

/* USB tasks — TinyUSB inline macros require C */
void usb_device_task(device_t *s) { tud_task(); }
void usb_host_task(device_t *s) { if (tuh_inited()) tuh_task(); }

/* heartbeat_output_task — now fully in Rust (#[export_name]) */

/* Firmware upgrade (flash + queue) — requires direct flash/SDK access */
void firmware_upgrade_task(device_t *s) {
    if (!s->fw.upgrade_in_progress || !s->fw.byte_done || queue_is_full(queue_from_opaque(&s->uart_tx_queue))) return;
    if (s->fw.address > STAGING_IMAGE_SIZE) {
        s->fw.upgrade_in_progress = 0; s->fw.checksum = ~s->fw.checksum;
        if (calculate_firmware_crc32() != s->fw.checksum) {
            flash_range_erase((uint32_t)ADDR_FW_RUNNING - XIP_BASE, FLASH_SECTOR_SIZE);
            reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
        } else { s->_running_fw = _firmware_metadata; global_state.reboot_requested = true; }
    }
    if (TU_U32_BYTE0(s->fw.address) == 0x00)
        write_flash_page((uint32_t)ADDR_FW_RUNNING + ((s->fw.address-1) & 0xFFFFFF00) - XIP_BASE, s->page_buffer);
    request_byte(s, s->fw.address);
}

void request_byte(device_t *state, uint32_t address) {
    uart_packet_t p = { .data32[0] = address, .type = REQUEST_BYTE_MSG };
    state->fw.byte_done = false;
    queue_try_add(queue_from_opaque(&global_state.uart_tx_queue), &p);
}

void reboot(void) { *((volatile uint32_t*)(PPB_BASE + 0x0ED0C)) = 0x5FA0004; }

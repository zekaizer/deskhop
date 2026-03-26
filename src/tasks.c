/* DeskHop tasks — HAL-bound task functions + DMA buffer ops. */
#include "main.h"

extern void rust_heartbeat_output_task(device_t *);

/* USB tasks */
void usb_device_task(device_t *s) { tud_task(); }
void usb_host_task(device_t *s) { if (tuh_inited()) tuh_task(); }
void heartbeat_output_task(device_t *s) {
    rust_heartbeat_output_task(s);
#ifdef DH_DEBUG
    if (is_bootsel_pressed()) reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
#endif
}

/* process_hid_queue_task — now in Rust (service::tasks::process_hid_queue) */

/* Firmware upgrade (flash + queue) */
void firmware_upgrade_task(device_t *s) {
    if (!s->fw.upgrade_in_progress || !s->fw.byte_done || queue_is_full(&s->uart_tx_queue)) return;
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

/* DMA buffer operations + packet receiver */
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

void request_byte(device_t *state, uint32_t address) {
    uart_packet_t p = { .data32[0] = address, .type = REQUEST_BYTE_MSG };
    state->fw.byte_done = false;
    queue_try_add(&global_state.uart_tx_queue, &p);
}

void reboot(void) { *((volatile uint32_t*)(PPB_BASE + 0x0ED0C)) = 0x5FA0004; }

void packet_receiver_task(device_t *s) {
    uint32_t cp = (uint32_t)DMA_RX_BUFFER_SIZE - dma_channel_hw_addr(s->dma_rx_channel)->transfer_count;
    uint32_t d = get_ptr_delta(cp, s);
    while (d >= RAW_PACKET_LENGTH) {
        if (is_start_of_packet(s)) { fetch_packet(s); process_packet(&s->in_packet, s); return; }
        s->dma_ptr = NEXT_RING_IDX(s->dma_ptr); d--;
    }
}

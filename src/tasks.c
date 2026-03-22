/* DeskHop tasks — scheduling in Rust, HAL tasks here. */
#include "main.h"

extern void rust_kick_watchdog_task(device_t *), rust_screensaver_task(device_t *),
    rust_heartbeat_output_task(device_t *);

void kick_watchdog_task(device_t *s) { rust_kick_watchdog_task(s); }
void usb_device_task(device_t *s) { tud_task(); }
void usb_host_task(device_t *s) { if (tuh_inited()) tuh_task(); }
void screensaver_task(device_t *s) { rust_screensaver_task(s); }
void heartbeat_output_task(device_t *s) {
    rust_heartbeat_output_task(s);
#ifdef DH_DEBUG
    if (is_bootsel_pressed()) reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
#endif
}

/* HAL: queue + TinyUSB */
void process_hid_queue_task(device_t *s) {
    hid_generic_pkt_t p;
    if (!queue_try_peek(&s->hid_queue_out, &p) || !tud_hid_n_ready(p.instance)) return;
    if (tud_hid_n_report(p.instance, p.report_id, p.data, p.len))
        queue_try_remove(&s->hid_queue_out, &p);
}

/* HAL: flash + queue */
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

/* HAL: DMA + UART */
void packet_receiver_task(device_t *s) {
    uint32_t cp = (uint32_t)DMA_RX_BUFFER_SIZE - dma_channel_hw_addr(s->dma_rx_channel)->transfer_count;
    uint32_t d = get_ptr_delta(cp, s);
    while (d >= RAW_PACKET_LENGTH) {
        if (is_start_of_packet(s)) { fetch_packet(s); process_packet(&s->in_packet, s); return; }
        s->dma_ptr = NEXT_RING_IDX(s->dma_ptr); d--;
    }
}

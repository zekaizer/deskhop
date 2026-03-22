/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * Task scheduling is in Rust (src-rust/src/hal/scheduler.rs).
 * This file contains HAL-dependent task functions only.
 */

#include "main.h"

/* HAL-dependent: watchdog hardware access */
void kick_watchdog_task(device_t *state) {
    uint64_t core1_last_loop_pass = state->core1_last_loop_pass;
    uint64_t current_time = time_us_64();
    if (state->reboot_requested) return;
    if (current_time - core1_last_loop_pass < CORE1_HANG_TIMEOUT_US)
        watchdog_update();
}

/* HAL-dependent: TinyUSB task dispatch */
void usb_device_task(device_t *state) { tud_task(); }
void usb_host_task(device_t *state) { if (tuh_inited()) tuh_task(); }

/* Rust-implemented task functions */
extern void rust_screensaver_task(device_t *dev);
extern void rust_heartbeat_output_task(device_t *dev);

void screensaver_task(device_t *state) { rust_screensaver_task(state); }

void heartbeat_output_task(device_t *state) {
    rust_heartbeat_output_task(state);
#ifdef DH_DEBUG
    if (is_bootsel_pressed()) reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
#endif
}

/* HAL-dependent: queue_t + TinyUSB */
void process_hid_queue_task(device_t *state) {
    hid_generic_pkt_t packet;
    if (!queue_try_peek(&state->hid_queue_out, &packet)) return;
    if (!tud_hid_n_ready(packet.instance)) return;
    bool ok = tud_hid_n_report(packet.instance, packet.report_id, packet.data, packet.len);
    if (ok) queue_try_remove(&state->hid_queue_out, &packet);
}

/* HAL-dependent: flash + queue + DMA */
void firmware_upgrade_task(device_t *state) {
    if (!state->fw.upgrade_in_progress || !state->fw.byte_done) return;
    if (queue_is_full(&state->uart_tx_queue)) return;

    if (state->fw.address > STAGING_IMAGE_SIZE) {
        state->fw.upgrade_in_progress = 0;
        state->fw.checksum = ~state->fw.checksum;
        if (calculate_firmware_crc32() != state->fw.checksum) {
            flash_range_erase((uint32_t)ADDR_FW_RUNNING - XIP_BASE, FLASH_SECTOR_SIZE);
            reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
        } else {
            state->_running_fw = _firmware_metadata;
            global_state.reboot_requested = true;
        }
    }

    if (TU_U32_BYTE0(state->fw.address) == 0x00) {
        uint32_t page_start_addr = (state->fw.address - 1) & 0xFFFFFF00;
        write_flash_page((uint32_t)ADDR_FW_RUNNING + page_start_addr - XIP_BASE, state->page_buffer);
    }

    request_byte(state, state->fw.address);
}

/* HAL-dependent: DMA + UART */
void packet_receiver_task(device_t *state) {
    uint32_t current_pointer =
        (uint32_t)DMA_RX_BUFFER_SIZE - dma_channel_hw_addr(state->dma_rx_channel)->transfer_count;
    uint32_t delta = get_ptr_delta(current_pointer, state);

    while (delta >= RAW_PACKET_LENGTH) {
        if (is_start_of_packet(state)) {
            fetch_packet(state);
            process_packet(&state->in_packet, state);
            return;
        }
        state->dma_ptr = NEXT_RING_IDX(state->dma_ptr);
        delta--;
    }
}

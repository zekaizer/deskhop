/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, version 3.
 *
 * See the file LICENSE for the full license text.
 */

#include "main.h"

void task_scheduler(device_t *state, task_t *task) {
    uint64_t current_time = time_us_64();

    if (current_time < task->next_run)
        return;

    task->next_run = current_time + task->frequency;
    task->exec(state);
}

/* ================================================== *
 * ==============  Watchdog Functions  ============== *
 * ================================================== */

void kick_watchdog_task(device_t *state) {
    /* Read the timer AFTER duplicating the core1 timestamp,
       so it doesn't get updated in the meantime. */
    uint64_t core1_last_loop_pass = state->core1_last_loop_pass;
    uint64_t current_time         = time_us_64();

    /* If a reboot is requested, we'll stop updating watchdog */
    if (state->reboot_requested)
        return;

    /* If core1 stops updating the timestamp, we'll stop kicking the watchog and reboot */
    if (current_time - core1_last_loop_pass < CORE1_HANG_TIMEOUT_US)
        watchdog_update();
}

/* ================================================== *
 * ===============  USB Device / Host  ============== *
 * ================================================== */

void usb_device_task(device_t *state) {
    tud_task();
}

/* Check if passthrough needs activation and handle re-enumeration sequence.
 * Runs on core0 since tud_disconnect/tud_connect are device-side operations. */
void passthrough_task(device_t *state) {
    /* BOOTSEL switch: flag set by core1, safe to execute on core0 */
    if (state->bootsel_switch_requested) {
        state->bootsel_switch_requested = false;
        dh_debug_printf("[BTN] BOOTSEL → output %s\n",
                        BOARD_ROLE == OUTPUT_A ? "A" : "B");
        set_active_output(state, BOARD_ROLE);
    }

    passthrough_state_t *pt = passthrough_get_state();

    /* Phase 1: Activate after captures stabilize (500ms since last capture) */
    if (!pt->active && pt->iface_count > 0 && pt->last_capture_us > 0) {
        if (time_us_64() - pt->last_capture_us > _MS(500)) {
            dh_debug_printf("[PT] Activating passthrough (%d ifaces)\n", pt->iface_count);
            if (passthrough_activate(pt)) {
                dh_debug_printf("[PT] Disconnect for re-enumeration\n");
                tud_disconnect();
                pt->reconnect_at_us = time_us_64() + _MS(200);
            } else {
                dh_debug_printf("[PT] Activation failed\n");
            }
        }
    }

    /* Phase 2: Reconnect after 200ms disconnect delay (USB spec minimum) */
    if (pt->reconnect_at_us > 0 && time_us_64() >= pt->reconnect_at_us) {
        dh_debug_printf("[PT] Reconnect with new descriptors\n");
        tud_connect();
        pt->reconnect_at_us = 0;
    }

    /* Phase 2.5: IRoot feature discovery (P3) — runs after activation,
     * uses out_queue to send IRoot queries one at a time. */
    if (pt->active && !pt->hidpp_disc.done) {
        /* Start discovery after activation */
        if (pt->hidpp_disc.state == DISC_IDLE)
            pt->hidpp_disc.state = DISC_DETECT_DEVICE;
        passthrough_discovery_step(pt);
        passthrough_scan_step(pt);
    }

    /* Phase 3: Send HID++ output via raw control transfer.
     * Logitech receivers expect full report (rid + data) in the DATA phase,
     * with rid also in wValue — matching Linux hid-logitech-hidpp behavior. */
    if (pt->out_queue.pending) {
        uint8_t rid  = pt->out_queue.report_id;
        uint8_t type = pt->out_queue.report_type;
        uint16_t len = pt->out_queue.len;

        /* Build full report: report_id + payload */
        static uint8_t buf[33];
        buf[0] = rid;
        memcpy(buf + 1, pt->out_queue.data, len);
        uint16_t full_len = len + 1;

        /* Host-side instance number matches USB bInterfaceNumber */
        uint8_t itf_num = pt->out_queue.instance;

        static tusb_control_request_t request;
        request = (tusb_control_request_t){
            .bmRequestType_bit = {
                .recipient = TUSB_REQ_RCPT_INTERFACE,
                .type      = TUSB_REQ_TYPE_CLASS,
                .direction = TUSB_DIR_OUT
            },
            .bRequest = HID_REQ_CONTROL_SET_REPORT,
            .wValue   = tu_htole16((uint16_t)((type << 8) | rid)),
            .wIndex   = tu_htole16((uint16_t)itf_num),
            .wLength  = tu_htole16(full_len)
        };

        tuh_xfer_t xfer = {
            .daddr       = pt->out_queue.dev_addr,
            .ep_addr     = 0,
            .setup       = &request,
            .buffer      = buf,
            .complete_cb = NULL,
            .user_data   = 0
        };

        bool ok = tuh_control_xfer(&xfer);
        dh_debug_printf("[PT] OUT raw xfer itf=%d rid=0x%02X wLen=%d ok=%d\n",
                        itf_num, rid, full_len, ok);
        pt->out_queue.pending = false;
    }
}

void usb_host_task(device_t *state) {
    if (tuh_inited())
        tuh_task();
}

mouse_report_t *screensaver_pong(device_t *state) {
    static mouse_report_t report = {0};
    static int dx = 20, dy = 25;

    /* Check if we are bouncing off the walls and reverse direction in that case. */
    if (report.x + dx < MIN_SCREEN_COORD || report.x + dx > MAX_SCREEN_COORD)
        dx = -dx;

    if (report.y + dy < MIN_SCREEN_COORD || report.y + dy > MAX_SCREEN_COORD)
        dy = -dy;

    report.x += dx;
    report.y += dy;

    return &report;
}

mouse_report_t *screensaver_jitter(device_t *state) {
    static mouse_report_t report = {
        .y = JITTER_DISTANCE,
        .mode = RELATIVE,
    };
    report.y = -report.y;

    return &report;
}

/* Have something fun and entertaining when idle. */
void screensaver_task(device_t *state) {
    const uint32_t delays[] = {
        0,        /* DISABLED, unused index 0 */
        5000,     /* PONG, move mouse every 5 ms for a high framerate */
        10000000, /* JITTER, once every 10 sec is more than enough */
    };
    static int last_pointer_move = 0;
    screensaver_t *screensaver = &state->config.output[BOARD_ROLE].screensaver;
    uint64_t inactivity_period = time_us_64() - state->last_activity[BOARD_ROLE];

    /* If we're not enabled, nothing to do here. */
    if (screensaver->mode == DISABLED)
        return;

    /* System is still not idle for long enough to activate or screensaver mode is not supported */
    if (inactivity_period < screensaver->idle_time_us || screensaver->mode > MAX_SS_VAL)
        return;

    /* We exceeded the maximum permitted screensaver runtime */
    if (screensaver->max_time_us
        && inactivity_period > (screensaver->max_time_us + screensaver->idle_time_us))
        return;

    /* If we're the selected output and we can only run on inactive output, nothing to do here. */
    if (screensaver->only_if_inactive && CURRENT_BOARD_IS_ACTIVE_OUTPUT)
        return;

    /* We're active! Now check if it's time to move the cursor yet. */
    if (time_us_32() - last_pointer_move < delays[screensaver->mode])
        return;

    /* Return, if we're not connected or the host is suspended */
    if(!tud_ready()) {
        return;
    }

    mouse_report_t *report;
    switch (screensaver->mode) {
        case PONG:
            report = screensaver_pong(state);
            break;

        case JITTER:
            report = screensaver_jitter(state);
            break;

        default:
            return;
    }

    /* Move mouse pointer */
    queue_mouse_report(report, state);

    /* Update timer of the last pointer move */
    last_pointer_move = time_us_32();
}

/* Periodically emit heartbeat packets */
void heartbeat_output_task(device_t *state) {
    /* If firmware upgrade is in progress, don't touch flash_cs */
    if (state->fw.upgrade_in_progress)
        return;

    if (state->config_mode_active) {
        /* Leave config mode if timeout expired and user didn't click exit */
        if (time_us_64() > state->config_mode_timer)
            reboot();

        /* Keep notifying the user we're still in config mode */
        blink_led(state);
    }

#ifdef DH_DEBUG
    /* BOOTSEL: short press sets flag for core0, long hold = bootsel reset */
    {
        static uint32_t press_start = 0;
        if (is_bootsel_pressed()) {
            if (press_start == 0)
                press_start = time_us_32();
            else if ((time_us_32() - press_start) > 2000000)
                reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
        } else if (press_start != 0) {
            state->bootsel_switch_requested = true;
            press_start = 0;
        }
    }
#endif

    uart_packet_t packet = {
        .type = HEARTBEAT_MSG,
        .data16 = {
            [0] = state->_running_fw.version,
            [2] = state->active_output,
        },
    };

    queue_try_add(&global_state.uart_tx_queue, &packet);
}


/* Process other outgoing hid report messages. */
void process_hid_queue_task(device_t *state) {
    hid_generic_pkt_t packet;

    if (!queue_try_peek(&state->hid_queue_out, &packet))
        return;

    if (!tud_hid_n_ready(packet.instance))
        return;

    /* ... try sending it to the host, if it's successful */
    bool succeeded = tud_hid_n_report(packet.instance, packet.report_id, packet.data, packet.len);

    /* ... then we can remove it from the queue. Race conditions shouldn't happen [tm] */
    if (succeeded)
        queue_try_remove(&state->hid_queue_out, &packet);
}

/* Task that handles copying firmware from the other device to ours */
void firmware_upgrade_task(device_t *state) {
    if (!state->fw.upgrade_in_progress || !state->fw.byte_done)
        return;

    if (queue_is_full(&state->uart_tx_queue))
        return;

    /* End condition, when reached the process is completed. */
    if (state->fw.address > STAGING_IMAGE_SIZE) {
        state->fw.upgrade_in_progress = 0;
        state->fw.checksum = ~state->fw.checksum;

        /* Checksum mismatch, we wipe the stage 2 bootloader and rely on ROM recovery */
        if(calculate_firmware_crc32() != state->fw.checksum) {
            flash_range_erase((uint32_t)ADDR_FW_RUNNING - XIP_BASE, FLASH_SECTOR_SIZE);
            reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
        }

        else {
            state->_running_fw = _firmware_metadata;
            global_state.reboot_requested = true;
        }
    }

    /* If we're on the last element of the current page, page is done - write it. */
    if (TU_U32_BYTE0(state->fw.address) == 0x00) {

        uint32_t page_start_addr = (state->fw.address - 1) & 0xFFFFFF00;
        write_flash_page((uint32_t)ADDR_FW_RUNNING + page_start_addr - XIP_BASE, state->page_buffer);
    }

    request_byte(state, state->fw.address);
}

void packet_receiver_task(device_t *state) {
    uint32_t current_pointer
        = (uint32_t)DMA_RX_BUFFER_SIZE - dma_channel_hw_addr(state->dma_rx_channel)->transfer_count;
    uint32_t delta = get_ptr_delta(current_pointer, state);

    /* If we don't have enough characters for a packet, skip loop and return immediately */
    while (delta >= RAW_PACKET_LENGTH) {
        if (is_start_of_packet(state)) {
            fetch_packet(state);
            process_packet(&state->in_packet, state);
            return;
        }

        /* No packet found, advance to next position and decrement delta */
        state->dma_ptr = NEXT_RING_IDX(state->dma_ptr);
        delta--;
    }
}

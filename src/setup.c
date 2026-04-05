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

/* ================================================== *
 * =============  Initial Board Setup  ============== *
 * ================================================== */

#include "main.h"

/* ================================================== *
 * Perform initial UART setup
 * ================================================== */

void serial_init(uint8_t tx_pin, uint8_t rx_pin) {
    uart_init(SERIAL_UART, SERIAL_BAUDRATE);
    uart_set_hw_flow(SERIAL_UART, false, false);
    uart_set_format(SERIAL_UART, SERIAL_DATA_BITS, SERIAL_STOP_BITS, SERIAL_PARITY);
    uart_set_translate_crlf(SERIAL_UART, false);
    uart_set_fifo_enabled(SERIAL_UART, true);
    gpio_set_function(tx_pin, GPIO_FUNC_UART);
    gpio_set_function(rx_pin, GPIO_FUNC_UART);
}

/* ================================================== *
 * PIO USB configuration, D+ pin 14, D- pin 15
 * ================================================== */

void pio_usb_host_config(uint8_t board_role) {
    /* tuh_configure() must be called before tuh_init() */
    static pio_usb_configuration_t config = PIO_USB_DEFAULT_CONFIG;
    config.pin_dp                         = PIO_USB_DP_PIN_DEFAULT;

    /* Board B is always report mode, board A is default-boot if configured */
    if (board_role == OUTPUT_B || ENFORCE_KEYBOARD_BOOT_PROTOCOL == 0)
        tuh_hid_set_default_protocol(HID_PROTOCOL_REPORT);

    tuh_configure(BOARD_TUH_RHPORT, TUH_CFGID_RPI_PIO_USB_CONFIGURATION, &config);

    /* Initialize and configure TinyUSB Host */
    tuh_init(1);
}

/* ================================================== *
 * Board Autoprobe Routine
 * ================================================== */

/* Probing algorithm logic:
    - RX pin is driven by the digital isolator IC
    - IF we are board A, it will be connected to pin 13
      and it will drive it either high or low at any given time
    - Before uart setup, enable it as an input
    - Go through a probing sequence of 8 values and pull either up or down
      to that value
    - Read out the value on the RX pin
    - If the entire sequence of values match, we are definitely floating
      so IC is not connected on BOARD_A_RX, and we're BOARD B
*/
int board_autoprobe(void) {
    const bool probing_sequence[] = {true, false, false, true, true, false, true, false};
    const int seq_len = ARRAY_SIZE(probing_sequence);

    /* Set the pin as INPUT and initialize it */
    gpio_init(BOARD_A_RX);
    gpio_set_dir(BOARD_A_RX, GPIO_IN);

    for (int i=0; i<seq_len; i++) {
        if (probing_sequence[i])
            gpio_pull_up(BOARD_A_RX);
        else
            gpio_pull_down(BOARD_A_RX);

        /* Wait for value to settle */
        sleep_ms(3);

        /* Read the value */
        bool value = gpio_get(BOARD_A_RX);
        gpio_disable_pulls(BOARD_A_RX);

        /* If values mismatch at any point, means IC is connected and we're board A */
        if (probing_sequence[i] != value)
            return OUTPUT_A;
    }

    /* If it was just reading the pull up/down in all cases, pin is floating and we're board B */
    return OUTPUT_B;
}


/* ================================================== *
 * Check if we should boot in configuration mode or not
 * ================================================== */

bool is_config_mode_active(void) {
    /* Watchdog registers survive reboot (RP2040 datasheet section 2.8.1.1) */
    bool is_active = (watchdog_hw->scratch[5] == MAGIC_WORD_1 &&
                      watchdog_hw->scratch[6] == MAGIC_WORD_2);

    /* Remove, so next reboot it's no longer active */
    if (is_active)
        watchdog_hw->scratch[5] = 0;

    reset_config_timer();

    return is_active;
}


/* ================================================== *
 * Configure DMA for reliable UART transfers
 * ================================================== */
const uint8_t* uart_buffer_pointers[1] = {uart_rxbuf};
uint8_t uart_rxbuf[DMA_RX_BUFFER_SIZE] __attribute__((aligned(DMA_RX_BUFFER_SIZE))) ;
uint8_t uart_txbuf[DMA_TX_BUFFER_SIZE] __attribute__((aligned(DMA_TX_BUFFER_SIZE))) ;

static void configure_tx_dma(void) {
    global_hw.dma_tx_channel = dma_claim_unused_channel(true);

    dma_channel_config tx_config = dma_channel_get_default_config(global_hw.dma_tx_channel);
    channel_config_set_transfer_data_size(&tx_config, DMA_SIZE_8);

    /* Writing uart (always write the same address, but source addr changes as we read) */
    channel_config_set_read_increment(&tx_config, true);
    channel_config_set_write_increment(&tx_config, false);

    // channel_config_set_ring(&tx_config, false, 4);
    channel_config_set_dreq(&tx_config, DREQ_UART0_TX);

    /* Configure, but don't start immediately. We'll do this each time the outgoing
       packet is ready and we copy it to the buffer */
    dma_channel_configure(
        global_hw.dma_tx_channel,
        &tx_config,
        &uart0_hw->dr,
        uart_txbuf,
        0,
        false
    );
}

static void configure_rx_dma(void) {
    /* Find an empty channel, store it for later reference */
    global_hw.dma_rx_channel = dma_claim_unused_channel(true);
    global_hw.dma_control_channel = dma_claim_unused_channel(true);

    dma_channel_config config = dma_channel_get_default_config(global_hw.dma_rx_channel);
    dma_channel_config control_config = dma_channel_get_default_config(global_hw.dma_control_channel);

    channel_config_set_transfer_data_size(&config, DMA_SIZE_8);
    channel_config_set_transfer_data_size(&control_config, DMA_SIZE_32);

    // The read address is the address of the UART data register which is constant
    channel_config_set_read_increment(&config, false);
    channel_config_set_read_increment(&control_config, false);

    // Read into a ringbuffer with 1024 (2^10) elements
    channel_config_set_write_increment(&config, true);
    channel_config_set_write_increment(&control_config, false);

    channel_config_set_ring(&config, true, 10);

    // The UART signals when data is avaliable
    channel_config_set_dreq(&config, DREQ_UART0_RX);

    channel_config_set_chain_to(&config, global_hw.dma_control_channel);

    dma_channel_configure(
        global_hw.dma_rx_channel,
        &config,
        uart_rxbuf,
        &uart0_hw->dr,
        DMA_RX_BUFFER_SIZE,
        false);

    dma_channel_configure(
        global_hw.dma_control_channel,
        &control_config,
        &dma_hw->ch[global_hw.dma_rx_channel].al2_write_addr_trig,
        uart_buffer_pointers,
        1,
        false);

    dma_channel_start(global_hw.dma_control_channel);
}


/* ================================================== *
 * Perform initial board/usb setup
 * ================================================== */
int board;

void initial_setup(void) {
    /* PIO USB requires a clock multiple of 12 MHz, setting to 120 MHz */
    set_sys_clock_khz(120000, true);

    /* Search the persistent storage sector in flash for valid config or use defaults */
    load_config();

    /* Init and enable the on-board LED GPIO as output */
    gpio_init(GPIO_LED_PIN);
    gpio_set_dir(GPIO_LED_PIN, GPIO_OUT);

    /* Detect which board we're running on and check config mode */
    bool config_mode = is_config_mode_active();
    uint8_t role = board_autoprobe();

    /* Initialize and configure UART (pins depend on board role) */
    uint8_t tx_pin = (role == OUTPUT_A) ? BOARD_A_TX : BOARD_B_TX;
    uint8_t rx_pin = (role == OUTPUT_A) ? BOARD_A_RX : BOARD_B_RX;
    serial_init(tx_pin, rx_pin);

    /* Initialize keyboard and mouse queues */
    queue_init(queue_from_opaque(&global_hw.kbd_queue), sizeof(hid_kbd_report_t), KBD_QUEUE_LENGTH);
    queue_init(queue_from_opaque(&global_hw.mouse_queue), sizeof(mouse_report_t), MOUSE_QUEUE_LENGTH);

    /* Initialize generic HID packet queue */
    queue_init(queue_from_opaque(&global_hw.hid_queue_out), sizeof(hid_generic_pkt_t), HID_QUEUE_LENGTH);

    /* Initialize UART queue */
    queue_init(queue_from_opaque(&global_hw.uart_tx_queue), sizeof(uart_packet_t), UART_QUEUE_LENGTH);

    /* Store probed values into Rust-owned global_cfg BEFORE launching core1,
       so core1's Rust loop sees correct board_role and config_mode_active. */
    extern void rust_init_config(bool config_mode_active, uint8_t board_role, uint64_t timestamp);
    rust_init_config(config_mode, role, time_us_64());

    /* Setup RP2040 Core 1 */
    multicore_reset_core1();
    multicore_launch_core1(core1_main);

    /* Initialize and configure TinyUSB Device */
    tud_init(BOARD_TUD_RHPORT);

    /* Initialize and configure TinyUSB Host */
    pio_usb_host_config(role);

    /* Initialize and configure DMA */
    configure_tx_dma();
    configure_rx_dma();

    /* Load the current firmware info */
    global_fw._running_fw = _firmware_metadata;

    /* Setup the watchdog so we reboot and recover from a crash.
       Disabled in debug builds to allow diagnostic LED blinks and CDC output
       without triggering reboot. */
#ifndef DH_DEBUG
    watchdog_enable(WATCHDOG_TIMEOUT, WATCHDOG_PAUSE_ON_DEBUG);
#endif
}

/* ==========  End of Initial Board Setup  ========== */

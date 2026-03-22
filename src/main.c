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

/* Rust FFI — takes device_t* and never returns (runs core0 main loop) */
extern void rust_main_loop(device_t *state) __attribute__((noreturn));

/*********  Global Variables  **********/
device_t global_state     = {0};
device_t *device          = &global_state;

firmware_metadata_t _firmware_metadata __attribute__((section(".section_metadata"))) = {
    .version = 0x0001,
};

/* ================================================== *
 * ==============  Main Program Loops  ============== *
 * ================================================== */

int main(void) {
    // Wait for the board to settle
    sleep_ms(10);

    // Initial board setup
    initial_setup(device);

    // Initial state, A is the default output
    set_active_output(device, OUTPUT_A);

    // Rust owns the core0 main loop — never returns
    rust_main_loop(device);
}

void core1_main() {
    static task_t tasks_core1[] = {
        [0] = {.exec = &usb_host_task,           .frequency = _TOP()},       // .-> USB host task, needs to run as often as possible
        [1] = {.exec = &packet_receiver_task,    .frequency = _TOP()},       // | Receive data over serial from the other board
        [2] = {.exec = &led_blinking_task,       .frequency = _HZ(30)},      // | Check if LED needs blinking
        [3] = {.exec = &screensaver_task,        .frequency = _HZ(120)},     // | Handle "screensaver" movements
        [4] = {.exec = &firmware_upgrade_task,   .frequency = _HZ(4000)},    // | Send firmware to the other board if needed
        [5] = {.exec = &heartbeat_output_task,   .frequency = _HZ(1)},       // | Output periodic heartbeats
    };                                                                       // `----- then go back and repeat forever
    const int NUM_TASKS = ARRAY_SIZE(tasks_core1);

    while (true) {
        // Update the timestamp, so core0 can figure out if we're dead
        device->core1_last_loop_pass = time_us_64();

        for (int i = 0; i < NUM_TASKS; i++)
            task_scheduler(device, &tasks_core1[i]);
    }
}
/* =======  End of Main Program Loops  ======= */

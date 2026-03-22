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

/* Rust FFI — never-returning main loops for both cores */
extern void rust_main_loop(device_t *state) __attribute__((noreturn));
extern void rust_core1_loop(device_t *state) __attribute__((noreturn));

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
    rust_core1_loop(device);
}
/* =======  End of Main Program Loops  ======= */

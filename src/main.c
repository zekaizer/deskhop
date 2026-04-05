/* DeskHop entry — both core loops in Rust. */
#include "main.h"

extern void rust_main_loop(void) __attribute__((noreturn));
extern void rust_core1_loop(void) __attribute__((noreturn));

device_hid_t    global_hid = {0};
/* global_cfg owned by Rust — see src-rust/src/domain/structs.rs */
device_fw_t     global_fw  = {0};
device_led_t    global_led = {0};
device_hw_t     global_hw  = {0};
firmware_metadata_t _firmware_metadata __attribute__((section(".section_metadata"))) = { .version = 0x0001 };

int main(void) {
    sleep_ms(10);
    initial_setup();

    /* Layout verification is now compile-time (build.rs + bindgen) */

    set_active_output(OUTPUT_A);
    rust_main_loop();
}

void core1_main() { rust_core1_loop(); }

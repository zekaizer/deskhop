/* DeskHop entry — both core loops in Rust. */
#include "main.h"

extern void rust_main_loop(device_t *) __attribute__((noreturn));
extern void rust_core1_loop(device_t *) __attribute__((noreturn));

device_t global_state = {0};
device_t *device = &global_state;
firmware_metadata_t _firmware_metadata __attribute__((section(".section_metadata"))) = { .version = 0x0001 };

extern bool hal_verify_device_layout(void);
extern void hal_debug_blink(int count, int delay_ms);

int main(void) {
    sleep_ms(10);
    initial_setup(device);

    /* Verify Rust Device struct matches C device_t layout */
    if (!hal_verify_device_layout()) {
        /* Layout mismatch — rapid blink forever (10 fast blinks) */
        while (1) hal_debug_blink(10, 50);
    }

    set_active_output(device, OUTPUT_A);
    rust_main_loop(device);
}

void core1_main() { rust_core1_loop(device); }

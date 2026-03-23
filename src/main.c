/* DeskHop entry — both core loops in Rust. */
#include "main.h"

extern void rust_main_loop(device_t *) __attribute__((noreturn));
extern void rust_core1_loop(device_t *) __attribute__((noreturn));

device_t global_state = {0};
device_t *device = &global_state;
firmware_metadata_t _firmware_metadata __attribute__((section(".section_metadata"))) = { .version = 0x0001 };

extern int hal_verify_device_layout(void);
extern void hal_debug_blink(int count, int delay_ms);
extern void hal_dump_layout(void);

int main(void) {
    sleep_ms(10);
    initial_setup(device);

    /* Verify Rust Device struct matches C device_t layout */
    int layout_err = hal_verify_device_layout();
    if (layout_err) {
        /* Dump layout info via CDC for diagnosis */
        tud_init(BOARD_TUD_RHPORT);
        sleep_ms(2000); /* Wait for USB enumeration */
        hal_dump_layout();

        /* Blink error code: long pause, then N blinks = error field */
        while (1) {
            sleep_ms(1000);
            hal_debug_blink(layout_err, 200);
        }
    }

    set_active_output(device, OUTPUT_A);
    rust_main_loop(device);
}

void core1_main() { rust_core1_loop(device); }

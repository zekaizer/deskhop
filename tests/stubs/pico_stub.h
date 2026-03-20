/* Minimal type stubs for host-side unit testing.
 * Provides only the types needed by new module headers
 * without pulling in the full Pico SDK / TinyUSB. */

#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

/* Mouse report layout (matches structs.h for passthrough conversion) */
typedef struct __attribute__((packed)) {
    uint8_t buttons;
    int16_t x;
    int16_t y;
    int8_t  wheel;
    int8_t  pan;
    uint8_t mode;
} mouse_report_t;

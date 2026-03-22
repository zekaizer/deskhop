/*
 * Key Remap Engine — firmware-level key remapping (SIMPLE + TAP_HOLD).
 */
#pragma once

#include <stdint.h>
#include <stdbool.h>

#define MAX_REMAP_ENTRIES    16
#define TAP_HOLD_DEFAULT_US  250000 /* 250ms */

typedef enum {
    REMAP_SIMPLE,
    REMAP_TAP_HOLD,
    /* Future: REMAP_TAP_DANCE, REMAP_COMBO, REMAP_MODIFIER_MORPH, REMAP_MACRO */
} remap_type_t;

typedef struct {
    uint8_t keycode;
    uint8_t modifier;    /* modifier bitmask (0 = none) */
} key_action_t;

typedef struct {
    uint8_t      trigger;      /* physical keycode to intercept */
    remap_type_t type;
    uint8_t      output_mask;  /* bit0=Output A, bit1=Output B, 0xFF=all */

    union {
        struct {
            key_action_t replacement;
        } simple;

        struct {
            key_action_t tap_action;
            key_action_t hold_action;
            uint32_t     threshold_us;
        } tap_hold;
    };
} remap_entry_t;

typedef struct {
    remap_entry_t entries[MAX_REMAP_ENTRIES];
    uint8_t       count;
} remap_config_t;

/* Runtime state per entry */
typedef enum {
    RS_IDLE,
    RS_WAITING,    /* tap-hold: waiting for threshold */
    RS_HELD,       /* tap-hold: hold confirmed */
} remap_state_t;

typedef struct {
    remap_state_t state;
    uint64_t      timestamp;
    bool          consumed;
} remap_runtime_t;

typedef struct {
    remap_config_t   config;
    remap_runtime_t  runtime[MAX_REMAP_ENTRIES];
} remap_engine_t;

typedef enum {
    REMAP_PASS,      /* report unchanged, pass through */
    REMAP_MODIFIED,  /* report modified, pass through */
    REMAP_CONSUMED,  /* report consumed, do not pass */
} remap_result_t;

/* Forward declarations for unit test builds */
#ifdef UNIT_TEST
typedef struct {
    uint8_t modifier;
    uint8_t reserved;
    uint8_t keycode[6];
} hid_keyboard_report_t;

/* Mirror os_type_e values needed by remap engine */
enum { LINUX = 1, MACOS = 2, WINDOWS = 3 };
#endif

void remap_engine_init(remap_engine_t *engine, uint8_t os_a, uint8_t os_b);
remap_result_t remap_engine_process(remap_engine_t *engine,
                                    hid_keyboard_report_t *report,
                                    uint8_t active_output);
bool remap_engine_tick(remap_engine_t *engine, uint64_t now_us);

/* Get pending key report if tick generated one (tap-hold timeout) */
bool remap_engine_get_pending(remap_engine_t *engine,
                              hid_keyboard_report_t *out);

/* Get combined output of all active remap entries (e.g., hold keys in RS_HELD) */
void remap_engine_get_active_output(remap_engine_t *engine,
                                    hid_keyboard_report_t *out);

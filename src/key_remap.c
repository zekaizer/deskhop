/*
 * Key Remap Engine — SIMPLE and TAP_HOLD key remapping.
 */

#ifdef UNIT_TEST
#include "key_remap.h"
#include <string.h>
#else
#include "main.h"
#endif

void remap_engine_init(remap_engine_t *engine, uint8_t os_a, uint8_t os_b) {
    if (!engine)
        return;
    memset(engine->runtime, 0, sizeof(engine->runtime));
    engine->config.count = 0;

    /* CapsLock TAP_HOLD: skip on macOS outputs (native CapsLock → 한영) */
    uint8_t mask = 0;
    if (os_a != MACOS) mask |= (1 << 0); /* Output A */
    if (os_b != MACOS) mask |= (1 << 1); /* Output B */

    if (mask != 0) {
        engine->config.entries[0] = (remap_entry_t){
            .trigger     = 0x39,
            .type        = REMAP_TAP_HOLD,
            .output_mask = mask,
            .tap_hold = {
                .tap_action  = { .keycode = 0x90, .modifier = 0 }, /* LANG1 */
                .hold_action = { .keycode = 0x39, .modifier = 0 }, /* CapsLock */
                .threshold_us = TAP_HOLD_DEFAULT_US,                /* 200ms */
            },
        };
        engine->config.count = 1;
    }
}

/* Check if keycode is present in report */
static bool report_has_key(const hid_keyboard_report_t *report, uint8_t keycode) {
    for (int i = 0; i < 6; i++) {
        if (report->keycode[i] == keycode)
            return true;
    }
    return false;
}

/* Replace keycode in report, return true if found */
static bool report_replace_key(hid_keyboard_report_t *report,
                               uint8_t old_key, uint8_t new_key) {
    for (int i = 0; i < 6; i++) {
        if (report->keycode[i] == old_key) {
            report->keycode[i] = new_key;
            return true;
        }
    }
    return false;
}

/* Remove keycode from report */
static void report_remove_key(hid_keyboard_report_t *report, uint8_t keycode) {
    for (int i = 0; i < 6; i++) {
        if (report->keycode[i] == keycode) {
            report->keycode[i] = 0;
            return;
        }
    }
}

/* Apply modifier from key_action: add action's modifier bits to report */
static void apply_action_modifier(hid_keyboard_report_t *report,
                                  const key_action_t *action) {
    report->modifier |= action->modifier;
}

/* Find remap entry matching trigger key and active output */
static remap_entry_t *find_entry(remap_engine_t *engine, uint8_t keycode,
                                 uint8_t active_output) {
    for (uint8_t i = 0; i < engine->config.count; i++) {
        remap_entry_t *e = &engine->config.entries[i];
        if (e->trigger != keycode)
            continue;
        /* Check output_mask: bit0=A(0), bit1=B(1) */
        if (e->output_mask == 0xFF || (e->output_mask & (1 << active_output)))
            return e;
    }
    return NULL;
}

static uint8_t find_entry_index(remap_engine_t *engine, remap_entry_t *entry) {
    return (uint8_t)(entry - engine->config.entries);
}

remap_result_t remap_engine_process(remap_engine_t *engine,
                                    hid_keyboard_report_t *report,
                                    uint8_t active_output) {
    if (!engine || !report || engine->config.count == 0)
        return REMAP_PASS;

    remap_result_t result = REMAP_PASS;

    for (uint8_t i = 0; i < engine->config.count; i++) {
        remap_entry_t *e = &engine->config.entries[i];
        remap_runtime_t *r = &engine->runtime[i];

        /* Check output_mask */
        if (e->output_mask != 0xFF && !(e->output_mask & (1 << active_output)))
            continue;

        bool key_pressed = report_has_key(report, e->trigger);

        switch (e->type) {
        case REMAP_SIMPLE:
            if (key_pressed) {
                report_replace_key(report, e->trigger, e->simple.replacement.keycode);
                apply_action_modifier(report, &e->simple.replacement);
                result = REMAP_MODIFIED;
            }
            break;

        case REMAP_TAP_HOLD:
            if (key_pressed && r->state == RS_IDLE) {
                /* Key just pressed: start waiting */
                r->state = RS_WAITING;
#ifdef UNIT_TEST
                r->timestamp = 0; /* tests set this manually */
#else
                r->timestamp = time_us_64();
#endif
                /* Remove trigger key from this report (don't send yet) */
                report_remove_key(report, e->trigger);
                result = REMAP_MODIFIED;
            } else if (key_pressed && r->state == RS_WAITING) {
                /* Still held, keep consuming */
                report_remove_key(report, e->trigger);
                result = REMAP_MODIFIED;
            } else if (key_pressed && r->state == RS_HELD) {
                /* Remove trigger — hold key is injected via get_active_output */
                report_remove_key(report, e->trigger);
                result = REMAP_MODIFIED;
            } else if (!key_pressed && r->state == RS_WAITING) {
                /* Released before threshold: emit tap */
                r->state = RS_IDLE;
                r->consumed = true; /* signal pending tap */
                result = REMAP_MODIFIED;
            } else if (!key_pressed && r->state == RS_HELD) {
                /* Released after hold */
                r->state = RS_IDLE;
                result = REMAP_MODIFIED;
            }
            break;
        }
    }

    return result;
}

bool remap_engine_tick(remap_engine_t *engine, uint64_t now_us) {
    if (!engine)
        return false;

    bool state_changed = false;

    for (uint8_t i = 0; i < engine->config.count; i++) {
        remap_entry_t *e = &engine->config.entries[i];
        remap_runtime_t *r = &engine->runtime[i];

        if (e->type != REMAP_TAP_HOLD)
            continue;

        if (r->state == RS_WAITING && r->timestamp > 0) {
            uint32_t threshold = e->tap_hold.threshold_us;
            if (threshold == 0)
                threshold = TAP_HOLD_DEFAULT_US;
            if (now_us - r->timestamp >= threshold) {
                r->state = RS_HELD;
                state_changed = true;
            }
        }
    }

    return state_changed;
}

bool remap_engine_get_pending(remap_engine_t *engine,
                              hid_keyboard_report_t *out) {
    if (!engine || !out)
        return false;

    for (uint8_t i = 0; i < engine->config.count; i++) {
        remap_runtime_t *r = &engine->runtime[i];
        if (r->consumed) {
            r->consumed = false;
            remap_entry_t *e = &engine->config.entries[i];
            if (e->type == REMAP_TAP_HOLD) {
                memset(out, 0, sizeof(hid_keyboard_report_t));
                out->keycode[0] = e->tap_hold.tap_action.keycode;
                out->modifier = e->tap_hold.tap_action.modifier;
                return true;
            }
        }
    }
    return false;
}

void remap_engine_get_active_output(remap_engine_t *engine,
                                    hid_keyboard_report_t *out) {
    memset(out, 0, sizeof(hid_keyboard_report_t));
    if (!engine)
        return;

    for (uint8_t i = 0; i < engine->config.count; i++) {
        remap_entry_t *e = &engine->config.entries[i];
        remap_runtime_t *r = &engine->runtime[i];

        if (e->type == REMAP_TAP_HOLD && r->state == RS_HELD) {
            /* Add hold action key to output — find first empty slot */
            for (int k = 0; k < 6; k++) {
                if (out->keycode[k] == 0) {
                    out->keycode[k] = e->tap_hold.hold_action.keycode;
                    break;
                }
            }
            out->modifier |= e->tap_hold.hold_action.modifier;
        }
    }
}

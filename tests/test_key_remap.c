/*
 * Unit tests for Key Remap Engine (SIMPLE + TAP_HOLD).
 */
#ifndef UNIT_TEST
#define UNIT_TEST
#endif
#include "unity.h"
#include "key_remap.h"
#include <string.h>

static remap_engine_t engine;

void setUp(void) {
    memset(&engine, 0, sizeof(engine));
    remap_engine_init(&engine, LINUX, LINUX);
    /* Clear default entries so each test controls its own config */
    engine.config.count = 0;
}

void tearDown(void) {}

/* Helper: add a SIMPLE remap entry */
static void add_simple(uint8_t trigger, uint8_t replacement,
                        uint8_t rep_modifier, uint8_t output_mask) {
    uint8_t idx = engine.config.count++;
    engine.config.entries[idx] = (remap_entry_t){
        .trigger = trigger,
        .type = REMAP_SIMPLE,
        .output_mask = output_mask,
        .simple = {.replacement = {.keycode = replacement, .modifier = rep_modifier}}
    };
}

/* Helper: add a TAP_HOLD remap entry */
static void add_tap_hold(uint8_t trigger,
                          uint8_t tap_key, uint8_t tap_mod,
                          uint8_t hold_key, uint8_t hold_mod,
                          uint32_t threshold_us, uint8_t output_mask) {
    uint8_t idx = engine.config.count++;
    engine.config.entries[idx] = (remap_entry_t){
        .trigger = trigger,
        .type = REMAP_TAP_HOLD,
        .output_mask = output_mask,
        .tap_hold = {
            .tap_action = {.keycode = tap_key, .modifier = tap_mod},
            .hold_action = {.keycode = hold_key, .modifier = hold_mod},
            .threshold_us = threshold_us
        }
    };
}

/* Helper: create report with one key */
static hid_keyboard_report_t make_report(uint8_t key, uint8_t modifier) {
    hid_keyboard_report_t r = {0};
    r.keycode[0] = key;
    r.modifier = modifier;
    return r;
}

/* ================================================== *
 * ==============  Init Tests  ====================== *
 * ================================================== */

void test_init_zeros_runtime(void) {
    engine.runtime[0].state = RS_HELD;
    remap_engine_init(&engine, LINUX, LINUX);
    TEST_ASSERT_EQUAL(RS_IDLE, engine.runtime[0].state);
}

void test_init_null_safe(void) {
    remap_engine_init(NULL, LINUX, LINUX); /* should not crash */
}

/* ================================================== *
 * ==============  SIMPLE Tests  ==================== *
 * ================================================== */

void test_simple_replaces_key(void) {
    add_simple(0x39, 0xE0, 0, 0xFF); /* CapsLock(0x39) → Left Ctrl(0xE0) */
    hid_keyboard_report_t report = make_report(0x39, 0);

    remap_result_t r = remap_engine_process(&engine, &report, 0);

    TEST_ASSERT_EQUAL(REMAP_MODIFIED, r);
    TEST_ASSERT_EQUAL_UINT8(0xE0, report.keycode[0]);
}

void test_simple_adds_modifier(void) {
    add_simple(0x39, 0x00, 0x01, 0xFF); /* CapsLock → Left Ctrl modifier */
    hid_keyboard_report_t report = make_report(0x39, 0);

    remap_engine_process(&engine, &report, 0);

    TEST_ASSERT_EQUAL_UINT8(0x01, report.modifier);
}

void test_simple_no_match_passes(void) {
    add_simple(0x39, 0xE0, 0, 0xFF);
    hid_keyboard_report_t report = make_report(0x04, 0); /* 'a' key, not CapsLock */

    remap_result_t r = remap_engine_process(&engine, &report, 0);

    TEST_ASSERT_EQUAL(REMAP_PASS, r);
    TEST_ASSERT_EQUAL_UINT8(0x04, report.keycode[0]);
}

void test_simple_output_mask_a_only(void) {
    add_simple(0x39, 0xE0, 0, 0x01); /* Only output A (bit0) */
    hid_keyboard_report_t report = make_report(0x39, 0);

    /* Output A: should remap */
    remap_result_t r = remap_engine_process(&engine, &report, 0);
    TEST_ASSERT_EQUAL(REMAP_MODIFIED, r);
    TEST_ASSERT_EQUAL_UINT8(0xE0, report.keycode[0]);

    /* Output B: should NOT remap */
    report = make_report(0x39, 0);
    r = remap_engine_process(&engine, &report, 1);
    TEST_ASSERT_EQUAL(REMAP_PASS, r);
    TEST_ASSERT_EQUAL_UINT8(0x39, report.keycode[0]);
}

void test_empty_config_passes(void) {
    hid_keyboard_report_t report = make_report(0x04, 0);
    remap_result_t r = remap_engine_process(&engine, &report, 0);
    TEST_ASSERT_EQUAL(REMAP_PASS, r);
}

/* ================================================== *
 * ==============  TAP_HOLD Tests  ================== *
 * ================================================== */

void test_tap_hold_consumes_on_press(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF); /* Tab: tap=LANG1, hold=Tab */
    hid_keyboard_report_t report = make_report(0x2B, 0); /* Tab down */

    remap_result_t r = remap_engine_process(&engine, &report, 0);

    TEST_ASSERT_EQUAL(REMAP_MODIFIED, r);
    TEST_ASSERT_EQUAL_UINT8(0, report.keycode[0]); /* trigger removed */
    TEST_ASSERT_EQUAL(RS_WAITING, engine.runtime[0].state);
}

void test_tap_hold_tap_emits_on_release(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);

    /* Press */
    hid_keyboard_report_t report = make_report(0x2B, 0);
    remap_engine_process(&engine, &report, 0);

    /* Release (no keys) */
    hid_keyboard_report_t empty = {0};
    remap_engine_process(&engine, &empty, 0);

    /* Should have pending tap action */
    hid_keyboard_report_t pending = {0};
    bool has_pending = remap_engine_get_pending(&engine, &pending);
    TEST_ASSERT_TRUE(has_pending);
    TEST_ASSERT_EQUAL_UINT8(0x90, pending.keycode[0]); /* LANG1 */
}

void test_tap_hold_becomes_hold_after_threshold(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);

    /* Press */
    hid_keyboard_report_t report = make_report(0x2B, 0);
    remap_engine_process(&engine, &report, 0);
    engine.runtime[0].timestamp = 1000000; /* 1s */

    /* Tick past threshold */
    remap_engine_tick(&engine, 1200001); /* 1s + 200ms + 1us */

    TEST_ASSERT_EQUAL(RS_HELD, engine.runtime[0].state);
}

void test_tap_hold_held_removes_trigger(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);

    /* Force into HELD state */
    engine.runtime[0].state = RS_HELD;

    /* Report with trigger key held — trigger is removed from report,
     * hold key is injected via get_active_output (not in report itself) */
    hid_keyboard_report_t report = make_report(0x2B, 0);
    remap_result_t r = remap_engine_process(&engine, &report, 0);

    TEST_ASSERT_EQUAL(REMAP_MODIFIED, r);
    TEST_ASSERT_EQUAL_UINT8(0, report.keycode[0]); /* trigger removed */
}

void test_get_active_output_held(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0x02, 200000, 0xFF);
    engine.runtime[0].state = RS_HELD;

    hid_keyboard_report_t out = {0};
    remap_engine_get_active_output(&engine, &out);

    TEST_ASSERT_EQUAL_UINT8(0x2B, out.keycode[0]); /* hold action key */
    TEST_ASSERT_EQUAL_UINT8(0x02, out.modifier);    /* hold action modifier */
}

void test_get_active_output_idle_empty(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);
    /* state is RS_IDLE (default) */

    hid_keyboard_report_t out = {0};
    remap_engine_get_active_output(&engine, &out);

    TEST_ASSERT_EQUAL_UINT8(0, out.keycode[0]); /* no active output */
}

void test_tick_returns_true_on_hold_transition(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);

    hid_keyboard_report_t report = make_report(0x2B, 0);
    remap_engine_process(&engine, &report, 0);
    engine.runtime[0].timestamp = 1000000;

    /* Before threshold: returns false */
    bool changed = remap_engine_tick(&engine, 1100000);
    TEST_ASSERT_FALSE(changed);

    /* At threshold: returns true */
    changed = remap_engine_tick(&engine, 1200000);
    TEST_ASSERT_TRUE(changed);
    TEST_ASSERT_EQUAL(RS_HELD, engine.runtime[0].state);

    /* Already held: returns false */
    changed = remap_engine_tick(&engine, 1300000);
    TEST_ASSERT_FALSE(changed);
}

void test_tap_hold_release_after_hold(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);
    engine.runtime[0].state = RS_HELD;

    /* Release */
    hid_keyboard_report_t empty = {0};
    remap_engine_process(&engine, &empty, 0);

    TEST_ASSERT_EQUAL(RS_IDLE, engine.runtime[0].state);

    /* No pending tap after hold release */
    hid_keyboard_report_t pending = {0};
    TEST_ASSERT_FALSE(remap_engine_get_pending(&engine, &pending));
}

void test_tap_hold_tick_no_crash_on_idle(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 200000, 0xFF);
    remap_engine_tick(&engine, 999999999); /* should not crash */
    TEST_ASSERT_EQUAL(RS_IDLE, engine.runtime[0].state);
}

void test_tap_hold_default_threshold(void) {
    add_tap_hold(0x2B, 0x90, 0, 0x2B, 0, 0, 0xFF); /* threshold=0 → use default */

    hid_keyboard_report_t report = make_report(0x2B, 0);
    remap_engine_process(&engine, &report, 0);
    engine.runtime[0].timestamp = 1000000;

    /* Just before default threshold */
    remap_engine_tick(&engine, 1000000 + TAP_HOLD_DEFAULT_US - 1);
    TEST_ASSERT_EQUAL(RS_WAITING, engine.runtime[0].state);

    /* At default threshold */
    remap_engine_tick(&engine, 1000000 + TAP_HOLD_DEFAULT_US);
    TEST_ASSERT_EQUAL(RS_HELD, engine.runtime[0].state);
}

/* ================================================== *
 * ==============  Multiple Entries  ================= *
 * ================================================== */

void test_multiple_simple_entries(void) {
    add_simple(0x39, 0xE0, 0, 0xFF);  /* CapsLock → Ctrl */
    add_simple(0xE6, 0x90, 0, 0xFF);  /* Right Alt → LANG1 */

    hid_keyboard_report_t report = {0};
    report.keycode[0] = 0x39;
    report.keycode[1] = 0xE6;

    remap_engine_process(&engine, &report, 0);

    TEST_ASSERT_EQUAL_UINT8(0xE0, report.keycode[0]);
    TEST_ASSERT_EQUAL_UINT8(0x90, report.keycode[1]);
}

void test_null_engine_passes(void) {
    hid_keyboard_report_t report = make_report(0x04, 0);
    TEST_ASSERT_EQUAL(REMAP_PASS, remap_engine_process(NULL, &report, 0));
}

void test_null_report_passes(void) {
    TEST_ASSERT_EQUAL(REMAP_PASS, remap_engine_process(&engine, NULL, 0));
}

int main(void) {
    UNITY_BEGIN();

    /* Init */
    RUN_TEST(test_init_zeros_runtime);
    RUN_TEST(test_init_null_safe);

    /* SIMPLE */
    RUN_TEST(test_simple_replaces_key);
    RUN_TEST(test_simple_adds_modifier);
    RUN_TEST(test_simple_no_match_passes);
    RUN_TEST(test_simple_output_mask_a_only);
    RUN_TEST(test_empty_config_passes);

    /* TAP_HOLD */
    RUN_TEST(test_tap_hold_consumes_on_press);
    RUN_TEST(test_tap_hold_tap_emits_on_release);
    RUN_TEST(test_tap_hold_becomes_hold_after_threshold);
    RUN_TEST(test_tap_hold_held_removes_trigger);
    RUN_TEST(test_get_active_output_held);
    RUN_TEST(test_get_active_output_idle_empty);
    RUN_TEST(test_tick_returns_true_on_hold_transition);
    RUN_TEST(test_tap_hold_release_after_hold);
    RUN_TEST(test_tap_hold_tick_no_crash_on_idle);
    RUN_TEST(test_tap_hold_default_threshold);

    /* Multiple entries / edge cases */
    RUN_TEST(test_multiple_simple_entries);
    RUN_TEST(test_null_engine_passes);
    RUN_TEST(test_null_report_passes);

    return UNITY_END();
}

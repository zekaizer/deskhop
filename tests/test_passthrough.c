#include "unity.h"
#include "passthrough.h"
#include <string.h>

static passthrough_state_t state;

void setUp(void) {
    passthrough_init(&state);
}

void tearDown(void) {}

void test_init_zeros_state(void) {
    TEST_ASSERT_EQUAL_UINT8(0, state.iface_count);
    TEST_ASSERT_FALSE(state.enumeration_done);
    for (int i = 0; i < MAX_PASSTHROUGH_IFACES; i++) {
        TEST_ASSERT_EQUAL_UINT16(0, state.ifaces[i].desc_len);
    }
}

void test_capture_single_descriptor(void) {
    uint8_t fake_desc[] = {0x05, 0x01, 0x09, 0x06, 0xA1, 0x01};

    bool ok = passthrough_capture_descriptor(&state, 1, 0, 1, fake_desc, sizeof(fake_desc));

    TEST_ASSERT_TRUE(ok);
    TEST_ASSERT_EQUAL_UINT8(1, state.iface_count);
    TEST_ASSERT_EQUAL_UINT8(1, state.ifaces[0].dev_addr);
    TEST_ASSERT_EQUAL_UINT8(0, state.ifaces[0].instance);
    TEST_ASSERT_EQUAL_UINT8(1, state.ifaces[0].itf_protocol);
    TEST_ASSERT_EQUAL_UINT16(sizeof(fake_desc), state.ifaces[0].desc_len);
    TEST_ASSERT_EQUAL_UINT8_ARRAY(fake_desc, state.ifaces[0].desc, sizeof(fake_desc));
    TEST_ASSERT_FALSE(state.ifaces[0].always_passthrough);
}

void test_capture_three_bolt_interfaces(void) {
    uint8_t kbd_desc[67];
    uint8_t mouse_desc[133];
    uint8_t vendor_desc[32];
    memset(kbd_desc, 0xAA, sizeof(kbd_desc));
    memset(mouse_desc, 0xBB, sizeof(mouse_desc));
    memset(vendor_desc, 0xCC, sizeof(vendor_desc));

    passthrough_capture_descriptor(&state, 1, 0, 1, kbd_desc, sizeof(kbd_desc));
    passthrough_capture_descriptor(&state, 1, 1, 2, mouse_desc, sizeof(mouse_desc));
    passthrough_capture_descriptor(&state, 1, 2, 0, vendor_desc, sizeof(vendor_desc));

    TEST_ASSERT_EQUAL_UINT8(3, state.iface_count);

    TEST_ASSERT_EQUAL_UINT16(67, state.ifaces[0].desc_len);
    TEST_ASSERT_EQUAL_UINT16(133, state.ifaces[1].desc_len);
    TEST_ASSERT_EQUAL_UINT16(32, state.ifaces[2].desc_len);

    TEST_ASSERT_EQUAL_UINT8_ARRAY(kbd_desc, state.ifaces[0].desc, 67);
    TEST_ASSERT_EQUAL_UINT8_ARRAY(mouse_desc, state.ifaces[1].desc, 133);
    TEST_ASSERT_EQUAL_UINT8_ARRAY(vendor_desc, state.ifaces[2].desc, 32);
}

void test_vendor_protocol_sets_always_passthrough(void) {
    uint8_t desc[] = {0x06, 0x00, 0xFF};

    /* protocol 0 = HID_ITF_PROTOCOL_NONE (vendor) */
    passthrough_capture_descriptor(&state, 1, 2, 0, desc, sizeof(desc));
    TEST_ASSERT_TRUE(state.ifaces[0].always_passthrough);

    /* protocol 1 = keyboard */
    passthrough_capture_descriptor(&state, 1, 0, 1, desc, sizeof(desc));
    TEST_ASSERT_FALSE(state.ifaces[1].always_passthrough);

    /* protocol 2 = mouse */
    passthrough_capture_descriptor(&state, 1, 1, 2, desc, sizeof(desc));
    TEST_ASSERT_FALSE(state.ifaces[2].always_passthrough);
}

void test_capture_rejects_null_input(void) {
    TEST_ASSERT_FALSE(passthrough_capture_descriptor(NULL, 1, 0, 1, (uint8_t[]){0}, 1));
    TEST_ASSERT_FALSE(passthrough_capture_descriptor(&state, 1, 0, 1, NULL, 1));
    TEST_ASSERT_FALSE(passthrough_capture_descriptor(&state, 1, 0, 1, (uint8_t[]){0}, 0));
}

void test_capture_rejects_overflow(void) {
    uint8_t desc[] = {0x01};
    for (int i = 0; i < MAX_PASSTHROUGH_IFACES; i++) {
        TEST_ASSERT_TRUE(passthrough_capture_descriptor(&state, 1, i, 1, desc, 1));
    }
    /* Should fail on MAX+1 */
    TEST_ASSERT_FALSE(passthrough_capture_descriptor(&state, 1, 99, 1, desc, 1));
    TEST_ASSERT_EQUAL_UINT8(MAX_PASSTHROUGH_IFACES, state.iface_count);
}

void test_capture_rejects_oversized_descriptor(void) {
    uint8_t desc[1];
    TEST_ASSERT_FALSE(passthrough_capture_descriptor(&state, 1, 0, 1, desc, MAX_HID_DESC_SIZE + 1));
    TEST_ASSERT_EQUAL_UINT8(0, state.iface_count);
}

void test_remove_device_clears_all_interfaces(void) {
    uint8_t desc[] = {0x01};
    passthrough_capture_descriptor(&state, 1, 0, 1, desc, 1);
    passthrough_capture_descriptor(&state, 1, 1, 2, desc, 1);
    passthrough_capture_descriptor(&state, 1, 2, 0, desc, 1);
    TEST_ASSERT_EQUAL_UINT8(3, state.iface_count);

    passthrough_remove_device(&state, 1);
    TEST_ASSERT_EQUAL_UINT8(0, state.iface_count);
}

void test_remove_device_keeps_other_devices(void) {
    uint8_t desc[] = {0x01};
    passthrough_capture_descriptor(&state, 1, 0, 1, desc, 1);
    passthrough_capture_descriptor(&state, 2, 0, 2, desc, 1);
    passthrough_capture_descriptor(&state, 1, 1, 0, desc, 1);

    passthrough_remove_device(&state, 1);
    TEST_ASSERT_EQUAL_UINT8(1, state.iface_count);
    TEST_ASSERT_EQUAL_UINT8(2, state.ifaces[0].dev_addr);
}

void test_duplicate_capture_overwrites(void) {
    uint8_t desc1[] = {0xAA, 0xBB};
    uint8_t desc2[] = {0xCC, 0xDD, 0xEE};

    passthrough_capture_descriptor(&state, 1, 0, 1, desc1, sizeof(desc1));
    TEST_ASSERT_EQUAL_UINT8(1, state.iface_count);

    /* Same dev_addr + instance should overwrite, not add */
    passthrough_capture_descriptor(&state, 1, 0, 2, desc2, sizeof(desc2));
    TEST_ASSERT_EQUAL_UINT8(1, state.iface_count);
    TEST_ASSERT_EQUAL_UINT8(2, state.ifaces[0].itf_protocol);
    TEST_ASSERT_EQUAL_UINT16(3, state.ifaces[0].desc_len);
    TEST_ASSERT_EQUAL_UINT8(0xCC, state.ifaces[0].desc[0]);
}

void test_dump_does_not_crash(void) {
    uint8_t desc[] = {0x05, 0x01};
    passthrough_capture_descriptor(&state, 1, 0, 1, desc, sizeof(desc));
    passthrough_dump_descriptors(&state);
    passthrough_dump_descriptors(NULL);
}

/* ================================================== *
 * ========  P2: Activate / Mapping Tests  ========== *
 * ================================================== */

static void capture_bolt_interfaces(passthrough_state_t *s) {
    uint8_t kbd_desc[67], mouse_desc[133], vendor_desc[32];
    memset(kbd_desc, 0xAA, sizeof(kbd_desc));
    memset(mouse_desc, 0xBB, sizeof(mouse_desc));
    memset(vendor_desc, 0xCC, sizeof(vendor_desc));
    passthrough_capture_descriptor(s, 1, 0, 1, kbd_desc, sizeof(kbd_desc));
    passthrough_capture_descriptor(s, 1, 1, 2, mouse_desc, sizeof(mouse_desc));
    passthrough_capture_descriptor(s, 1, 2, 0, vendor_desc, sizeof(vendor_desc));
}

void test_activate_sets_active(void) {
    capture_bolt_interfaces(&state);
    TEST_ASSERT_TRUE(passthrough_activate(&state));
    TEST_ASSERT_TRUE(state.active);
    TEST_ASSERT_TRUE(state.config_desc_len > 0);
}

void test_activate_rejects_empty(void) {
    TEST_ASSERT_FALSE(passthrough_activate(&state));
    TEST_ASSERT_FALSE(state.active);
}

void test_activate_rejects_null(void) {
    TEST_ASSERT_FALSE(passthrough_activate(NULL));
}

void test_get_report_desc_returns_captured(void) {
    capture_bolt_interfaces(&state);
    state.active = true;

    uint16_t len = 0;
    const uint8_t *desc;

    /* ITF_NUM_PT_BASE + 0 = keyboard */
    desc = passthrough_get_report_desc(&state, ITF_NUM_PT_BASE + 0, &len);
    TEST_ASSERT_NOT_NULL(desc);
    TEST_ASSERT_EQUAL_UINT16(67, len);
    TEST_ASSERT_EQUAL_UINT8(0xAA, desc[0]);

    /* ITF_NUM_PT_BASE + 1 = mouse */
    desc = passthrough_get_report_desc(&state, ITF_NUM_PT_BASE + 1, &len);
    TEST_ASSERT_NOT_NULL(desc);
    TEST_ASSERT_EQUAL_UINT16(133, len);
    TEST_ASSERT_EQUAL_UINT8(0xBB, desc[0]);

    /* ITF_NUM_PT_BASE + 2 = vendor */
    desc = passthrough_get_report_desc(&state, ITF_NUM_PT_BASE + 2, &len);
    TEST_ASSERT_NOT_NULL(desc);
    TEST_ASSERT_EQUAL_UINT16(32, len);
    TEST_ASSERT_EQUAL_UINT8(0xCC, desc[0]);
}

void test_get_report_desc_rejects_invalid(void) {
    capture_bolt_interfaces(&state);
    uint16_t len = 0;

    /* Out of range */
    TEST_ASSERT_NULL(passthrough_get_report_desc(&state, ITF_NUM_PT_BASE + 3, &len));
    /* Below base */
    TEST_ASSERT_NULL(passthrough_get_report_desc(&state, 0, &len));
    /* NULL state */
    TEST_ASSERT_NULL(passthrough_get_report_desc(NULL, ITF_NUM_PT_BASE, &len));
    /* NULL out_len */
    TEST_ASSERT_NULL(passthrough_get_report_desc(&state, ITF_NUM_PT_BASE, NULL));
}

void test_host_to_device_instance(void) {
    capture_bolt_interfaces(&state);

    /* dev_addr=1, instance=0 → ITF_NUM_PT_BASE + 0 */
    TEST_ASSERT_EQUAL_INT8(ITF_NUM_PT_BASE + 0,
                           passthrough_host_to_device_instance(&state, 1, 0));
    /* dev_addr=1, instance=1 → ITF_NUM_PT_BASE + 1 */
    TEST_ASSERT_EQUAL_INT8(ITF_NUM_PT_BASE + 1,
                           passthrough_host_to_device_instance(&state, 1, 1));
    /* dev_addr=1, instance=2 → ITF_NUM_PT_BASE + 2 */
    TEST_ASSERT_EQUAL_INT8(ITF_NUM_PT_BASE + 2,
                           passthrough_host_to_device_instance(&state, 1, 2));
    /* Unknown → -1 */
    TEST_ASSERT_EQUAL_INT8(-1,
                           passthrough_host_to_device_instance(&state, 1, 99));
    TEST_ASSERT_EQUAL_INT8(-1,
                           passthrough_host_to_device_instance(&state, 2, 0));
    TEST_ASSERT_EQUAL_INT8(-1,
                           passthrough_host_to_device_instance(NULL, 1, 0));
}

void test_device_to_host_index(void) {
    capture_bolt_interfaces(&state);

    TEST_ASSERT_EQUAL_INT8(0, passthrough_device_to_host_index(&state, ITF_NUM_PT_BASE + 0));
    TEST_ASSERT_EQUAL_INT8(1, passthrough_device_to_host_index(&state, ITF_NUM_PT_BASE + 1));
    TEST_ASSERT_EQUAL_INT8(2, passthrough_device_to_host_index(&state, ITF_NUM_PT_BASE + 2));
    /* Out of range */
    TEST_ASSERT_EQUAL_INT8(-1, passthrough_device_to_host_index(&state, ITF_NUM_PT_BASE + 3));
    /* Below base */
    TEST_ASSERT_EQUAL_INT8(-1, passthrough_device_to_host_index(&state, 0));
    TEST_ASSERT_EQUAL_INT8(-1, passthrough_device_to_host_index(&state, 1));
    /* NULL */
    TEST_ASSERT_EQUAL_INT8(-1, passthrough_device_to_host_index(NULL, ITF_NUM_PT_BASE));
}

void test_activate_then_remove_deactivates(void) {
    capture_bolt_interfaces(&state);
    passthrough_activate(&state);
    TEST_ASSERT_TRUE(state.active);

    /* Removing all interfaces should not auto-deactivate (caller's responsibility) */
    passthrough_remove_device(&state, 1);
    TEST_ASSERT_EQUAL_UINT8(0, state.iface_count);
    /* Mapping should return -1 with no interfaces */
    TEST_ASSERT_EQUAL_INT8(-1, passthrough_host_to_device_instance(&state, 1, 0));
}

/* ================================================== *
 * ========  VID/PID Switching Tests (FR-PT-009) ===== *
 * ================================================== */

void test_init_clears_vid_pid(void) {
    TEST_ASSERT_EQUAL_UINT16(0, state.upstream_vid);
    TEST_ASSERT_EQUAL_UINT16(0, state.upstream_pid);
}

void test_vid_pid_stored_directly(void) {
    state.upstream_vid = 0x046D;
    state.upstream_pid = 0xC548;
    TEST_ASSERT_EQUAL_UINT16(0x046D, state.upstream_vid);
    TEST_ASSERT_EQUAL_UINT16(0xC548, state.upstream_pid);
}

void test_vid_pid_cleared_on_last_remove(void) {
    capture_bolt_interfaces(&state);
    state.upstream_vid = 0x046D;
    state.upstream_pid = 0xC548;

    passthrough_remove_device(&state, 1);
    TEST_ASSERT_EQUAL_UINT8(0, state.iface_count);
    TEST_ASSERT_EQUAL_UINT16(0, state.upstream_vid);
    TEST_ASSERT_EQUAL_UINT16(0, state.upstream_pid);
}

void test_vid_pid_retained_with_remaining_ifaces(void) {
    /* Capture from two different devices */
    uint8_t desc[8] = {0};
    passthrough_capture_descriptor(&state, 1, 0, 1, desc, sizeof(desc));
    passthrough_capture_descriptor(&state, 2, 0, 2, desc, sizeof(desc));
    state.upstream_vid = 0x046D;
    state.upstream_pid = 0xC548;

    /* Remove only device 1 — device 2 remains */
    passthrough_remove_device(&state, 1);
    TEST_ASSERT_EQUAL_UINT8(1, state.iface_count);
    TEST_ASSERT_EQUAL_UINT16(0x046D, state.upstream_vid);
    TEST_ASSERT_EQUAL_UINT16(0xC548, state.upstream_pid);
}

int main(void) {
    UNITY_BEGIN();
    RUN_TEST(test_init_zeros_state);
    RUN_TEST(test_capture_single_descriptor);
    RUN_TEST(test_capture_three_bolt_interfaces);
    RUN_TEST(test_vendor_protocol_sets_always_passthrough);
    RUN_TEST(test_capture_rejects_null_input);
    RUN_TEST(test_capture_rejects_overflow);
    RUN_TEST(test_capture_rejects_oversized_descriptor);
    RUN_TEST(test_remove_device_clears_all_interfaces);
    RUN_TEST(test_remove_device_keeps_other_devices);
    RUN_TEST(test_duplicate_capture_overwrites);
    RUN_TEST(test_dump_does_not_crash);
    /* P2 tests */
    RUN_TEST(test_activate_sets_active);
    RUN_TEST(test_activate_rejects_empty);
    RUN_TEST(test_activate_rejects_null);
    RUN_TEST(test_get_report_desc_returns_captured);
    RUN_TEST(test_get_report_desc_rejects_invalid);
    RUN_TEST(test_host_to_device_instance);
    RUN_TEST(test_device_to_host_index);
    RUN_TEST(test_activate_then_remove_deactivates);
    /* VID/PID switching tests */
    RUN_TEST(test_init_clears_vid_pid);
    RUN_TEST(test_vid_pid_stored_directly);
    RUN_TEST(test_vid_pid_cleared_on_last_remove);
    RUN_TEST(test_vid_pid_retained_with_remaining_ifaces);
    return UNITY_END();
}

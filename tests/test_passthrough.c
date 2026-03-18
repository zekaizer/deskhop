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

int main(void) {
    UNITY_BEGIN();
    RUN_TEST(test_init_zeros_state);
    RUN_TEST(test_capture_single_descriptor);
    RUN_TEST(test_capture_three_bolt_interfaces);
    RUN_TEST(test_vendor_protocol_sets_always_passthrough);
    RUN_TEST(test_capture_rejects_null_input);
    RUN_TEST(test_capture_rejects_overflow);
    RUN_TEST(test_capture_rejects_oversized_descriptor);
    return UNITY_END();
}

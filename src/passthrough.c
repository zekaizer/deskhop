/*
 * Semi-DDM USB Passthrough — descriptor capture and state management.
 */

#include "main.h"

static passthrough_state_t pt_state;

passthrough_state_t *passthrough_get_state(void) {
    return &pt_state;
}

void passthrough_init(passthrough_state_t *state) {
    memset(state, 0, sizeof(passthrough_state_t));
}

bool passthrough_capture_descriptor(passthrough_state_t *state,
                                    uint8_t dev_addr,
                                    uint8_t instance,
                                    uint8_t itf_protocol,
                                    uint8_t const *desc_report,
                                    uint16_t desc_len) {
    if (!state || !desc_report || desc_len == 0)
        return false;

    if (state->iface_count >= MAX_PASSTHROUGH_IFACES)
        return false;

    if (desc_len > MAX_HID_DESC_SIZE)
        return false;

    passthrough_iface_t *iface = &state->ifaces[state->iface_count];

    iface->dev_addr     = dev_addr;
    iface->instance     = instance;
    iface->itf_protocol = itf_protocol;
    iface->desc_len     = desc_len;
    memcpy(iface->desc, desc_report, desc_len);

    /* HID++ vendor interfaces use protocol NONE */
    iface->always_passthrough = (itf_protocol == HID_ITF_PROTOCOL_NONE);

    state->iface_count++;
    return true;
}

void passthrough_dump_descriptors(const passthrough_state_t *state) {
    if (!state)
        return;

    static const char *proto_names[] = {"NONE/Vendor", "Keyboard", "Mouse"};

    dh_debug_printf("\n=== Passthrough: %d interface(s) captured ===\n", state->iface_count);

    for (uint8_t i = 0; i < state->iface_count; i++) {
        const passthrough_iface_t *iface = &state->ifaces[i];
        const char *name = (iface->itf_protocol <= 2) ? proto_names[iface->itf_protocol] : "Unknown";

        dh_debug_printf("\n[%d] dev=%d inst=%d proto=%s len=%d always_pt=%d\n",
                        i, iface->dev_addr, iface->instance,
                        name, iface->desc_len, iface->always_passthrough);

        /* Hex dump in rows of 16 bytes */
        for (uint16_t off = 0; off < iface->desc_len; off += 16) {
            dh_debug_printf("  %04X: ", off);
            for (uint16_t j = 0; j < 16 && (off + j) < iface->desc_len; j++) {
                dh_debug_printf("%02X ", iface->desc[off + j]);
            }
            dh_debug_printf("\n");
        }
    }
    dh_debug_printf("=== End descriptor dump ===\n\n");
}

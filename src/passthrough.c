/*
 * Semi-DDM USB Passthrough — descriptor capture and state management.
 */

#ifdef UNIT_TEST
#include "passthrough.h"
#include <string.h>
#define HID_ITF_PROTOCOL_NONE 0
int dh_debug_printf(const char *format, ...);
#else
#include "main.h"
#endif

/* ================================================== *
 * ==============  P2: Device-side API  ============= *
 * ================================================== */

#ifdef UNIT_TEST
/* Stub: real implementation lives in usb_descriptors.c */
void passthrough_build_config_desc(passthrough_state_t *state) {
    state->config_desc_len = 9;
}
#endif

bool passthrough_activate(passthrough_state_t *state) {
    if (!state || state->iface_count == 0)
        return false;

    passthrough_build_config_desc(state);

    if (state->config_desc_len == 0)
        return false;

    state->active = true;
    return true;
}

const uint8_t *passthrough_get_report_desc(const passthrough_state_t *state,
                                           uint8_t device_instance,
                                           uint16_t *out_len) {
    if (!state || !out_len || device_instance < ITF_NUM_PT_BASE)
        return NULL;

    uint8_t idx = device_instance - ITF_NUM_PT_BASE;
    if (idx >= state->iface_count)
        return NULL;

    *out_len = state->ifaces[idx].desc_len;
    return state->ifaces[idx].desc;
}

int8_t passthrough_host_to_device_instance(const passthrough_state_t *state,
                                           uint8_t dev_addr,
                                           uint8_t instance) {
    if (!state)
        return -1;

    for (uint8_t i = 0; i < state->iface_count; i++) {
        if (state->ifaces[i].dev_addr == dev_addr &&
            state->ifaces[i].instance == instance) {
            return (int8_t)(ITF_NUM_PT_BASE + i);
        }
    }
    return -1;
}

int8_t passthrough_device_to_host_index(const passthrough_state_t *state,
                                        uint8_t device_instance) {
    if (!state || device_instance < ITF_NUM_PT_BASE)
        return -1;

    uint8_t idx = device_instance - ITF_NUM_PT_BASE;
    if (idx >= state->iface_count)
        return -1;

    return (int8_t)idx;
}

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

    if (desc_len > MAX_HID_DESC_SIZE)
        return false;

    /* Check for duplicate (same dev_addr + instance) and overwrite */
    passthrough_iface_t *iface = NULL;
    for (uint8_t i = 0; i < state->iface_count; i++) {
        if (state->ifaces[i].dev_addr == dev_addr && state->ifaces[i].instance == instance) {
            iface = &state->ifaces[i];
            break;
        }
    }

    /* No existing entry — allocate new slot */
    if (!iface) {
        if (state->iface_count >= MAX_PASSTHROUGH_IFACES)
            return false;
        iface = &state->ifaces[state->iface_count++];
    }

    iface->dev_addr     = dev_addr;
    iface->instance     = instance;
    iface->itf_protocol = itf_protocol;
    iface->desc_len     = desc_len;
    memcpy(iface->desc, desc_report, desc_len);

    /* HID++ vendor interfaces use protocol NONE */
    iface->always_passthrough = (itf_protocol == HID_ITF_PROTOCOL_NONE);

    return true;
}

void passthrough_remove_device(passthrough_state_t *state, uint8_t dev_addr) {
    if (!state)
        return;

    /* Remove all interfaces belonging to this device, compact the array */
    uint8_t write = 0;
    for (uint8_t read = 0; read < state->iface_count; read++) {
        if (state->ifaces[read].dev_addr != dev_addr) {
            if (write != read)
                state->ifaces[write] = state->ifaces[read];
            write++;
        }
    }
    /* Zero out vacated slots */
    for (uint8_t i = write; i < state->iface_count; i++)
        memset(&state->ifaces[i], 0, sizeof(passthrough_iface_t));

    state->iface_count = write;

    /* Clear upstream identity when no interfaces remain */
    if (state->iface_count == 0) {
        state->upstream_vid = 0;
        state->upstream_pid = 0;
    }
}

bool passthrough_is_hidpp_input_event(const uint8_t *report, uint16_t len) {
    if (len < 4)
        return false;
    /* HID++ 2.0 short (0x10) or long (0x11) report */
    if (report[0] != 0x10 && report[0] != 0x11)
        return false;
    /* sw_id == 0 → unsolicited event (device-initiated input) */
    return (report[3] & 0x0F) == 0;
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

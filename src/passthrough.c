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
    if (report[0] != HIDPP_REPORT_ID_SHORT && report[0] != HIDPP_REPORT_ID_LONG)
        return false;
    return (report[3] & 0x0F) == 0;
}

/* ================================================== *
 * =========  P3: IRoot Feature Discovery  ========== *
 * ================================================== */

bool passthrough_send_iroot_query(passthrough_state_t *state,
                                  uint8_t device_idx, uint16_t feature_id) {
    if (!state || state->out_queue.pending)
        return false;

    /* IRoot is always feature index 0, function 0 */
    state->out_queue.dev_addr    = state->ifaces[0].dev_addr;
    state->out_queue.instance    = 0; /* find vendor interface instance */
    for (uint8_t i = 0; i < state->iface_count; i++) {
        if (state->ifaces[i].always_passthrough) {
            state->out_queue.dev_addr = state->ifaces[i].dev_addr;
            state->out_queue.instance = state->ifaces[i].instance;
            break;
        }
    }
    state->out_queue.report_id   = HIDPP_REPORT_ID_SHORT;
    state->out_queue.report_type = 2; /* HID_REPORT_TYPE_OUTPUT */
    state->out_queue.data[0]     = device_idx;
    state->out_queue.data[1]     = 0x00; /* feature index 0 = IRoot */
    state->out_queue.data[2]     = (0x00 << 4) | HIDPP_SWID_DESKHOP; /* fn=0 | sw_id */
    state->out_queue.data[3]     = (uint8_t)(feature_id >> 8);
    state->out_queue.data[4]     = (uint8_t)(feature_id & 0xFF);
    state->out_queue.data[5]     = 0x00;
    state->out_queue.len         = 6;
    state->out_queue.pending     = true;

    return true;
}

void passthrough_handle_iroot_response(passthrough_state_t *state,
                                       const uint8_t *report, uint16_t len) {
    if (!state || len < 7)
        return;

    uint8_t feature_index = report[4];
    hidpp_discovery_t *d = &state->hidpp_disc;

    switch (d->state) {
    case DISC_QUERY_HIRES_SCROLL:
        d->fi_hires_scroll = feature_index;
        dh_debug_printf("[DISC] HiResScroll → index 0x%02X\n", feature_index);
        d->state = DISC_QUERY_THUMBWHEEL;
        d->query_sent_us = 0;
        break;
    case DISC_QUERY_THUMBWHEEL:
        d->fi_thumbwheel = feature_index;
        dh_debug_printf("[DISC] Thumbwheel → index 0x%02X\n", feature_index);
        d->state = DISC_DONE;
        d->done = true;
        dh_debug_printf("[DISC] Discovery complete: scroll=0x%02X thumb=0x%02X\n",
                        d->fi_hires_scroll, d->fi_thumbwheel);
        break;
    default:
        break;
    }
}

#ifndef UNIT_TEST
#include "pico/time.h"
#endif

void passthrough_discovery_step(passthrough_state_t *state) {
    if (!state || !state->active)
        return;

    hidpp_discovery_t *d = &state->hidpp_disc;
    if (d->done || d->state == DISC_IDLE)
        return;

#ifdef UNIT_TEST
    uint64_t now = 0;
#else
    uint64_t now = time_us_64();
#endif

    /* Timeout: skip current query after 2s */
    if (d->query_sent_us > 0 && (now - d->query_sent_us) > HIDPP_DISC_TIMEOUT_US) {
        dh_debug_printf("[DISC] Timeout in state %d, advancing\n", d->state);
        if (d->state == DISC_QUERY_HIRES_SCROLL)
            d->state = DISC_QUERY_THUMBWHEEL;
        else {
            d->state = DISC_DONE;
            d->done = true;
        }
        d->query_sent_us = 0;
        return;
    }

    /* Wait for device detection */
    if (d->state == DISC_DETECT_DEVICE)
        return;

    /* Send queries when out_queue is free */
    if (d->query_sent_us > 0)
        return; /* waiting for response */

    uint16_t feature_id = 0;
    if (d->state == DISC_QUERY_HIRES_SCROLL)
        feature_id = 0x2121;
    else if (d->state == DISC_QUERY_THUMBWHEEL)
        feature_id = 0x2150;

    if (feature_id && passthrough_send_iroot_query(state, d->device_idx, feature_id)) {
        d->query_sent_us = now ? now : 1; /* avoid 0 which means "not sent" */
        dh_debug_printf("[DISC] Sent IRoot query for 0x%04X\n", feature_id);
    }
}

/* ================================================== *
 * =====  P3: HID++ → Mouse Report Conversion  ===== *
 * ================================================== */

/* Minimal mouse report layout for conversion (matches structs.h mouse_report_t) */
typedef struct {
    uint8_t buttons;
    int16_t x;
    int16_t y;
    int8_t  wheel;
    int8_t  pan;
    uint8_t mode;
} passthrough_mouse_report_t;

static int8_t clamp_i8(int16_t val) {
    if (val > 127) return 127;
    if (val < -128) return -128;
    return (int8_t)val;
}

bool passthrough_convert_hidpp_to_mouse(const passthrough_state_t *state,
                                        const uint8_t *report, uint16_t len,
                                        void *out_mouse) {
    if (!state || !report || !out_mouse || len < 7)
        return false;

    const hidpp_discovery_t *d = &state->hidpp_disc;
    if (!d->done)
        return false;

    uint8_t feature_idx = report[2];
    passthrough_mouse_report_t *m = (passthrough_mouse_report_t *)out_mouse;

    if (d->fi_hires_scroll && feature_idx == d->fi_hires_scroll) {
        /* HiRes Scroll event: params[0]=flags, params[1-2]=deltaV (int16 BE) */
        int16_t delta_v = (int16_t)((report[5] << 8) | report[6]);
        m->wheel = clamp_i8(delta_v);
        return true;
    }

    if (d->fi_thumbwheel && feature_idx == d->fi_thumbwheel) {
        /* Thumbwheel event: horizontal scroll delta */
        int16_t delta_h = (int16_t)((report[5] << 8) | report[6]);
        m->pan = clamp_i8(delta_h);
        return true;
    }

    return false;
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

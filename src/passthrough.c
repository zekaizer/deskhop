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

    /* Reset passthrough state when no interfaces remain */
    if (state->iface_count == 0) {
        state->upstream_vid = 0;
        state->upstream_pid = 0;
        state->active = false;
        state->hidpp_disc.button_state = 0;
        state->hidpp_disc.fi_reprog_controls = 0;
        state->hidpp_disc.done = false;
        state->hidpp_disc.state = DISC_IDLE;
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
    case DISC_QUERY_REPROG_CONTROLS:
        d->fi_reprog_controls = feature_index;
        dh_debug_printf("[DISC] ReprogControls → index 0x%02X\n", feature_index);
        d->state = DISC_QUERY_HIRES_SCROLL;
        d->query_sent_us = 0;
        break;
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
        dh_debug_printf("[DISC] Discovery complete: reprog=0x%02X scroll=0x%02X thumb=0x%02X\n",
                        d->fi_reprog_controls, d->fi_hires_scroll, d->fi_thumbwheel);
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
        switch (d->state) {
        case DISC_QUERY_REPROG_CONTROLS: d->state = DISC_QUERY_HIRES_SCROLL; break;
        case DISC_QUERY_HIRES_SCROLL:    d->state = DISC_QUERY_THUMBWHEEL;   break;
        default:
            d->state = DISC_DONE;
            d->done = true;
            break;
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
    if (d->state == DISC_QUERY_REPROG_CONTROLS)
        feature_id = 0x1B04;
    else if (d->state == DISC_QUERY_HIRES_SCROLL)
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

/* Minimal mouse report layout for conversion.
 * MUST match structs.h mouse_report_t layout exactly (TU_ATTR_PACKED). */
typedef struct TU_ATTR_PACKED {
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

/* Map CID to mouse button bit. Returns 0 if not a mouse button. */
static uint8_t cid_to_button_bit(uint8_t cid_lo) {
    switch (cid_lo) {
    case 0x50: return 0x01; /* Left */
    case 0x51: return 0x02; /* Right */
    case 0x52: return 0x04; /* Middle */
    case 0x53: return 0x08; /* Back */
    case 0x56: return 0x10; /* Forward */
    default:   return 0;
    }
}

/* Auto-learn feature indices from observed events instead of IRoot.
 * Called on each sw_id=0 event; updates discovery if pattern matches. */
/* Auto-learn ReprogControls feature index from observed button events.
 * HiResScroll/Thumbwheel are NOT auto-learned — they work via interface 1. */
static void autolearn_feature(hidpp_discovery_t *d, uint8_t fi, uint8_t fn,
                              const uint8_t *params, uint16_t params_len) {
    if (fn == 2 && params_len >= 3 && params[0] == 0x00 && cid_to_button_bit(params[1])) {
        if (d->fi_reprog_controls != fi) {
            d->fi_reprog_controls = fi;
            dh_debug_printf("[AUTO] ReprogControls → 0x%02X\n", fi);
        }
    }
}

bool passthrough_convert_hidpp_to_mouse(const passthrough_state_t *state,
                                        const uint8_t *report, uint16_t len,
                                        void *out_mouse) {
    if (!state || !report || !out_mouse || len < 7)
        return false;

    /* Allow non-const access for auto-learn and button state */
    hidpp_discovery_t *d = (hidpp_discovery_t *)&state->hidpp_disc;

    uint8_t feature_idx = report[2];
    uint8_t fn = (report[3] >> 4) & 0x0F;
    const uint8_t *params = &report[4];
    uint16_t params_len = len - 4;
    passthrough_mouse_report_t *m = (passthrough_mouse_report_t *)out_mouse;

    /* Auto-learn feature indices from event patterns */
    autolearn_feature(d, feature_idx, fn, params, params_len);

    /* ReprogControls V4 analyticsKeyEvent (fn=2):
     * [rid, dev, fi, fn|sw, 0x00, cid_lo, action, 0x00, ...]
     * action: 0x01=press, 0x00=release.
     * Only updates button_state; caller handles report generation. */
    if (feature_idx == d->fi_reprog_controls && fn == 2 && params_len >= 3) {
        uint8_t cid_lo = params[1];
        uint8_t action = params[2];
        uint8_t bit = cid_to_button_bit(cid_lo);
        if (bit) {
            if (action)
                d->button_state |= bit;
            else
                d->button_state &= ~bit;
        }
        if (state->hidpp_scan.pipe_debug_enabled)
            dh_debug_printf("[P2] fn2 cid=0x%02X act=%d bit=0x%02X → btn=0x%02X\n",
                            cid_lo, action, bit, d->button_state);
        return false;
    }

    /* ReprogControls V4 divertedButtonsEvent (fn=0):
     * [rid, dev, fi, fn|sw, cid1_hi, cid1_lo, cid2_hi, cid2_lo, ...]
     * Bitmap of ALL currently pressed CIDs. */
    if (feature_idx == d->fi_reprog_controls && fn == 0 && params_len >= 2) {
        uint8_t buttons = 0;
        int max_cids = (int)(params_len / 2);
        if (max_cids > 4) max_cids = 4;
        for (int i = 0; i < max_cids; i++) {
            uint16_t cid = (uint16_t)((params[i * 2] << 8) | params[i * 2 + 1]);
            if (cid == 0) continue;
            buttons |= cid_to_button_bit((uint8_t)(cid & 0xFF));
        }
        d->button_state = buttons;
        return false; /* button_state updated; merged via output_mouse_report */
    }

    /* HiResScroll event (fn=0): params[0]=flags, params[1-2]=deltaV */
    if (d->fi_hires_scroll && feature_idx == d->fi_hires_scroll && fn == 0) {
        int16_t delta_v = (int16_t)((params[1] << 8) | params[2]);
        m->wheel = clamp_i8(delta_v);
        return true;
    }

    /* Thumbwheel event (fn=0): horizontal scroll delta */
    if (d->fi_thumbwheel && feature_idx == d->fi_thumbwheel && fn == 0) {
        int16_t delta_h = (int16_t)((params[1] << 8) | params[2]);
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

/* ================================================== *
 * ======  HID++ Protocol Scan (Debug Hotkey)  ====== *
 * ================================================== */

/* Send IFeatureSet.getFeatureID(index) — feature index 1, function 1.
 * Request:  [rid, devIdx, 0x01, (fn=1<<4 | swId), featureIndex, 0, 0]
 * Response: [rid, devIdx, 0x01, (fn=1<<4 | swId), featureID_hi, featureID_lo, featureType] */
static bool send_ifeatureset_query(passthrough_state_t *state,
                                   uint8_t device_idx, uint8_t feature_index) {
    if (!state || state->out_queue.pending)
        return false;

    for (uint8_t i = 0; i < state->iface_count; i++) {
        if (state->ifaces[i].always_passthrough) {
            state->out_queue.dev_addr = state->ifaces[i].dev_addr;
            state->out_queue.instance = state->ifaces[i].instance;
            break;
        }
    }
    state->out_queue.report_id   = HIDPP_REPORT_ID_SHORT;
    state->out_queue.report_type = 2;
    state->out_queue.data[0]     = device_idx;
    state->out_queue.data[1]     = 0x01; /* IFeatureSet is always at index 1 */
    state->out_queue.data[2]     = (0x01 << 4) | HIDPP_SWID_DESKHOP; /* fn=1 | sw_id */
    state->out_queue.data[3]     = feature_index;
    state->out_queue.data[4]     = 0x00;
    state->out_queue.data[5]     = 0x00;
    state->out_queue.len         = 6;
    state->out_queue.pending     = true;
    return true;
}

void passthrough_start_hidpp_scan(passthrough_state_t *state) {
    if (!state || !state->active)
        return;

    hidpp_scan_t *s = &state->hidpp_scan;

    /* Toggle raw dump mode */
    s->raw_dump_enabled = !s->raw_dump_enabled;
    dh_debug_printf("\n[SCAN] Raw HID++ dump: %s\n", s->raw_dump_enabled ? "ON" : "OFF");

    /* Start full feature scan for device 1, then device 2 */
    s->device_idx    = 1;
    s->next_device   = 2;
    s->query_idx     = 0;
    s->feature_count = HIDPP_SCAN_MAX_FEATURES;
    s->query_sent_us = 0;
    s->state         = SCAN_QUERY_IROOT;

    dh_debug_printf("[SCAN] Starting IFeatureSet scan for device %d...\n", s->device_idx);
    dh_debug_printf("[SCAN]  idx → featureID (type)\n");
}

void passthrough_scan_step(passthrough_state_t *state) {
    if (!state)
        return;

    hidpp_scan_t *s = &state->hidpp_scan;
    if (s->state == SCAN_IDLE || s->state == SCAN_DONE)
        return;

#ifndef UNIT_TEST
    uint64_t now = time_us_64();
#else
    uint64_t now = 0;
#endif

    /* Timeout: skip to next query after 500ms */
    if (s->query_sent_us > 0 && (now - s->query_sent_us) > 500000) {
        dh_debug_printf("[SCAN]  %2d → TIMEOUT\n", s->query_idx);
        s->query_idx++;
        s->query_sent_us = 0;
    }

    if (s->query_idx >= s->feature_count) {
        /* Move to next device or finish */
        if (s->next_device > 0) {
            s->device_idx    = s->next_device;
            s->next_device   = 0;
            s->query_idx     = 0;
            s->feature_count = HIDPP_SCAN_MAX_FEATURES;
            s->query_sent_us = 0;
            dh_debug_printf("\n[SCAN] Scanning device %d...\n", s->device_idx);
            dh_debug_printf("[SCAN]  idx → featureID (type)\n");
            return;
        }
        s->state = SCAN_DONE;
        dh_debug_printf("[SCAN] === Scan complete ===\n\n");
        return;
    }

    if (s->query_sent_us > 0)
        return; /* waiting for response */

    if (send_ifeatureset_query(state, s->device_idx, s->query_idx)) {
        s->query_sent_us = now ? now : 1;
    }
}

void passthrough_handle_scan_response(passthrough_state_t *state,
                                      const uint8_t *report, uint16_t len) {
    if (!state || len < 7)
        return;

    hidpp_scan_t *s = &state->hidpp_scan;
    if (s->state != SCAN_QUERY_IROOT)
        return;

    /* Response from IFeatureSet.getFeatureID:
     * report[4] = featureID high, report[5] = featureID low, report[6] = type */
    uint16_t feature_id = (uint16_t)((report[4] << 8) | report[5]);
    uint8_t  ftype      = report[6];

    if (feature_id == 0 && s->query_idx > 0) {
        /* End of feature table — no more features */
        dh_debug_printf("[SCAN]  %2d → (end)\n", s->query_idx);
        s->feature_count = s->query_idx;
    } else {
        dh_debug_printf("[SCAN]  %2d → 0x%04X (type=%d)\n", s->query_idx, feature_id, ftype);
    }

    s->query_idx++;
    s->query_sent_us = 0;
}

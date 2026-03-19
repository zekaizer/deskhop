/*
 * Semi-DDM USB Passthrough — descriptor capture and state management.
 */
#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#define MAX_PASSTHROUGH_IFACES 6
#define MAX_HID_DESC_SIZE      512
#define MAX_CONFIG_DESC_SIZE   280

/* Device-side passthrough interface base (after DeskHop's ITF 0, 1) */
#define ITF_NUM_PT_BASE        2
/* Device-side passthrough endpoint base (IN direction) */
#define EPNUM_PT_BASE          0x83

typedef struct {
    uint8_t  dev_addr;
    uint8_t  instance;
    uint8_t  itf_protocol;
    uint16_t desc_len;
    uint8_t  desc[MAX_HID_DESC_SIZE];
    bool     always_passthrough;
} passthrough_iface_t;

typedef struct {
    uint8_t              iface_count;
    passthrough_iface_t  ifaces[MAX_PASSTHROUGH_IFACES];
    bool                 enumeration_done;

    /* Device-side passthrough state (P2) */
    bool                 active;
    uint8_t              config_desc[MAX_CONFIG_DESC_SIZE];
    uint16_t             config_desc_len;
    uint64_t             last_capture_us;   /* Timestamp of most recent capture */
    uint64_t             reconnect_at_us;   /* Scheduled tud_connect() time (0=none) */

    /* Upstream device identity for VID/PID switching (FR-PT-009) */
    uint16_t             upstream_vid;
    uint16_t             upstream_pid;

    /* Deferred HID++ output report (sent from main loop, not callback) */
    struct {
        uint8_t  dev_addr;
        uint8_t  instance;
        uint8_t  report_id;
        uint8_t  report_type;
        uint8_t  data[32];
        uint16_t len;
        bool     pending;
    } out_queue;
} passthrough_state_t;

passthrough_state_t *passthrough_get_state(void);
void passthrough_init(passthrough_state_t *state);
bool passthrough_capture_descriptor(passthrough_state_t *state,
                                    uint8_t dev_addr,
                                    uint8_t instance,
                                    uint8_t itf_protocol,
                                    uint8_t const *desc_report,
                                    uint16_t desc_len);
void passthrough_remove_device(passthrough_state_t *state, uint8_t dev_addr);
void passthrough_dump_descriptors(const passthrough_state_t *state);

/* P2: Device-side descriptor generation and instance mapping.
 * passthrough_build_config_desc is declared here but implemented in usb_descriptors.c
 * (firmware build) or as a stub in passthrough.c (UNIT_TEST build). */
bool passthrough_activate(passthrough_state_t *state);
void passthrough_build_config_desc(passthrough_state_t *state);

const uint8_t *passthrough_get_report_desc(const passthrough_state_t *state,
                                           uint8_t device_instance,
                                           uint16_t *out_len);
int8_t passthrough_host_to_device_instance(const passthrough_state_t *state,
                                           uint8_t dev_addr,
                                           uint8_t instance);
int8_t passthrough_device_to_host_index(const passthrough_state_t *state,
                                        uint8_t device_instance);
bool passthrough_is_hidpp_input_event(const uint8_t *report, uint16_t len);

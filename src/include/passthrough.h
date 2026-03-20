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

/* HID++ IRoot feature discovery (P3) */
#define HIDPP_SWID_DESKHOP     0x0F
#define HIDPP_REPORT_ID_SHORT  0x10
#define HIDPP_REPORT_ID_LONG   0x11
#define HIDPP_DISC_TIMEOUT_US  2000000  /* 2 seconds */

enum hidpp_disc_state {
    DISC_IDLE = 0,
    DISC_DETECT_DEVICE,
    DISC_QUERY_REPROG_CONTROLS,
    DISC_QUERY_HIRES_SCROLL,
    DISC_QUERY_THUMBWHEEL,
    DISC_DONE
};

typedef struct {
    uint8_t  state;              /* enum hidpp_disc_state */
    uint8_t  device_idx;         /* HID++ device index (byte[1]), 0=undetected */
    uint8_t  fi_reprog_controls; /* Feature ID 0x1B04 → feature index (0=not found) */
    uint8_t  fi_hires_scroll;    /* Feature ID 0x2121 → feature index (0=not found) */
    uint8_t  fi_thumbwheel;      /* Feature ID 0x2150 → feature index */
    uint8_t  button_state;       /* Accumulated mouse button bitmap */
    bool     done;
    uint64_t query_sent_us;      /* timeout detection */

    /* Passive sniffing: learn feature indices from host setup commands */
} hidpp_discovery_t;

typedef struct {
    uint8_t  dev_addr;
    uint8_t  instance;
    uint8_t  itf_protocol;
    uint16_t desc_len;
    uint8_t  desc[MAX_HID_DESC_SIZE];
    bool     always_passthrough;
} passthrough_iface_t;

/* HID++ protocol analysis (debug hotkey) */
#define HIDPP_SCAN_MAX_FEATURES 32

enum hidpp_scan_state {
    SCAN_IDLE = 0,
    SCAN_QUERY_IROOT,      /* Query IRoot for each feature index */
    SCAN_DONE
};

typedef struct {
    uint8_t  state;                          /* enum hidpp_scan_state */
    uint8_t  device_idx;                     /* Current device being scanned */
    uint8_t  next_device;                    /* Next device to scan (0=done) */
    uint8_t  query_idx;                      /* Current feature index being queried */
    uint8_t  feature_count;                  /* Total features found */
    uint64_t query_sent_us;
    bool     raw_dump_enabled;               /* Toggle for raw event hex dump */
    bool     pipe_debug_enabled;             /* Toggle for pipeline verification log */
} hidpp_scan_t;

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

    /* P3: IRoot feature discovery */
    hidpp_discovery_t    hidpp_disc;

    /* HID++ protocol scan (debug) */
    hidpp_scan_t         hidpp_scan;
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

/* P3: IRoot discovery and HID++ conversion */
bool passthrough_send_iroot_query(passthrough_state_t *state,
                                  uint8_t device_idx, uint16_t feature_id);
void passthrough_handle_iroot_response(passthrough_state_t *state,
                                       const uint8_t *report, uint16_t len);
void passthrough_discovery_step(passthrough_state_t *state);
bool passthrough_convert_hidpp_to_mouse(const passthrough_state_t *state,
                                        const uint8_t *report, uint16_t len,
                                        void *out_mouse);

void passthrough_start_hidpp_scan(passthrough_state_t *state);
void passthrough_scan_step(passthrough_state_t *state);
void passthrough_handle_scan_response(passthrough_state_t *state,
                                      const uint8_t *report, uint16_t len);

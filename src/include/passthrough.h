/*
 * Semi-DDM USB Passthrough — descriptor capture and state management.
 */
#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#define MAX_PASSTHROUGH_IFACES 6
#define MAX_HID_DESC_SIZE      512

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

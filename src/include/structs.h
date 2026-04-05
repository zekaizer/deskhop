/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, version 3.
 *
 * See the file LICENSE for the full license text.
 */
#pragma once

#include <stdint.h>
#include <stdbool.h>
#include "flash.h"
#include "packet.h"
#include "screen.h"
#include "constants.h"

/* TU_ATTR_PACKED: use __attribute__((packed)) when TinyUSB is not included */
#ifndef TU_ATTR_PACKED
#define TU_ATTR_PACKED __attribute__((packed))
#endif

/* Opaque SDK types — size/alignment verified by _Static_assert in sdk_verify.h.
 * C code accesses actual SDK types via inline accessors in sdk_verify.h. */
#define QUEUE_OPAQUE_SIZE  16
#define QUEUE_OPAQUE_ALIGN 4
typedef struct __attribute__((aligned(QUEUE_OPAQUE_ALIGN))) {
    uint8_t _data[QUEUE_OPAQUE_SIZE];
} queue_opaque_t;

#define HID_KBD_REPORT_SIZE 8
typedef struct TU_ATTR_PACKED {
    uint8_t modifier;
    uint8_t reserved;
    uint8_t keycode[6];
} hid_kbd_report_t;

/* Constants from hid_parser.h — duplicated here to avoid SDK dependency chain.
 * Verified by _Static_assert in sdk_verify.h. */
#ifndef MAX_DEVICES
#define MAX_DEVICES    4
#endif
#ifndef MAX_INTERFACES
#define MAX_INTERFACES 4
#endif

/* hid_interface_t — opaque placeholder. Actual type in hid_parser.h (SDK-dependent).
 * Size/alignment verified by _Static_assert in sdk_verify.h. */
#define HID_INTERFACE_OPAQUE_SIZE  932
#define HID_INTERFACE_OPAQUE_ALIGN 4
typedef struct __attribute__((aligned(HID_INTERFACE_OPAQUE_ALIGN))) {
    uint8_t _data[HID_INTERFACE_OPAQUE_SIZE];
} hid_iface_opaque_t;

typedef void (*action_handler_t)();

typedef struct { // Maps message type -> message handler function
    enum packet_type_e type;
    action_handler_t handler;
} uart_handler_t;

typedef struct {
    uint8_t modifier;                 // Which modifier is pressed
    uint8_t keys[KEYS_IN_USB_REPORT]; // Which keys need to be pressed
    uint8_t key_count;                // How many keys are pressed
    action_handler_t action_handler;  // What to execute when the key combination is detected
    bool pass_to_os;                  // True if we are to pass the key to the OS too
    bool acknowledge;                 // True if we are to notify the user about registering keypress
} hotkey_combo_t;

typedef struct TU_ATTR_PACKED {
    uint8_t buttons;
    int16_t x;
    int16_t y;
    int8_t wheel;
    int8_t pan;
    uint8_t mode;
} mouse_report_t;

typedef struct {
    uint8_t tip_pressure;
    uint8_t buttons; // Digitizer buttons
    uint16_t x;      // X coordinate (0-32767)
    uint16_t y;      // Y coordinate (0-32767)
} touch_report_t;

typedef struct {
    uint8_t instance;
    uint8_t report_id;
    uint8_t type;
    uint8_t len;
    uint8_t data[RAW_PACKET_LENGTH];
} hid_generic_pkt_t;

typedef enum { IDLE, READING_PACKET, PROCESSING_PACKET } receiver_state_t;

typedef struct {
    uint32_t address;         // Address we're sending to the other box
    uint32_t checksum;
    uint16_t version;
    bool byte_done;           // Has the byte been successfully transferred
    bool upgrade_in_progress; // True if firmware transfer from the other box is in progress
} fw_upgrade_state_t;

typedef struct {
    uint32_t magic_header;
    uint32_t version;

    uint8_t force_mouse_boot_mode;
    uint8_t force_kbd_boot_protocol;

    uint8_t kbd_led_as_indicator;
    uint8_t hotkey_toggle;
    uint8_t enable_acceleration;

    uint8_t enforce_ports;
    uint16_t jump_threshold;

    output_t output[NUM_SCREENS];

    // Semi-DDM passthrough settings
    uint8_t passthrough_enabled;
    uint8_t gaming_mode_default;
    uint16_t _reserved;
    uint32_t smartshift_double_click_ms;

    // Keep checksum at the end of the struct
    uint32_t checksum;
} config_t;


/*==============================================================================
 *  Device Sub-structs — independent global state groups
 *==============================================================================*/

/* HID input state (keyboard/mouse) */
typedef struct {
    uint8_t kbd_dev_addr;                            // Address of the keyboard device
    uint8_t kbd_instance;                            // Keyboard instance
    hid_kbd_report_t local_kbd_states[MAX_DEVICES];  // Per-device keyboard states
    hid_kbd_report_t remote_kbd_state;               // Combined remote keyboard state
    uint8_t max_kbd_idx;                             // Largest kbd_idx seen
    int16_t pointer_x;                               // Mouse pointer X
    int16_t pointer_y;                               // Mouse pointer Y
    int16_t mouse_buttons;                           // Mouse button state
} device_hid_t;

/* device_config_t is Rust-only — defined in src-rust/src/domain/structs.rs */

/* Firmware upgrade state */
typedef struct {
    fw_upgrade_state_t fw;                // Upgrade state machine
    firmware_metadata_t _running_fw;      // RAM copy of running fw metadata
    bool reboot_requested;                // If set, stop updating watchdog
    uint8_t page_buffer[FLASH_PAGE_SIZE]; // Shared buffer for flash writes
} device_fw_t;

/* Onboard LED blinky */
typedef struct {
    int32_t blinks_left;     // Remaining blink transitions
    int32_t last_led_change; // Timestamp of last LED state change
    uint8_t led_blink_mode;  // 0=none, 1=PT_WAIT slow pulse
} device_led_t;

#define LED_BLINK_NONE     0
#define LED_BLINK_PT_WAIT  1

/* Hardware / SDK-dependent state (C-only, not bindgen-able) */
typedef struct {
    queue_opaque_t hid_queue_out; // Outgoing HID messages
    queue_opaque_t kbd_queue;     // Keyboard reports
    queue_opaque_t mouse_queue;   // Mouse reports
    queue_opaque_t uart_tx_queue; // Outgoing UART packets

    hid_iface_opaque_t iface[MAX_DEVICES][MAX_INTERFACES]; // HID interfaces (opaque)
    uart_packet_t in_packet;                                // Incoming UART packet

    /* DMA */
    uint32_t dma_ptr;             // DMA ring buffer position
    uint32_t dma_rx_channel;
    uint32_t dma_control_channel;
    uint32_t dma_tx_channel;
} device_hw_t;

enum os_type_e {
    LINUX   = 1,
    MACOS   = 2,
    WINDOWS = 3,
    ANDROID = 4,
    OTHER   = 255,
};

enum screen_pos_e {
    NONE   = 0,
    LEFT   = 1,
    RIGHT  = 2,
    MIDDLE = 3,
};

enum screensaver_mode_e {
    DISABLED   = 0,
    PONG       = 1,
    JITTER     = 2,
    MAX_SS_VAL = JITTER,
};

extern const config_t default_config;
extern const config_t ADDR_CONFIG[];
extern const uint8_t ADDR_FW_METADATA[];
extern const uint8_t ADDR_FW_RUNNING[];
extern const uint8_t ADDR_FW_STAGING[];
extern const uint8_t ADDR_DISK_IMAGE[];

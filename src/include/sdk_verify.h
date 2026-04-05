/* Compile-time verification that opaque types in structs.h match actual SDK types.
 * Include this AFTER both structs.h and the actual SDK headers (via main.h). */
#pragma once

#include <pico/util/queue.h>

/* queue_opaque_t must match queue_t in size and alignment */
_Static_assert(sizeof(queue_t) == QUEUE_OPAQUE_SIZE,
    "queue_t size changed — update QUEUE_OPAQUE_SIZE in structs.h");
_Static_assert(_Alignof(queue_t) == QUEUE_OPAQUE_ALIGN,
    "queue_t alignment changed — update QUEUE_OPAQUE_ALIGN in structs.h");

/* hid_kbd_report_t must match TinyUSB hid_keyboard_report_t */
_Static_assert(sizeof(hid_kbd_report_t) == HID_KBD_REPORT_SIZE,
    "hid_kbd_report_t size mismatch");

/* FLASH constants: flash.h uses __has_include to pull SDK values when available.
 * Verify the values are what we expect. */
_Static_assert(FLASH_PAGE_SIZE == 256,
    "FLASH_PAGE_SIZE changed — update flash.h fallback");

/* hid_iface_opaque_t must match hid_interface_t in size and alignment */
_Static_assert(sizeof(hid_interface_t) == HID_INTERFACE_OPAQUE_SIZE,
    "hid_interface_t size changed — update HID_INTERFACE_OPAQUE_SIZE in structs.h");
_Static_assert(_Alignof(hid_interface_t) == HID_INTERFACE_OPAQUE_ALIGN,
    "hid_interface_t alignment changed — update HID_INTERFACE_OPAQUE_ALIGN in structs.h");

/* Verify constants match hid_parser.h values (structs.h uses #ifndef fallbacks) */
#include "hid_parser.h"
_Static_assert(MAX_DEVICES == 4,
    "MAX_DEVICES changed — update structs.h fallback");
_Static_assert(MAX_INTERFACES == 4,
    "MAX_INTERFACES changed — update structs.h fallback");

_Static_assert(FLASH_SECTOR_SIZE == 4096,
    "FLASH_SECTOR_SIZE changed — update flash.h fallback");

/* Accessor helpers — cast opaque to real SDK type */
static inline queue_t* queue_from_opaque(queue_opaque_t *opaque) {
    return (queue_t *)opaque;
}

static inline hid_interface_t* iface_from_opaque(hid_iface_opaque_t *opaque) {
    return (hid_interface_t *)opaque;
}

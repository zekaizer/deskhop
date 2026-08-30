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

/* When included from SDK-free context (bindgen), provide fallback constants.
 * When SDK is available, sdk_verify.h verifies they match. */
#if __has_include(<hardware/flash.h>)
#include <hardware/flash.h>
#else
#define FLASH_PAGE_SIZE   256
#define FLASH_SECTOR_SIZE 4096
#endif

/*==============================================================================
 *  Firmware Metadata
 *==============================================================================*/

typedef struct {
    uint32_t magic;
    uint16_t version;
    uint32_t checksum;
} firmware_metadata_t;

extern firmware_metadata_t _firmware_metadata;
#define FIRMWARE_METADATA_MAGIC   0xf00d

/* Per-tick fw-upgrade receiver decision, computed by the unit-tested Rust step
 * machine (service::fw_upgrade::next_step). Must match the Rust #[repr(C)]. */
typedef struct {
    uint8_t  write_page;       /* flush the completed page at page_offset */
    uint8_t  erase_sector;     /* erase the 4KB sector first */
    uint8_t  finalize;         /* all bytes in: verify CRC + promote/reboot */
    uint8_t  _pad;
    uint32_t page_offset;      /* image offset of the page to flush */
    uint32_t request_address;  /* next address to request when not finalizing */
} fw_step_t;
extern void rust_fw_next_step(uint32_t address, fw_step_t *out);
/* Stall recovery while waiting for a ResponseByte: nonzero = re-send the
 * RequestByte for the current address (service::fw_upgrade::stall_tick). */
extern uint32_t rust_fw_stall_tick(void);

/* Per-block USB-MSC UF2 write decision (config-mode drag-drop upgrade), computed
 * by the unit-tested Rust step machine (service::fw_upgrade::msc_write_step).
 * Must match the Rust #[repr(C)]. */
typedef struct {
    uint8_t  is_uf2;          /* valid UF2 magic — else ignore the block */
    uint8_t  is_first;        /* blockNo 0: (re)init running checksum + flag */
    uint8_t  accumulate_crc;  /* payload is inside the CRC-protected region */
    uint8_t  is_final;        /* last block: finalize CRC, verify, reboot/recover */
    uint32_t flash_offset;    /* image offset of this page (add running base) */
} msc_step_t;
extern void rust_msc_write_step(uint32_t block_no, uint32_t magic0, uint32_t magic1,
                                uint32_t magic_end, msc_step_t *out);

/*==============================================================================
 *  Firmware Transfer Packet
 *==============================================================================*/

typedef struct {
    uint8_t cmd;          // Byte 0 = command
    uint16_t page_number; // Bytes 1-2 = page number
    union {
        uint8_t offset;   // Byte 3 = offset
        uint8_t checksum; // In write packets, it's checksum
    };
    uint8_t data[4]; // Bytes 4-7 = data
} fw_packet_t;

/*==============================================================================
 *  Flash Memory Layout
 *==============================================================================*/

#define RUNNING_FIRMWARE_SLOT     0
#define STAGING_FIRMWARE_SLOT     1
#define STAGING_PAGES_CNT         1024
#define STAGING_IMAGE_SIZE        STAGING_PAGES_CNT * FLASH_PAGE_SIZE

/*==============================================================================
*  Lookup Tables
*==============================================================================*/

/* crc32_lookup_table removed — now internal to Rust crc module */

/*==============================================================================
 *  UF2 Firmware Format Structure
 *==============================================================================*/
typedef struct {
    uint32_t magicStart0;
    uint32_t magicStart1;
    uint32_t flags;
    uint32_t targetAddr;
    uint32_t payloadSize;
    uint32_t blockNo;
    uint32_t numBlocks;
    uint32_t fileSize;
    uint8_t data[476];
    uint32_t magicEnd;
} uf2_t;

#define UF2_MAGIC_START0 0x0A324655
#define UF2_MAGIC_START1 0x9E5D5157
#define UF2_MAGIC_END    0x0AB16F30

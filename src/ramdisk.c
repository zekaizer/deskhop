/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 * Based on the TinyUSB example by Ha Thach.
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, version 3.
 *
 * See the file LICENSE for the full license text.
 */

#include "main.h"

#define NUMBER_OF_BLOCKS 4096
#define ACTUAL_NUMBER_OF_BLOCKS 128
#define BLOCK_SIZE       512

void tud_msc_inquiry_cb(uint8_t lun, uint8_t vendor_id[8], uint8_t product_id[16], uint8_t product_rev[4]) {
    strcpy((char *)vendor_id, "DeskHop");
    strcpy((char *)product_id, "Config Mode");
    strcpy((char *)product_rev, "1.0");
}

bool tud_msc_test_unit_ready_cb(uint8_t lun) {
    return true;
}

void tud_msc_capacity_cb(uint8_t lun, uint32_t *block_count, uint16_t *block_size) {
    *block_count = NUMBER_OF_BLOCKS;
    *block_size  = BLOCK_SIZE;
}

bool tud_msc_start_stop_cb(uint8_t lun, uint8_t power_condition, bool start, bool load_eject) {
    return true;
}

/* Return the requested data, or -1 if out-of-bounds */
int32_t tud_msc_read10_cb(uint8_t lun, uint32_t lba, uint32_t offset, void *buffer, uint32_t bufsize) {
    const uint8_t *addr = &ADDR_DISK_IMAGE[lba * BLOCK_SIZE + offset];

    if (lba >= NUMBER_OF_BLOCKS)
        return -1;

    /* We lie about the image size - actually it's 64 kB, not 512 kB, so if we're out of bounds, return zeros */
    else if (lba >= ACTUAL_NUMBER_OF_BLOCKS)
        memset(buffer, 0x00, bufsize);

    else
        memcpy(buffer, addr, bufsize);

    return (int32_t)bufsize;
}

/* We're writable, so return true */
bool tud_msc_is_writable_cb(uint8_t lun) {
    return true;
}

/* Simple firmware write routine, we get 512-byte uf2 blocks with 256 byte payload */
int32_t tud_msc_write10_cb(uint8_t lun, uint32_t lba, uint32_t offset, uint8_t *buffer, uint32_t bufsize) {
    if (lba >= NUMBER_OF_BLOCKS)
        return -1;

    /* The UF2 block-number arithmetic (final block, page address, CRC-coverage
     * gate) is the unit-tested Rust step machine; C only does the flash writes
     * and the post-write CRC verify (which must read freshly-written flash). */
    uf2_t *uf2 = (uf2_t *)&buffer[0];
    msc_step_t s;
    rust_msc_write_step(uf2->blockNo, uf2->magicStart0, uf2->magicStart1, uf2->magicEnd, &s);

    /* If we're not detecting UF2 magic constants, we have nothing to do... */
    if (!s.is_uf2)
        return (int32_t)bufsize;

    /* Valid UF2 but blockNo past the image — refuse before touching flash */
    if (s.reject)
        return -1;

    if (s.is_first) {
        global_fw.fw.checksum = 0xffffffff;

        /* Make sure nobody else touches the flash during this operation, otherwise we get empty pages */
        global_fw.fw.upgrade_in_progress = true;
        dh_debug_printf("fw msc upload start (USB UF2 -> RUNNING)\n");
    }

    /* Update checksum continuously as blocks are being received (the last sector,
     * which holds the CRC itself, is excluded by s.accumulate_crc) */
    if (s.accumulate_crc)
        for (int i = 0; i < FLASH_PAGE_SIZE; i++)
            global_fw.fw.checksum = crc32_iter(global_fw.fw.checksum, buffer[32 + i]);

    write_flash_page((uint32_t)ADDR_FW_RUNNING + s.flash_offset - XIP_BASE, &buffer[32]);

    if (s.is_final) {
        global_fw.fw.checksum = ~global_fw.fw.checksum;

        /* If checksums don't match, overwrite first sector and rely on ROM bootloader for recovery */
        uint32_t calc = calculate_firmware_crc32();
        if (global_fw.fw.checksum != calc) {
            /* The silent killer: a corrupt UF2 erases boot2 and drops to BOOTSEL
             * with no clue why. Name the mismatch before we wipe + reboot. */
            dh_debug_printf("fw msc crc FAIL want=0x%08lx got=0x%08lx -> erase+bootloader\n",
                            (unsigned long)global_fw.fw.checksum, (unsigned long)calc);
            flash_range_erase((uint32_t)ADDR_FW_RUNNING - XIP_BASE, FLASH_SECTOR_SIZE);
            dh_enter_bootloader();
        }
        else {
            dh_debug_printf("fw msc crc ok crc=0x%08lx -> reboot\n", (unsigned long)global_fw.fw.checksum);
            global_fw.reboot_requested = true;
        }
    }

    /* Provide some visual indication that fw is being uploaded */
    toggle_led();
    watchdog_update();

    return (int32_t)bufsize;
}

/* This is a super-dumb, rudimentary disk, any other scsi command is simply rejected */
int32_t tud_msc_scsi_cb(uint8_t lun, uint8_t const scsi_cmd[16], void *buffer, uint16_t bufsize) {
    tud_msc_set_sense(lun, SCSI_SENSE_ILLEGAL_REQUEST, 0x20, 0x00);
    return -1;
}

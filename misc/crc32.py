import sys
import struct
import binascii

FLASH_SECTOR_SIZE = 4096
MAGIC_VALUE = 0xf00d

elf_filename = sys.argv[1]
output_filename = sys.argv[2]
version = sys.argv[3]

with open(elf_filename, 'r+b') as f:
    data = f.read()

    data = data[:-FLASH_SECTOR_SIZE]
    crc32_value = binascii.crc32(data) & 0xFFFFFFFF

with open(output_filename, 'wb') as f:
    # Match the C firmware_metadata_t layout: { uint32 magic; uint16 version;
    # uint32 checksum; }. uint32 alignment puts `checksum` at offset 8, so insert
    # 2 padding bytes after the u16 version (xx). Without this the firmware read
    # `checksum` from the wrong offset (it picked up the crc32's high half + pad),
    # so the reported/compared crc was not the real image CRC32.
    f.write(struct.pack('<IHxxI', MAGIC_VALUE, int(version), crc32_value))

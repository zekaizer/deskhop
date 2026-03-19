# HID++ Message Routing — Classification Rules

## HID++ 2.0 Message Structure

Logitech HID++ 2.0 uses two report formats on the vendor interface (interface 2):

| Field | Offset | Short (0x10) | Long (0x11) |
|-------|--------|--------------|-------------|
| Report ID | 0 | 0x10 | 0x11 |
| Device Index | 1 | 1 byte | 1 byte |
| Feature Index | 2 | 1 byte | 1 byte |
| Function ID | 3 (bits 7-4) | 4 bits | 4 bits |
| SW ID | 3 (bits 3-0) | 4 bits | 4 bits |
| Parameters | 4-6 | 3 bytes | 4-19 (16 bytes) |
| **Total** | | **7 bytes** | **20 bytes** |

## SW ID Classification

Byte 3 lower nibble (`report[3] & 0x0F`) distinguishes message origin:

| sw_id | Meaning | Example |
|-------|---------|---------|
| **0** | Unsolicited event (device → host) | Scroll, button press, battery change |
| **1-14** | Response to host query | Options+ feature discovery, config read/write |
| **15 (0x0F)** | Reserved (DeskHop will use for IRoot queries in P3) | Feature index discovery |

## Routing Decision Matrix

```
tuh_hid_report_received_cb
  └─ always_passthrough interface?
      ├─ YES (vendor/HID++)
      │   ├─ sw_id != 0 (protocol response)
      │   │   → Always forward to device side (Options+ on host A)
      │   ├─ sw_id == 0 (input event) + A active
      │   │   → Forward to device side
      │   └─ sw_id == 0 (input event) + B active
      │       → Drop (P3: convert to mouse_report_t via UART)
      └─ NO (mouse/keyboard passthrough)
          ├─ Active output board
          │   → Forward to device side (raw passthrough)
          └─ Inactive output board
              → Fall through to DeskHop parsing (UART relay)
```

## Observed Feature Index Table (Unifying C52B + MX Master 3S)

These indices are **device-specific** — the mapping between feature index and
feature ID is established dynamically via IRoot (feature 0x0000) at connection time.

| Feature Index | Feature ID (est.) | Name | Input Events? |
|--------------|-------------------|------|---------------|
| 0x00 | 0x0000 | IRoot | No |
| 0x02 | 0x0003 | IFeatureSet | No |
| 0x03 | 0x0005 | DeviceInfo | No |
| 0x04 | 0x1D4B | Connection | Yes (connect/disconnect) |
| 0x08 | 0x1000 | Battery | Yes (level change) |
| 0x09 | 0x1B04 | ReprogControls V4 | **Yes (button events)** |
| 0x0C | 0x2201 | DPI/Resolution | No |
| 0x0E | 0x2121 | HiResScroll | **Yes (scroll events)** |
| 0x0F | 0x2150 | Thumbwheel | **Yes (scroll/gesture)** |

## IRoot Query Protocol (P3 Reference)

To discover feature indices, query IRoot (always at index 0, function 0):

```
Request:  [0x10, devIdx, 0x00, (fn=0 << 4 | swId), featureID_hi, featureID_lo, 0x00]
Response: [0x10, devIdx, 0x00, (fn=0 << 4 | swId), featureIndex,  featureType,  0x00]
```

Example — discover HiRes Scroll (feature ID 0x2121):
```
TX: 10 01 00 0F 21 21 00    (swId=0x0F for DeskHop)
RX: 10 01 00 0F 0E 00 00    (featureIndex=0x0E, type=0)
```

## SET_REPORT Protocol Summary

See [hidpp-output-protocol.md](hidpp-output-protocol.md) for full details.

- TinyUSB `tuh_hid_set_report()` sends wLength=6 (wrong) → STALL
- Fix: `tuh_control_xfer` with wLength=7, data = [report_id + payload]
- Deferred from TinyUSB callback to main loop via `out_queue`

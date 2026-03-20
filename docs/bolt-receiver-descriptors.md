# Logitech Bolt Receiver — HID Descriptor Capture

Captured from Logitech Bolt USB Unifying Receiver (VID=0x046D, PID=0xC548)
via DeskHop `passthrough_dump_descriptors()` on 2026-03-19.

## Summary

| Interface | Protocol    | Length | always_pt | Description              |
|-----------|-------------|--------|-----------|--------------------------|
| 0         | Keyboard    | 59 B   | 0         | Standard HID keyboard    |
| 1         | Mouse       | 148 B  | 0         | Mouse + Consumer + System|
| 2         | NONE/Vendor | 93 B   | 1         | HID++ vendor (Logitech)  |

## Interface 0 — Keyboard (59 bytes)

```
0000: 05 01 09 06 A1 01 95 08 75 01 15 00 25 01 05 07
0010: 19 E0 29 E7 81 02 81 03 95 05 05 08 19 01 29 05
0020: 91 02 95 01 75 03 91 01 95 06 75 08 15 00 26 FF
0030: 00 05 07 19 00 2A FF 00 81 00 C0
```

Standard boot-compatible keyboard descriptor:
- 8-bit modifier bitmap (Left Ctrl .. Right GUI)
- 1 byte reserved
- 5 LED outputs (Num/Caps/Scroll/Compose/Kana)
- 6 key rollover array (Usage 0x00–0xFF)

## Interface 1 — Mouse (148 bytes)

```
0000: 05 01 09 02 A1 01 85 02 09 01 A1 00 95 10 75 01
0010: 15 00 25 01 05 09 19 01 29 10 81 02 95 02 75 0C
0020: 16 01 F8 26 FF 07 05 01 09 30 09 31 81 06 95 01
0030: 75 08 15 81 25 7F 09 38 81 06 95 01 05 0C 0A 38
0040: 02 81 06 C0 C0 05 0C 09 01 A1 01 85 03 95 02 75
0050: 10 15 01 26 FF 02 19 01 2A FF 02 81 00 C0 05 01
0060: 09 80 A1 01 85 04 95 01 75 02 15 01 25 03 09 82
0070: 09 81 09 83 81 00 75 06 81 03 C0 06 BC FF 09 88
0080: A1 01 85 08 95 01 75 08 15 01 26 FF 00 19 01 29
0090: FF 81 00 C0
```

Multi-report descriptor with 4 report IDs:
- **Report ID 0x02** — Mouse: 16 buttons, 12-bit X/Y (−2047..+2047), 8-bit wheel, AC pan
- **Report ID 0x03** — Consumer Control: 2× 16-bit usage selectors (0x01–0x2FF)
- **Report ID 0x04** — System Control: Sleep/Power Off/Power On (2-bit)
- **Report ID 0x08** — Vendor (Usage Page 0xFFBC): 1 byte

## Interface 2 — HID++ Vendor (93 bytes)

```
0000: 06 00 FF 09 01 A1 01 85 10 95 06 75 08 15 00 26
0010: FF 00 09 01 81 00 09 01 91 00 C0 06 00 FF 09 02
0020: A1 01 85 11 95 13 75 08 15 00 26 FF 00 09 02 81
0030: 00 09 02 91 00 C0 06 00 FF 09 04 A1 01 85 20 95
0040: 0E 75 08 15 00 26 FF 00 09 41 81 00 09 41 91 00
0050: 85 21 95 1F 09 42 81 00 09 42 91 00 C0
```

Logitech HID++ protocol (Usage Page 0xFF00):
- **Report ID 0x10** — Short HID++ (7 bytes: 6 data + 1 report ID)
- **Report ID 0x11** — Long HID++ (20 bytes: 19 data + 1 report ID)
- **Report ID 0x20** — DJ Short (15 bytes: 14 data + 1 report ID)
- **Report ID 0x21** — DJ Long (32 bytes: 31 data + 1 report ID)

All reports are bidirectional (Input + Output), enabling host ↔ device communication.
This interface is marked `always_passthrough = true` for Semi-DDM forwarding.

## HID++ Protocol Analysis (Device 1 — MX Mouse)

Captured via DeskHop `[OUT→RX]` and `[CVT]` debug logs on 2026-03-20.
Device index 0x01 = mouse, 0x02 = second paired device, 0xFF = receiver.

### IRoot Feature Index Map (Device 1)

Determined via autolearn from observed input events and Options+ output reports.

| Feature Index | Feature ID | Name               | Confirmed By                      |
|---------------|------------|--------------------|-----------------------------------|
| 0x02          | ?          | Unknown            | Options+ queries (fn=0,1,2)       |
| 0x05          | 0x0003?    | DeviceInfo?        | Options+ reads (fn=0,1)           |
| 0x09          | 0x1B04     | ReprogControls V4  | Autolearn from analyticsKeyEvent  |
| 0x0A          | ?          | Unknown            | Options+ configures (fn=0,1,3,7,8)|
| 0x0E          | 0x2121     | HiResScroll        | Autolearn from wheel events       |
| 0x0F          | 0x2150     | Thumbwheel         | Autolearn from thumbwheel events  |
| 0x10          | ?          | Unknown            | Options+ configures (fn=0,1,2)    |

### ReprogControls V4 (fi=0x09) — Button CIDs

| CID    | Button       | Bit  | Event Type               |
|--------|-------------|------|--------------------------|
| 0x0050 | Left Click  | 0x01 | fn=2 analyticsKeyEvent   |
| 0x0051 | Right Click | 0x02 | fn=2 analyticsKeyEvent   |
| 0x0052 | Middle Click| 0x04 | fn=2 analyticsKeyEvent   |
| 0x0053 | Back        | 0x08 | fn=2 analyticsKeyEvent   |
| 0x0056 | Forward     | 0x10 | fn=2 analyticsKeyEvent   |
| 0x00C3 | Gesture     | —    | fn=2 (unmapped in DeskHop)|
| 0x00C4 | SmartShift? | —    | fn=2 (unmapped in DeskHop)|

Button events use two HID++ event types:
- **fn=0 (divertedButtonsEvent)**: bitmap of ALL currently pressed CIDs
- **fn=2 (analyticsKeyEvent)**: individual press/release per CID

### HiResScroll (fi=0x0E) — Vertical Wheel

Event format (fn=0): `[rid, dev, 0x0E, 0x00, flags, deltaV_hi, deltaV_lo]`
- `flags`: scroll direction and mode bits
- `deltaV`: signed 16-bit vertical delta (clamped to int8 for standard HID)

**Options+ setWheelMode**: `[01 0E 2B 03]` = fn=2, param=0x03
- Switches wheel reporting from DJ (Interface 1) → HID++ (Interface 2)
- Enables high-resolution mode
- **Root cause of Android wheel loss**: DJ reports stop carrying wheel data

### Thumbwheel (fi=0x0F) — Horizontal Scroll

Event format (fn=0): `[rid, dev, 0x0F, 0x00, flags, deltaH_hi, deltaH_lo]`
- Same structure as HiResScroll but for horizontal axis

### Options+ Impact on DeskHop

When Logi Options+ detects the mouse (even without switching to Mac output):
1. Options+ sends `setWheelMode(0x03)` → wheel diverted from DJ to HID++
2. DJ Interface 1 mouse reports: wheel field becomes 0
3. HID++ Interface 2: HiResScroll/Thumbwheel events start arriving
4. DeskHop must convert these HID++ events to standard mouse reports for non-Options+ hosts

### Receiver Notifications

| Pattern                     | Meaning                        |
|-----------------------------|--------------------------------|
| `[FF 81 00 00]`            | Receiver keepalive/polling     |
| `[FF 83 B5 52]`            | Receiver device notification   |
| `[FF 80 00 00]`            | Receiver status                |
| device_idx 0xFF, fi=0x81   | Receiver-level event (not mouse — must be filtered from autolearn) |

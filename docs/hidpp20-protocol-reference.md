# HID++ 2.0 Protocol Reference

Compiled from official Logitech cpg-docs, Linux kernel driver (`hid-logitech-hidpp.c`),
Solaar source, logiops source (`PixlOne/logiops`), cvuchener/hidpp library,
and lekensteyn's reverse-engineered specs.

---

## 1. Transport Layer

### Report Format

All HID++ messages ride on standard HID reports. Byte 0 is the HID Report ID
(NOT part of the HID++ payload — USB HID layer strips it before delivery).

| Report ID | Name  | Total Size | Payload (params) |
|-----------|-------|-----------|-------------------|
| `0x10`    | Short | 7 bytes   | 4 bytes           |
| `0x11`    | Long  | 20 bytes  | 17 bytes          |

### Packet Layout (after Report ID)

```
Byte 0: Device Index
Byte 1: Feature Index
Byte 2: [Function ID (7:4)] [Software ID (3:0)]
Byte 3+: Parameters (4 bytes short / 17 bytes long)
```

- **Device Index**: Target device (0x01–0x06 for paired wireless, 0xFF for corded/receiver).
- **Feature Index**: Resolved via IRoot GetFeature. Index 0 = IRoot always.
- **Function ID** (bits 7:4): 0x0–0xF within a feature.
- **Software ID** (bits 3:0): Caller tag — response echoes this value for demuxing.
- Response mirrors request structure; errors use Feature Index `0xFF`.

### Error Response

```
Byte 0: Device Index
Byte 1: 0xFF (error marker)
Byte 2: Feature Index (of failed request)
Byte 3: [Function ID (7:4)] [Software ID (3:0)]
Byte 4: Error Code
```

| Error Code | Name                   |
|------------|------------------------|
| 0x00       | NoError                |
| 0x01       | Unknown                |
| 0x02       | InvalidArgument        |
| 0x03       | OutOfRange             |
| 0x04       | HWError                |
| 0x05       | LogitechInternal       |
| 0x06       | InvalidFeatureIndex    |
| 0x07       | InvalidFunctionID      |
| 0x08       | Busy                   |
| 0x09       | Unsupported            |

---

## 2. IRoot — Feature 0x0000

Always at feature index **0**. Required on all HID++ 2.0 devices.

### [Function 0] GetFeature

Resolves a 16-bit Feature ID to the device-local Feature Index.

**Request (Short):**
```
Param[0]: Feature ID MSB
Param[1]: Feature ID LSB
Param[2]: 0x00 (reserved)
```

**Response:**
```
Param[0]: Feature Index (0x00 = not found)
Param[1]: Flags + Type
           Bit 7: Obsolete (1 = deprecated)
           Bit 6: Hidden   (1 = internal/debug)
           Bits 5-0: Reserved
Param[2]: Feature Version (starts at 0, increments on backward-compat changes)
```

### [Function 1] GetProtocolVersion / Ping

**Request (Short):**
```
Param[0]: 0x00 (reserved)
Param[1]: 0x00 (reserved)
Param[2]: Ping data (echoed back)
```

**Response:**
```
Param[0]: Protocol Major version
           0x04 = HID++ 2.0 (current)
           0x02 = HID++ 2.0 (legacy encoding)
Param[1]: Protocol Minor version
Param[2]: Ping data (echo)
```

### [Event 0] NoOperation

Device-initiated keepalive to prevent low-power sleep. No payload. Host ignores.

---

## 3. IFeatureSet — Feature 0x0001

Enumerates all features supported by a device.

### [Function 0] GetCount

**Request:** No parameters.

**Response:**
```
Param[0]: Feature count (uint8_t, excludes IRoot itself)
```

### [Function 1] GetFeatureID

**Request:**
```
Param[0]: Feature index (1-based, 0 = IRoot not enumerable here)
```

**Response:**
```
Param[0-1]: Feature ID (uint16_t, big-endian)
Param[2]:   Flags
             Bit 7: Obsolete
             Bit 6: Hidden
             Bit 5: Internal
             Bits 4-0: Reserved
Param[3]:   Feature version (uint8_t)
```

### Options+ Enumeration Sequence

1. Ping via IRoot fn1 to confirm HID++ 2.0.
2. IRoot fn0 with Feature ID 0x0001 → get IFeatureSet index.
3. IFeatureSet fn0 → get count N.
4. Loop i = 1..N: IFeatureSet fn1(i) → get Feature ID, flags, version.
5. For each discovered feature: IRoot fn0(featureID) → resolve to feature index.
6. Interact with features using their resolved indices.

---

## 4. HiRes Wheel — Feature 0x2121

High-resolution vertical scroll wheel with SmartShift (ratchet/freewheel) support.

### [Function 0] GetCapabilities

**Response:**
```
Param[0]: Multiplier (uint8_t)
           Typical values: 8 (MX Master series), 15
           Represents hi-res counts per ratchet notch.
Param[1]: Capability flags
           Bit 3 (0x08): Invertible   — scroll direction can be inverted
           Bit 2 (0x04): HasRatchet   — ratchet/freewheel switch present
           Bits 1-0, 4-7: Reserved
```

### [Function 1] GetMode

**Response:**
```
Param[0]: Mode flags (uint8_t)
           Bit 2 (0x04): Inverted  — scroll direction is inverted
           Bit 1 (0x02): HiRes    — high-resolution reporting active
           Bit 0 (0x01): Target   — events sent to HID++ (vs native HID)
```

### [Function 2] SetMode

**Request:**
```
Param[0]: Mode flags (same layout as GetMode response)
           Bit 2 (0x04): Inverted
           Bit 1 (0x02): HiRes
           Bit 0 (0x01): Target
```

### [Function 3] GetRatchetState

**Response:**
```
Param[0]: Ratchet state
           0 = FreeWheel (free-spinning)
           1 = Ratchet   (notched/detent mode)
```

### [Event 0] WheelMovement

Triggered on each scroll tick. Sent as **Long report** (0x11).

```
Param[0]: Flags + Periods
           Bit 4 (0x10): HiRes flag — 1 if high-resolution data
           Bits 3-0 (0x0F): Periods — number of ratchet detents (notches)
Param[1]: Delta V MSB (int16_t, big-endian, signed)
Param[2]: Delta V LSB
```

- **HiRes mode** (bit 4 = 1): `deltaV` contains high-resolution counts.
  Actual notches = `deltaV / multiplier` (from GetCapabilities).
- **Lo-Res mode** (bit 4 = 0): `deltaV` = raw notch count (typically ±1 per detent).
- **Direction**: Sign of `deltaV` — positive = scroll up/away, negative = scroll down/toward.
- **Periods field**: Number of mechanical detents traversed (useful for ratchet mode timing).

### [Event 1] RatchetSwitch

Triggered when the user physically switches between ratchet and freewheel mode
(e.g., SmartShift toggle button on MX Master).

```
Param[0]: Ratchet state
           0 = FreeWheel
           1 = Ratchet
```

### Distinguishing Ratchet vs Free-Spin from Events

1. **RatchetSwitch event**: Direct notification of mode change.
2. **GetRatchetState (fn3)**: Poll current state.
3. **Heuristic from WheelMovement**: In ratchet mode, `periods > 0` and `deltaV` comes in
   discrete multiples of `multiplier`. In freewheel, `periods` may be 0 and `deltaV` streams
   continuously with fine granularity.

---

## 5. Thumbwheel — Feature 0x2150

Horizontal scroll wheel (e.g., MX Master side wheel).

### [Function 0] GetInfo

**Response:**
```
Param[0-1]: Native resolution (uint16_t BE)
             Typical: 18 (counts per full rotation in native mode)
Param[2-3]: Diverted resolution (uint16_t BE)
             Typical: 120 (counts per full rotation when diverted)
Param[4]:   Default direction
             0 = normal, 1 = inverted
Param[5]:   Capabilities (uint8_t)
             Bit 0 (0x01): Timestamp — timestamps in events
             Bit 1 (0x02): Touch    — touch detection
             Bit 2 (0x04): Proxy    — proximity sensing
             Bit 3 (0x08): SingleTap — tap recognition
Param[6-7]: Time elapsed (uint16_t BE, ms)
```

### [Function 1] GetStatus

**Response:**
```
Param[0]: Diverted (bool, full byte)
           0 = native HID reporting, 1 = diverted to HID++
Param[1]: Packed flags
           Bit 0: Inverted
           Bit 1: Touch active
           Bit 2: Proximity active
```

### [Function 2] SetReporting

**Request:** Same layout as GetStatus response.

### [Event 0] ThumbwheelEvent

Sent as **Long report** (0x11).

```
Param[0-1]: Rotation (int16_t BE, signed)
             Positive = right/clockwise, Negative = left/counter-clockwise
Param[2-3]: Timestamp (uint16_t BE, ms since last event, if capability bit set)
Param[4]:   Rotation status (enum)
             0 = Inactive
             1 = Start    — finger engaged
             2 = Active   — ongoing scroll
             3 = Stop     — finger released
Param[5]:   Flags (uint8_t, device-specific)
```

---

## 6. ReprogControls V4 — Feature 0x1B04

Programmable buttons/keys with diversion and remapping.

### [Function 0] GetControlCount

**Response:**
```
Param[0]: Count (uint8_t) — number of controls in CID table
```

### [Function 1] GetControlInfo

**Request:**
```
Param[0]: Index (0-based)
```

**Response (9 bytes, Long report):**
```
Param[0-1]: Control ID (CID, uint16_t BE)
Param[2-3]: Task ID (TID, uint16_t BE)
Param[4]:   Flags (ControlInfoFlags)
             Bit 0 (0x01): MouseButton
             Bit 1 (0x02): FKey
             Bit 2 (0x04): HotKey
             Bit 3 (0x08): FnToggle
             Bit 4 (0x10): ReprogHint (reprogrammable)
             Bit 5 (0x20): TemporaryDivertable
             Bit 6 (0x40): PersistentlyDivertable
             Bit 7 (0x80): Virtual
Param[5]:   Position (0 = N/A, 1-16 = F-key number)
Param[6]:   Group (0 = ungrouped, 1-8 = group membership)
Param[7]:   Group mask (bitmap: which groups this can remap to)
Param[8]:   Additional flags
             Bit 0 (0x01): RawXY capable
             Bit 1 (0x02): ForceRawXY (Solaar extension)
             Bit 2 (0x04): AnalyticsKeyEvents capable
             Bit 3 (0x08): RawWheel capable (Solaar: 0x800 in expanded flags)
```

### [Function 2] GetCidReporting

**Request:**
```
Param[0-1]: Control ID (uint16_t BE)
```

**Response:**
```
Param[0-1]: Control ID (echo)
Param[2]:   Reporting flags
             Bit 0 (0x01): TemporaryDiverted
             Bit 2 (0x04): PersistentlyDiverted
             Bit 4 (0x10): RawXYDiverted
Param[3-4]: Remap CID (uint16_t BE, 0 = self/no remap)
```

### [Function 3] SetCidReporting

**Request:**
```
Param[0-1]: Control ID (uint16_t BE)
Param[2]:   Flags (with change-valid bits)
             Bit 0: TemporaryDiverted (value)
             Bit 1: ChangeTemporaryDivert (valid bit)
             Bit 2: PersistentlyDiverted (value)
             Bit 3: ChangePersistentDivert (valid bit)
             Bit 4: RawXYDiverted (value)
             Bit 5: ChangeRawXYDivert (valid bit)
Param[3-4]: Remap target CID (uint16_t BE, 0 = keep current)
```

**Response:** Echoes request.

### [Event 0] DivertedButtonsEvent

Reports currently-pressed diverted buttons. Up to **4 CIDs** simultaneously.

```
Param[0-1]: CID 1 (uint16_t BE, 0x0000 = unused)
Param[2-3]: CID 2 (uint16_t BE, 0x0000 = unused)
Param[4-5]: CID 3 (uint16_t BE, 0x0000 = unused)
Param[6-7]: CID 4 (uint16_t BE, 0x0000 = unused)
```

When a diverted button is released, it disappears from the list.
When all diverted buttons are released, CID 1 = 0x0000.

### [Event 1] DivertedRawXYEvent

Mouse movement while a RawXY-diverted button is held.

```
Param[0-1]: dx (int16_t BE, signed)
Param[2-3]: dy (int16_t BE, signed)
```

### [Event 2] AnalyticsKeyEvent (V4+ extension)

Reported when `ANALYTICS_KEY_EVENTS_REPORTING` is enabled for a CID.
Format is not fully documented in public sources. Solaar defines capability
flag `0x400` (ANALYTICS_KEY_EVENTS) and mapping flag `0x100`
(ANALYTICS_KEY_EVENTS_REPORTING). This event reportedly carries per-key
press/release analytics with CID identification.

**Status:** Proprietary, incomplete public documentation.

### Common Mouse CIDs

| CID    | Name                        | Notes                           |
|--------|-----------------------------|---------------------------------|
| 0x0050 | Left_Button                 | Standard left click             |
| 0x0051 | Right_Button                | Standard right click            |
| 0x0052 | Middle_Button               | Wheel click                     |
| 0x0053 | Back_Button                 | Side button (thumb, back)       |
| 0x0056 | Forward_Button              | Side button (thumb, forward)    |
| 0x005B | Left_Tilt                   | Wheel tilt left                 |
| 0x005D | Right_Tilt                  | Wheel tilt right                |
| 0x00C3 | Mouse_Gesture_Button        | Gesture button (MX Master)      |
| 0x00C4 | Smart_Shift                 | SmartShift toggle               |
| 0x00D0 | MultiPlatform_Gesture_Button| Gesture (multi-host devices)    |
| 0x00D7 | Virtual_Gesture_Button      | Virtual gesture / switch receiver|
| 0x00ED | DPI_Change                  | DPI cycle button                |
| 0x00FD | DPI_Switch                  | DPI shift (held)                |
| 0x0102 | LeftAndRightClick           | Simultaneous L+R                |

### Extended CID Table (Keyboard/Media)

| CID    | Name              |
|--------|-------------------|
| 0x0001 | Volume_Up_old     |
| 0x0002 | Volume_Down_old   |
| 0x0003 | Mute              |
| 0x006E | Show_Desktop      |
| 0x006F | Lock_Screen       |
| 0x00BA | Switch_Apps       |
| 0x00E0 | Task_View         |
| 0x00E1 | Action_Center     |
| 0x00E4 | Previous_Track    |
| 0x00E5 | Play_Pause        |
| 0x00E6 | Next_Track        |
| 0x00E7 | Mute_Mic          |
| 0x00E8 | Volume_Down       |
| 0x00E9 | Volume_Up         |

### Solaar Expanded Key Flags (16-bit)

| Bit   | Value  | Name                        |
|-------|--------|-----------------------------|
| 0     | 0x0001 | MSE (mouse button)          |
| 1     | 0x0002 | IS_FN (function key)        |
| 2     | 0x0004 | NONSTANDARD                 |
| 3     | 0x0008 | FN_SENSITIVE                |
| 4     | 0x0010 | REPROGRAMMABLE              |
| 5     | 0x0020 | DIVERTABLE                  |
| 6     | 0x0040 | PERSISTENTLY_DIVERTABLE     |
| 7     | 0x0080 | VIRTUAL                     |
| 8     | 0x0100 | RAW_XY                      |
| 9     | 0x0200 | FORCE_RAW_XY                |
| 10    | 0x0400 | ANALYTICS_KEY_EVENTS        |
| 11    | 0x0800 | RAW_WHEEL                   |

### Solaar Expanded Mapping Flags (16-bit)

| Bit   | Value  | Name                             |
|-------|--------|----------------------------------|
| 0     | 0x0001 | DIVERTED                         |
| 2     | 0x0004 | PERSISTENTLY_DIVERTED            |
| 4     | 0x0010 | RAW_XY_DIVERTED                  |
| 6     | 0x0040 | FORCE_RAW_XY_DIVERTED            |
| 8     | 0x0100 | ANALYTICS_KEY_EVENTS_REPORTING   |
| 10    | 0x0400 | RAW_WHEEL                        |

---

## 7. Complete Feature ID Registry

Extracted from Logitech cpg-docs README.

### 00 — Important (System)
| ID       | Name                    |
|----------|-------------------------|
| `0x0000` | IRoot                   |
| `0x0001` | IFeatureSet             |
| `0x0002` | IFeatureInfo            |

### 01 — Common
| ID       | Name                          |
|----------|-------------------------------|
| `0x0003` | Device Information            |
| `0x0005` | Device Name and Type          |
| `0x0007` | Device Friendly Name          |
| `0x0008` | Keep-Alive                    |
| `0x0020` | Config Change                 |
| `0x00C2` | DFU Control Signed            |
| `0x00D0` | DFU                           |
| `0x1000` | Battery Unified Level Status  |
| `0x1001` | Battery Voltage               |
| `0x1814` | Change Host                   |
| `0x1B00` | ReprogControls V1             |
| `0x1B01` | ReprogControls V2             |
| `0x1B02` | ReprogControls V3             |
| `0x1B03` | ReprogControls V3.5           |
| `0x1B04` | ReprogControls V4             |
| `0x1D4B` | Wireless Device Status        |

### 02 — Mouse
| ID       | Name                    |
|----------|-------------------------|
| `0x2001` | Swap Left/Right Button  |
| `0x2100` | Vertical Scrolling      |
| `0x2110` | SmartShift Wheel        |
| `0x2120` | High-Resolution Scrolling (legacy) |
| `0x2121` | HiRes Wheel             |
| `0x2130` | Ratchet Wheel           |
| `0x2150` | Thumbwheel              |
| `0x2200` | Mouse Pointer           |
| `0x2201` | Adjustable DPI          |
| `0x2400` | Hybrid Tracking Engine  |

### 04 — Keyboard
| ID       | Name                    |
|----------|-------------------------|
| `0x40A2` | Fn Inversion (default)  |
| `0x40A3` | Fn Inversion (multi-host)|
| `0x4521` | Disable Keys            |
| `0x4600` | Crown                   |

### 06 — Touchpad
| ID       | Name                    |
|----------|-------------------------|
| `0x6100` | TouchPad Raw XY         |
| `0x6501` | Gestures 2              |

### 08 — Gaming
| ID       | Name                    |
|----------|-------------------------|
| `0x8060` | Adjustable Report Rate  |
| `0x8071` | RGB Effects             |
| `0x8100` | Onboard Profiles        |

---

## Sources

- [Logitech cpg-docs (official)](https://github.com/Logitech/cpg-docs/tree/master/hidpp20)
- [Logitech cpg-docs IRoot spec](https://github.com/Logitech/cpg-docs/blob/master/hidpp20/features/0x0000-IRoot.rst)
- [HID++ 2.0 Draft Specification (2012)](https://lekensteyn.nl/files/logitech/logitech_hidpp_2.0_specification_draft_2012-06-04.pdf)
- [lekensteyn x1B04 reverse-engineering](https://lekensteyn.nl/files/logitech/x1b04_specialkeysmsebuttons.html)
- [Linux kernel hid-logitech-hidpp.c](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-logitech-hidpp.c)
- [Solaar features list](https://pwr-solaar.github.io/Solaar/features/)
- [Solaar hidpp20.py](https://github.com/pwr-Solaar/Solaar/blob/master/lib/logitech_receiver/hidpp20.py)
- [Solaar special_keys.py (CID table)](https://github.com/pwr-Solaar/Solaar/blob/master/lib/logitech_receiver/special_keys.py)
- [logiops HIDPP 2.0 wiki](https://github.com/PixlOne/logiops/wiki/HIDPP--2.0)
- [logiops CID list](https://github.com/PixlOne/logiops/wiki/CIDs)
- [logiops HiresScroll.h](https://github.com/PixlOne/logiops/blob/main/src/logid/backend/hidpp20/features/HiresScroll.h)
- [logiops ThumbWheel.h](https://github.com/PixlOne/logiops/blob/main/src/logid/backend/hidpp20/features/ThumbWheel.h)
- [cvuchener/hidpp IReprogControlsV4](https://github.com/cvuchener/hidpp/blob/master/src/libhidpp/hidpp20/IReprogControlsV4.h)
- [cvuchener/hidpp IFeatureSet](https://github.com/cvuchener/hidpp/blob/master/src/libhidpp/hidpp20/IFeatureSet.h)
- [hidpp Rust crate](https://docs.rs/hidpp)

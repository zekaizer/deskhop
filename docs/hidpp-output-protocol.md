# Logitech HID++ Output Report Protocol — SET_REPORT Analysis

## Problem

DeskHop passthrough forwards HID++ output reports from the host (Options+)
to the Logitech receiver via PIO-USB. The receiver STALLs SET_REPORT
control transfers when using TinyUSB's default `tuh_hid_set_report()`.

## Receiver Hardware Facts (Verified on Real Hardware)

| Property | Unifying (C52B) | Bolt (C548) |
|----------|----------------|-------------|
| VID/PID | 0x046D/0xC52B | 0x046D/0xC548 |
| HID Interfaces | 3 (kbd, mouse, vendor) | 3 (kbd, mouse, vendor) |
| Vendor interface `bNumEndpoints` | 1 (IN only) | 1 (IN only) |
| Interrupt OUT endpoint | **None** | **None** |
| SET_REPORT (LED, rid=0x00, 1B) | OK (result=0) | OK (result=0) |
| SET_REPORT (HID++, rid=0x10, 6B) | **STALL** (result=2) | **STALL** (result=2) |

## USB HID Output Report Delivery

Per USB HID spec, output reports are sent via:
1. **Interrupt OUT endpoint** — if the interface has one
2. **SET_REPORT control transfer** — fallback when no OUT endpoint

Since Logitech receivers have no interrupt OUT endpoint, SET_REPORT is the
only standard mechanism.

## Linux Kernel Implementation

### hid-logitech-hidpp.c

```c
static int __hidpp_send_report(struct hid_device *hid_dev,
                               struct hidpp_report *hidpp_report)
{
    int fields_count = /* 7 for short, 20 for long */;

    /* Try interrupt OUT first (for Bluetooth/quirky devices) */
    ret = hid_hw_output_report(hid_dev, (u8 *)hidpp_report, fields_count);
    if (ret != -ENOSYS)
        return ret > 0 ? ret : -ECOMM;

    /* Fallback: SET_REPORT control transfer */
    ret = hid_hw_raw_request(hid_dev, hidpp_report->report_id,
                             (u8 *)hidpp_report, fields_count,
                             HID_OUTPUT_REPORT, HID_REQ_SET_REPORT);
    return (ret < 0) ? ret : ((ret != fields_count) ? -ECOMM : 0);
}
```

Key observation: `fields_count` is the **full report size including report_id**:
- HID++ short: 7 bytes (1 report_id + 6 data)
- HID++ long: 20 bytes (1 report_id + 19 data)

The `hid_hw_raw_request()` call passes the **full report buffer** (starting
with report_id byte) and the **full length** (7 or 20).

### hid-logitech-dj.c

```c
#define LOGITECH_DJ_INTERFACE_NUMBER 0x02
```

The DJ driver binds to **interface 2** (vendor/HID++) specifically.

### Linux HID Core — hid_hw_raw_request → usbhid_raw_request

The Linux USB HID transport (`hid-core.c` → `usbhid/hid-core.c`) translates
`hid_hw_raw_request()` into a USB control transfer:

```c
/* For SET_REPORT: */
usb_control_msg(dev, usb_sndctrlpipe(dev, 0),
    HID_REQ_SET_REPORT,                        /* bRequest = 0x09 */
    USB_TYPE_CLASS | USB_RECIP_INTERFACE,       /* bmRequestType = 0x21 */
    ((report_type + 1) << 8) | report_id,      /* wValue */
    interface->desc.bInterfaceNumber,           /* wIndex */
    buf, count, timeout);                       /* data, wLength */
```

**Critical detail**: `buf` contains the **full report including report_id as
the first byte**, and `count` is the **full length** (7 for short HID++).

## SET_REPORT Wire Format (Correct)

For HID++ short report `10 FF 00 1A 00 00 00`:

```
SETUP (8 bytes):
  bmRequestType = 0x21  (Host→Device, Class, Interface)
  bRequest      = 0x09  (SET_REPORT)
  wValue        = 0x0210 (type=OUTPUT=0x02, report_id=0x10)
  wIndex        = 0x0002 (interface 2 = vendor)
  wLength       = 0x0007 (7 bytes = full report)

DATA OUT (7 bytes):
  10 FF 00 1A 00 00 00  (report_id + payload)

STATUS IN:
  (zero-length DATA1, ACK)
```

## TinyUSB Default Behavior (Wrong for Logitech)

TinyUSB's `tuh_hid_set_report()` sends:
- `wLength = 6` (payload only, excludes report_id)
- DATA = `FF 00 1A 00 00 00` (no report_id prefix)

This causes STALL because the receiver expects the full 7-byte report.

## Fix: Use tuh_control_xfer Directly

Bypass `tuh_hid_set_report()` and construct the SET_REPORT manually with
the full report (report_id + payload) in the data phase:

```c
buf[0] = report_id;
memcpy(buf + 1, payload, payload_len);

tusb_control_request_t request = {
    .bmRequestType_bit = { .recipient = TUSB_REQ_RCPT_INTERFACE,
                           .type      = TUSB_REQ_TYPE_CLASS,
                           .direction = TUSB_DIR_OUT },
    .bRequest = 0x09,  /* SET_REPORT */
    .wValue   = (HID_REPORT_TYPE_OUTPUT << 8) | report_id,
    .wIndex   = interface_number,  /* must be 2 for vendor */
    .wLength  = payload_len + 1    /* full report length */
};
```

## References

- [Logitech HID++ 1.0 Specification](https://lekensteyn.nl/files/logitech/logitech_hidpp10_specification_for_Unifying_Receivers.pdf)
- [Linux kernel hid-logitech-hidpp.c](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-logitech-hidpp.c)
- [Linux kernel hid-logitech-dj.c](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-logitech-dj.c)
- USB HID Specification 1.11, Section 7.2.2 "Set_Report Request"
- USB 2.0 Specification, Section 8.5.3 "Control Transfers"

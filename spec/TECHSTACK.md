# DeskHop Semi-DDM 펌웨어 개발 기술 스택

> **프로젝트 목표**: DeskHop PCB v1.1 기반으로 Semi-DDM USB passthrough를 구현하여,
> Win11 노트북(트리플 모니터)에서 Logitech MX 시리즈의 전체 기능(HID++ 포함)을 유지하면서
> Lenovo K10 Pro Android 태블릿과 키보드/마우스를 핫키로 스위칭한다.
>
> **문서 버전**: 0.2 (팩트체크 반영)
> **최종 수정**: 2026-03-18

---

## 1. 하드웨어 개요 (고정)

### 1.1 DeskHop PCB v1.1

| 구성요소 | 파트 | 수량 | 역할 |
|----------|------|------|------|
| U1, U2 | Raspberry Pi Pico (RP2040) | 2 | 각각 하나의 output 담당 |
| U4 | TI ISO7721DR | 1 | 2kV galvanic isolation (UART) |
| U3, U5 | TPD4E1U06DBVR | 2 | USB ESD 보호 |
| J1, J4 | USB-A 커넥터 | 2 | 키보드/마우스 수신기 연결 |
| R1-R4 | 27Ω 0805 | 4 | USB 시리즈 저항 |
| C1-C4 | 100nF / 4.7μF 0805 | 4 | 디커플링 / VBUS 안정화 |

### 1.2 RP2040 리소스 할당 (Pico 1개 기준)

| 리소스 | 용도 | 비고 |
|--------|------|------|
| Native USB (RHPORT 0) | USB Device — 컴퓨터 연결 | 전용 4KB DPRAM, 16 endpoints |
| PIO-USB (RHPORT 1, GP14/GP15) | USB Host — 수신기 연결 | PIO 블록 1개 점유 (SM 3개 + 명령어 메모리 32 words 전부) |
| UART0 (GP12/13 또는 GP16/17) | 보드 간 통신 (ISO7721DR 경유) | **3,686,400 baud** (3.6864 Mbps), 8N1, DMA 기반 |
| Core 0 | TinyUSB device + host task, main loop | |
| Core 1 | PIO-USB low-level processing | |
| DMA (3 channels / 총 12개) | UART TX + RX + control (ring buffer) | 9 채널 여유 |
| SRAM (264 KB) | 펌웨어 전체 ~30–50 KB 사용 추정 | USB 스택 ~10–20 KB, 앱 로직 ~10–20 KB, 스택/힙 ~10 KB |
| Flash (2 MB) | 펌웨어 바이너리 ~100–120 KB | OTA staging ~120 KB, config ~8 KB, web UI ~30 KB. 약 1.7 MB 여유 |
| System Clock | 120 MHz | PIO-USB 요구: 12 MHz 배수 |

#### PIO 리소스 상세

Pico-PIO-USB는 **1개 PIO 블록의 SM 3개**(TX, RX, EOP 감지)와 명령어 메모리 전체(32 words)를 사용한다.
잔여 PIO 리소스: 사용 블록에 SM 1개, 미사용 블록에 SM 4개 + 명령어 메모리 32 words.

#### Pico-PIO-USB Host 제한사항

| 항목 | 기본값 | 비고 |
|------|--------|------|
| `PIO_USB_DEVICE_CNT` | 4 | 최대 연결 가능 USB 장치 수 |
| `PIO_USB_DEV_EP_CNT` | 16 | 장치당 최대 endpoint |
| 총 endpoint pool | 32 | 컴파일타임 설정 가능 |
| Root port | 2 | 동일 SM으로 핀 전환하여 공유 |
| Hub port | 8 | |
| 지원 속도 | Full-Speed / Low-Speed | High-Speed 미지원 |

### 1.3 물리 연결 토폴로지

```
                          ┌──────────────────────────────────┐
                          │        DeskHop PCB v1.1          │
                          │                                  │
 [Logitech Bolt          │  ┌─────────────┐  UART+ISO  ┌─────────────┐
  수신기] ──── USB-A ─────┤──│  Pico A      │◄──────────►│  Pico B      │
                          │  │  (Win11)     │  7721DR   │  (Android)  │
                          │  │  PIO=Host    │           │  PIO=Host   │
                          │  │  Native=Dev  │           │  Native=Dev │
                          │  └──────┬───────┘           └──────┬──────┘
                          │         │ micro USB                │ micro USB
                          └─────────┼──────────────────────────┼─────────┘
                                    │                          │
                            ┌───────▼───────┐         ┌───────▼────────┐
                            │  Win11 노트북  │         │ Lenovo K10 Pro │
                            │  (트리플 모니터)│         │ (USB-C OTG)    │
                            └───────────────┘         └────────────────┘
```

**핵심 설계 결정**: Bolt 수신기를 Win11 쪽 Pico(A)의 USB-A 호스트 포트에 연결.
동일 Pico 내에서 host→device raw passthrough가 가능하므로 UART 병목 없이 최소 지연.

### 1.4 Android 태블릿: Lenovo K10 Pro

| 항목 | 사양 |
|------|------|
| 모델 | Lenovo K10 Pro 10.6" LTE (글로벌롬) |
| SoC | MediaTek Helio G80 (Octa-core, 2x Cortex-A75 @2.0GHz + 6x Cortex-A55 @1.8GHz) |
| RAM / Storage | 6 GB LPDDR4x / 128 GB |
| 디스플레이 | 10.6" IPS LCD, 2000 × 1200 (2K) |
| OS | Android 12 |
| USB | USB Type-C 2.0 |
| 배터리 | 7700 mAh, 20W 충전 |
| 연결 | Wi-Fi 802.11 a/b/g/n/ac (dual-band), Bluetooth 5.0, 4G LTE |

#### DeskHop 연결 시 고려사항

| 항목 | 상태 | 비고 |
|------|------|------|
| USB OTG 지원 | ✅ | Helio G80 (MT6769V)는 MUSB 기반 USB 2.0 OTG dual-role controller 내장. USB-C 포트로 HID 장치 연결 가능 |
| OTG 어댑터 필요 | ✅ | DeskHop Pico의 micro USB 출력 → 태블릿 USB-C 연결에 micro USB male → USB-C male 케이블 또는 OTG 어댑터 조합 필요 |
| OTG 전원 공급 | ⚠️ 확인 필요 | USB 2.0 OTG 스펙상 최대 **500mA @ 5V** (2.5W) 공급 가능. DeskHop Pico 소비 전류는 이 범위 내이나, 태블릿 배터리 잔량 낮을 때 OTG VBUS 차단 가능성 있음. 실측 필요 |
| HID 장치 인식 | ✅ | Android 12 + Helio G80 조합에서 USB HID 호환성 이슈 보고 없음. 표준 HID 네이티브 지원 |
| 충전 동시 사용 | ❌ | USB-C 포트 1개이므로 DeskHop 연결 중 유선 충전 불가. 7700mAh이므로 일반 사용 기준 충분 |

> **주의**: Lenovo K10 Pro는 중국/수출 시장 전용 제품("Qitian K10 Pro")으로 Lenovo PSREF에 정식 등재되어 있지 않다.
> 모델 번호 TB331FU는 Lenovo Tab M11에 해당하며 K10 Pro와 다른 제품이다. 정확한 K10 Pro 모델 번호는 실기 확인 필요.

#### 케이블 구성

```
DeskHop Pico B (micro USB male)
     │
     ▼
 micro USB → USB-A female 변환 케이블
     │
     ▼
 USB-A male → USB-C male OTG 케이블/어댑터
     │
     ▼
 Lenovo K10 Pro (USB-C)
```

또는 micro USB male → USB-C male 직결 케이블(OTG 지원 제품)을 사용하면 단순화 가능.

---

## 2. 기존 DeskHop 펌웨어 아키텍처

### 2.1 소프트웨어 스택

```
┌────────────────────────────────────────────────────┐
│                   Application Layer                │
│  keyboard.c / mouse.c / handlers.c / tasks.c       │
├────────────────────────────────────────────────────┤
│              HID Report Processing                 │
│  hid_parser.c / hid_report.c / usb_descriptors.c  │
├────────────────────────────────────────────────────┤
│                   USB Stack                        │
│  TinyUSB Device (RHPORT 0) + Host (RHPORT 1)      │
├────────────────────────────────────────────────────┤
│              Board Communication                   │
│  uart.c + DMA (protocol.c — 고정 길이 패킷)        │
├────────────────────────────────────────────────────┤
│                   HAL / BSP                        │
│  Pico SDK + Pico-PIO-USB                           │
└────────────────────────────────────────────────────┘
```

### 2.2 현재 HID 처리 파이프라인

```
[수신기] → tuh_hid_report_received_cb()
               │
               ▼
         HID Report 파싱 (hid_parser.c)
         extract_kbd_data() / extract_report_values()
               │
               ▼
         내부 report 구조체로 변환
         (hid_keyboard_report_t / mouse_report_t)
               │
               ├── active output == 자기 보드 → 로컬 큐잉 → tud_hid_n_report()
               └── active output == 상대 보드 → UART 패킷 전송 → 상대 보드 큐잉
```

### 2.3 현재 USB Device Descriptor (컴퓨터에 노출)

| Interface | Report Descriptor | 용도 |
|-----------|-------------------|------|
| ITF_NUM_HID | Keyboard + Abs Mouse + Consumer + System | 메인 HID 복합 장치 |
| ITF_NUM_HID_REL_M | Relative Mouse | Windows 멀티모니터 workaround |
| (config mode) ITF_NUM_HID_VENDOR | Vendor HID | WebHID 설정 페이지 |
| (config mode) MSC | Mass Storage | DESKHOP 드라이브 |

마우스 report는 buttons 8개 + X/Y (16bit) + wheel + pan으로 **고정**.
Logitech HID++ vendor interface는 캡처/전달되지 않음.

### 2.4 제한사항 (현재 펌웨어)

| 항목 | 상세 |
|------|------|
| Windows 멀티모니터 | KB5003637 이후 absolute coordinate가 primary monitor에만 매핑. 보조 모니터는 relative fallback (gaming mode) |
| Vendor-specific HID | HID++ (Logitech), Razer Chroma 등 vendor endpoint 미지원 |
| Device descriptor | 컴파일타임 고정, 런타임 변경 불가 |
| UART 패킷 | 고정 길이, raw report passthrough 미고려 |

---

## 3. Semi-DDM Passthrough 아키텍처 설계

### 3.1 Semi-DDM 개념

| KVM 클래스 | 콘솔 측 | PC 측 | 특징 |
|------------|---------|-------|------|
| Emulated | generic descriptor | generic descriptor | 기본 K&M만 지원 |
| **Semi-DDM** | **실제 장치 descriptor 유지** | **active: 실제 / inactive: emulation** | 확장 기능 지원 + 빠른 전환 |
| Full DDM | 실제 descriptor | 모든 PC에 동시 유지 | 최상의 호환성 |

> **DDM 관련 참고**: ConnectPRO는 DDM을 미국 특허로 주장하나 **특허 번호는 미공개**이며 USPTO에서 확인 불가.
> "DDM"이라는 용어는 StarTech, TESmart 등 타사에서도 사용 중이며, 업계 표준 정의가 아닌 마케팅 용어에 가깝다.
> 본 프로젝트의 Semi-DDM 구현은 DeskHop의 오픈소스 코드 위에 descriptor passthrough를 추가하는 것으로,
> 상용 DDM KVM의 전용 ASIC/MCU 기반 구현(descriptor 캐싱+리플레이)과는 아키텍처가 다르다.

본 프로젝트는 **Semi-DDM** 방식을 기반으로 하되, HID++ vendor interface에 한해 **Full DDM 특성**을 적용한다:

- **Win11 (Pico A)**: Bolt 수신기의 원본 descriptor/report를 그대로 passthrough
- **Android (Pico B)**: 기존 DeskHop 방식 (파싱 후 고정 descriptor로 재생성)
- **HID++ vendor interface**: **프로토콜 메시지**(sw_id≠0)는 active output과 무관하게 Win11 ↔ 수신기 간 상시 양방향 통신 유지 (Options+ 세션 끊김 방지). **입력 이벤트**(sw_id=0, HiRes Scroll/버튼 등)는 active output에만 전달

### 3.2 모드별 데이터 경로

#### Active Output = Win11 (Passthrough 모드)

```
[Bolt 수신기]
     │
     ▼ tuh_hid_report_received_cb()
     │
     │  *** 파싱 없음 — raw report 그대로 전달 ***
     │
     ▼ tud_hid_n_report(instance, report_id, raw_report, len)
     │
     ▼
[Win11] ← 원본 HID report 수신 (HID++ 포함)
```

- 키보드 report는 예외: 키 리매핑 엔진 처리를 위해 파싱 경로 유지 (§4 참조)
- Output report (Win11 → 수신기 방향)도 passthrough 필요: `tud_hid_set_report_cb()`에서 큐잉 → `passthrough_task()`에서 `tuh_control_xfer()`로 전송. TinyUSB의 `tuh_hid_set_report()`는 wLength에 report_id를 포함하지 않아 Logitech 수신기에서 STALL 발생. `tuh_control_xfer()`로 직접 SET_REPORT 전송 시 wLength=full report(7B/20B), DATA=[report_id + payload]

#### Active Output = Android (Interface 레벨 분리)

HID++ vendor interface는 active output과 **무관하게** Win11에 항상 양방향 통신을 유지한다.
이를 통해 Android 사용 중에도 Options+가 수신기 연결을 lost로 판단하지 않으며,
설정 변경, 배터리 상태 조회 등이 정상 동작한다.

```
[Bolt 수신기] → tuh_hid_report_received_cb()
                    │
                    ├── itf_protocol == KEYBOARD
                    │     → 파싱 → 키 리매핑 → UART → Pico B → Android
                    │
                    ├── itf_protocol == MOUSE
                    │     → 파싱 → UART → Pico B → Android
                    │
                    └── itf_protocol == NONE (HID++ vendor)
                          │
                          ├── sw_id ≠ 0 (프로토콜 응답)
                          │     → tud_hid_n_report() → Win11    ✅ 항상 전달
                          │
                          └── sw_id == 0 (입력 이벤트: 스크롤, 버튼 등)
                                → drop (P3: mouse_report_t 변환 → UART → Pico B)
```

> **sw_id 분류**: HID++ 2.0 report의 byte[3] 하위 4비트(sw_id)로 구분.
> sw_id=0은 디바이스가 자발적으로 보내는 입력 이벤트(unsolicited),
> sw_id≠0은 호스트 쿼리에 대한 응답(solicited). `passthrough_is_hidpp_input_event()` 참조.

```
[Win11 Options+] → tud_hid_set_report_cb()
                    │
                    └── instance == HID++ vendor interface
                          → out_queue에 큐잉 → passthrough_task()
                          → tuh_control_xfer() → Bolt 수신기  ✅ 항상 전달
```

#### Interface 유형별 라우팅 정책

| Interface 유형 | Input (수신기→PC) | Output (PC→수신기) | Active 의존 |
|---------------|-------------------|-------------------|------------|
| Keyboard (KEYBOARD) | active output에만 전달 | LED set_report (기존) | ✅ 의존 |
| Mouse (MOUSE) | active output에만 전달 | 없음 | ✅ 의존 |
| HID++ vendor (NONE) — 프로토콜 (sw_id≠0) | **Win11에 항상 전달** | **항상 수신기로 전달** | ❌ **상시 연결** |
| HID++ vendor (NONE) — 입력 이벤트 (sw_id=0) | active output에만 전달 | — | ✅ 의존 |

### 3.3 핵심 데이터 구조체

```c
/* passthrough.h */

#define MAX_PASSTHROUGH_IFACES  6
#define MAX_HID_DESC_SIZE       512
#define MAX_CONFIG_DESC_SIZE    280
#define ITF_NUM_PT_BASE         2      /* device 측 passthrough interface base */
#define EPNUM_PT_BASE           0x83   /* device 측 passthrough endpoint base */

typedef struct {
    uint8_t  dev_addr;
    uint8_t  instance;             /* host 측 instance */
    uint8_t  itf_protocol;         /* HID_ITF_PROTOCOL_KEYBOARD / MOUSE / NONE */
    uint16_t desc_len;
    uint8_t  desc[MAX_HID_DESC_SIZE];   /* raw HID report descriptor */
    bool     always_passthrough;        /* true: HID++ vendor — 프로토콜 메시지 상시 전달 */
} passthrough_iface_t;

typedef struct {
    uint8_t              iface_count;
    passthrough_iface_t  ifaces[MAX_PASSTHROUGH_IFACES];
    bool                 enumeration_done;

    /* Device-side passthrough state */
    bool                 active;
    uint8_t              config_desc[MAX_CONFIG_DESC_SIZE];
    uint16_t             config_desc_len;
    uint64_t             last_capture_us;
    uint64_t             reconnect_at_us;

    /* Upstream device identity (FR-PT-009) */
    uint16_t             upstream_vid;
    uint16_t             upstream_pid;

    /* Deferred HID++ output report queue */
    struct {
        uint8_t  dev_addr;
        uint8_t  instance;
        uint8_t  report_id;
        uint8_t  report_type;
        uint8_t  data[32];
        uint16_t len;
        bool     pending;
    } out_queue;
} passthrough_state_t;
```

Enumeration 시 `itf_protocol == HID_ITF_PROTOCOL_NONE`인 interface는 `always_passthrough = true`로 설정한다.
이 플래그가 설정된 interface의 **프로토콜 메시지**(sw_id≠0)는 output 전환과 무관하게 Win11 ↔ 수신기 간 항상 중계한다.
**입력 이벤트**(sw_id=0)는 active output에만 전달한다 (FR-AO-005).

Output report는 TinyUSB device callback에서 직접 전송할 수 없으므로 `out_queue`에 큐잉한 뒤
`passthrough_task()`(10Hz)에서 `tuh_control_xfer()`로 전송한다.

### 3.4 구현 단계별 계획

#### Phase 1: Descriptor 캡처 및 덤프

**목표**: Bolt 수신기가 노출하는 모든 HID interface의 descriptor를 시리얼 로그로 확인.

**수정 파일**: `usb.c` (`tuh_hid_mount_cb`)

**작업 내용**:
- `tuh_descriptor_get_hid_report()` 콜백에서 각 interface의 raw descriptor를 `passthrough_state.ifaces[n].desc`에 복사
- CDC 디버그 출력(또는 UART printf)으로 descriptor hex dump
- Logitech Bolt 수신기의 interface 구조 확인

**Bolt 수신기 확인된 구조** (VID=0x046D, PID=0xC548, Full-Speed, 98mA, 내부 hub 없음):

| Interface | Protocol | Endpoint | Report Descriptor | Report IDs | 용도 |
|-----------|----------|----------|-------------------|------------|------|
| 0 | KEYBOARD (boot) | EP 0x81 | 67 bytes | 없음 (modifier + 120-key bitmap, 15 bytes) | 키보드 |
| 1 | MOUSE (boot) | EP 0x82 | 133 bytes | 0x02 (mouse 16btn, 16bit X/Y, wheel, pan), 0x03 (consumer control), 0x04 (system control) | 마우스 + 미디어 + 시스템 |
| 2 | NONE (vendor) | EP 0x83 | usage page 0xFF00 | 0x10 (HID++ short: 7B), 0x11 (HID++ long: 20B) | HID++ 제어 |

> **주의**: Bolt는 DJ report를 사용하지 않는다. Unifying 수신기와 달리 DJ report ID(0x20/0x21)가 없으며,
> HID++ device index addressing (0xFF=수신기, 0x01~0x06=페어링 장치)으로만 통신한다.
> Linux 커널에서도 `hid-logitech-dj` 모듈이 아닌 `hid-generic`으로 처리된다.

#### Phase 2: 고정 Descriptor Passthrough

**목표**: Phase 1에서 덤프한 descriptor를 하드코딩하여 Win11에 노출, raw report 전달 확인.

**수정 파일**: `usb_descriptors.c`, `usb.c`, `tusb_config.h`

**작업 내용**:
- `tusb_config.h`에서 `CFG_TUD_HID`를 6으로 확대 (Win11 Pico 빌드 전용)
  - HID instance당 SRAM ~208B (`hidd_interface_t` 16B + `hidd_epbuf_t` 192B). 2→6 증가 시 추가 ~832B (무시 가능)
  - **endpoint 제약**: RP2040 native USB는 16 endpoints. HID 6개면 IN endpoint 6개 소비. DeskHop 기존(2 HID + MSC + CDC debug) 감안 시 여유 있음
- 덤프된 descriptor 기반으로 `desc_hid_report_passthrough[]` 하드코딩
- `tud_hid_descriptor_report_cb()`에서 passthrough descriptor 반환 (동적 버퍼 포인터 반환 가능)
- Configuration descriptor를 수동으로 바이트 단위 구성 (`build_config_descriptor()`)
  - **중요**: configuration descriptor의 `wReportLength` 필드가 `tud_hid_descriptor_report_cb()`의 반환 버퍼 크기와 일치해야 함. TinyUSB는 `hidd_open()` 시 이 필드에서 길이를 읽음
- `tuh_hid_report_received_cb()`에서 raw report → `tud_hid_n_report()` 직접 전달

**검증 기준**:
- Win11에서 Device Manager → HID 장치가 Bolt 수신기와 동일한 interface 수로 인식
- Logitech Options+가 수신기를 인식하고 연결 상태 표시

##### 조건부 VID/PID 전환 (Conditional VID/PID Switching)

Logi Options+는 VID/PID로 디바이스를 식별한다. DeskHop의 VID/PID(0x1209/0xC000)로는
Options+가 HID++ 통신을 시도하지 않으므로, passthrough 활성화 시 upstream 디바이스의
VID/PID를 그대로 노출해야 한다.

**설계**:
- `passthrough_state_t`에 `upstream_vid`, `upstream_pid` 필드 추가
- `tuh_hid_mount_cb()`에서 `tuh_vid_pid_get()`로 upstream VID/PID 캡처 (동기, 1회)
- Re-enumeration 시 `tud_descriptor_device_cb()`가 upstream VID/PID로 device descriptor 반환
- `tud_descriptor_string_cb()`에서 Logitech VID(0x046D) 감지 시 제조사/제품 문자열 전환
  - Manufacturer → "Logitech", Product → "USB Receiver" (하드코딩, async capture 회피)
- Upstream 디바이스 분리(unmount) 시 DeskHop 원래 VID/PID로 재열거

**VID/PID 전환 흐름**:
```
[Bolt 연결] → tuh_hid_mount_cb → upstream_vid=0x046D, upstream_pid=0xC548 캡처
           → 500ms 안정화 대기
           → passthrough_activate() → config descriptor 생성
           → tud_disconnect() → 200ms → tud_connect()
           → tud_descriptor_device_cb(): VID=0x046D, PID=0xC548 반환
           → tud_descriptor_string_cb(): "Logitech" / "USB Receiver" 반환
           → Host(Win11/macOS)가 Logitech Bolt로 인식
           → Options+가 HID++ 통신 시작

[Bolt 분리] → tuh_hid_umount_cb → passthrough_remove_device()
           → iface_count == 0 → active=false, upstream_vid/pid=0
           → tud_disconnect() → 200ms → tud_connect()
           → tud_descriptor_device_cb(): VID=0x1209, PID=0xC000 반환 (DeskHop)
```

**우선순위**: config mode > passthrough VID/PID > DeskHop VID/PID

**비 Logitech 디바이스**: upstream_vid != 0x046D인 경우 VID/PID는 전환되지만
string descriptor는 DeskHop 문자열 유지. Options+ 연동은 Logitech 전용.

#### Phase 3: 양방향 상시 통신 (HID++ Always-On Passthrough) — 구현 완료

**목표**: HID++ vendor interface의 양방향 통신을 active output과 무관하게 항상 유지.
Options+가 수신기를 항상 인식하고, Android 사용 중에도 설정 변경/배터리 조회 등이 동작.

**수정 파일**: `usb.c`, `passthrough.c`, `tasks.c`

**구현 결과**:

1. **Input (수신기 → PC) 라우팅** (`tuh_hid_report_received_cb`):
   - `always_passthrough` interface에서 sw_id 분류 (`passthrough_is_hidpp_input_event()`)
   - sw_id≠0 (프로토콜 응답): 항상 `tud_hid_n_report()` → Win11
   - sw_id=0 (입력 이벤트): active output일 때만 전달, 비활성 시 drop
   - `tuh_hid_receive_report()`는 항상 re-queue (vendor interface 폴링 유지 필수)

2. **Output (PC → 수신기) 전달** (`tud_hid_set_report_cb` → `passthrough_task`):
   - TinyUSB device callback 내에서 control transfer 불가 → `out_queue`에 큐잉
   - `passthrough_task()`(10Hz)에서 `tuh_control_xfer()`로 직접 SET_REPORT 전송
   - **TinyUSB의 `tuh_hid_set_report()`는 사용 불가**: wLength에 report_id 미포함 → Logitech 수신기 STALL
   - 올바른 포맷: wLength=full report(7B/20B), DATA=[report_id + payload]
   - 상세: `docs/hidpp-output-protocol.md` 참조

3. **양방향 매핑**: `passthrough_host_to_device_instance()` / `passthrough_device_to_host_index()`

**HID++ 2.0 메시지 구조** (vendor interface, report ID 0x10/0x11):

| 필드 | Offset | Short (0x10, 7B) | Long (0x11, 20B) |
|------|--------|-------------------|-------------------|
| Report ID | 0 | 0x10 | 0x11 |
| Device Index | 1 | 1 byte | 1 byte |
| Feature Index | 2 | 1 byte | 1 byte |
| Function ID \| SW ID | 3 | 상위 4bit \| 하위 4bit | 상위 4bit \| 하위 4bit |
| Parameters | 4+ | 3 bytes | 16 bytes |

**관찰된 Feature Index 테이블** (Unifying C52B + MX Master 3S, 디바이스별 동적 할당):

| Feature Index | Feature ID (추정) | 이름 | 입력 이벤트? |
|--------------|-------------------|------|-------------|
| 0x00 | 0x0000 | IRoot | No |
| 0x08 | 0x1000 | Battery | Yes |
| 0x09 | 0x1B04 | ReprogControls V4 | **Yes (버튼)** |
| 0x0E | 0x2121 | HiResScroll | **Yes (스크롤)** |
| 0x0F | 0x2150 | Thumbwheel | **Yes (제스처)** |

> Feature index는 디바이스마다 다르게 할당된다. IRoot(feature 0x0000, 항상 index 0)를 통해
> 런타임에 feature ID → feature index 매핑을 조회할 수 있다. P3(HID++ 변환)에서 활용 예정.

**검증 결과** (Unifying C52B):
- ✅ Options+에서 MX Master + MX Keys 검색/연결 확인
- ✅ Android active 상태에서 Options+ "connected" 유지
- ✅ Android active 상태에서 Options+ 설정 변경 정상 적용
- ✅ Android active 상태에서 HID++ 입력 이벤트(휠/사이드버튼)가 Win11에서 동작하지 않음 (sw_id 분류)
- ⏳ Bolt (C548) 테스트 미완료 — 별도 확인 필요

#### Phase 4: 동적 Descriptor 생성

**목표**: 하드코딩된 descriptor를 런타임 캡처로 교체.

**수정 파일**: `passthrough.c` (신규), `usb_descriptors.c`

**작업 내용**:
- `build_config_descriptor()`: 캡처된 interface 수/descriptor 길이 기반으로 configuration descriptor 동적 조립
  - 각 interface의 `wReportLength`를 캡처된 `desc_len`과 정확히 일치시켜야 함
- `tud_hid_descriptor_report_cb(instance)`: `passthrough_state.ifaces[instance].desc` 반환
- `tud_descriptor_configuration_cb()`: 동적 configuration descriptor 버퍼 반환 (TinyUSB 공식 `dynamic_configuration` 예제 참고)
- Re-enumeration 절차:
  1. 수신기 host enumeration 완료 대기
  2. `tud_disconnect()` — D+ pull-up 해제
  3. descriptor 버퍼 구성
  4. **~200ms 대기** (USB 스펙상 호스트가 disconnect를 인식하는 최소 시간)
  5. `tud_connect()` — D+ pull-up 재활성화
  6. Win11이 새 descriptor로 re-enumerate

**TinyUSB 동적 descriptor 선행 사례**:
- **GP2040-CE**: RP2040 게임패드 펌웨어. XInput/DirectInput/Switch descriptor를 부팅 시 동적 선택
- **Adafruit TinyUSB Arduino**: C++ Builder 객체로 런타임 descriptor 조립
- TinyUSB maintainer (@hathach)가 Discussion #3242에서 `tud_disconnect()/tud_connect()` 경량 re-enumeration 패턴 확인 (2025.09)

**TinyUSB 제약 대응**:
- `CFG_TUD_HID = 6` (최대치 미리 확보, Bolt는 3개 사용)
- 미사용 instance는 configuration descriptor에 미포함 → Win11은 실제 interface만 인식
- `_usbd_dev.itf2drv[]` 배열 크기가 `CFG_TUD_HID` 이내면 문제 없음
- 대안 (full re-init): `tud_deinit()` → `tud_init()` — 전체 스택 해제/재초기화. 더 무겁지만 확실

#### Phase 5: 모드 분기 및 전환

**목표**: 핫키(`Left CTRL + Caps Lock`)로 output 전환 시 keyboard/mouse 라우팅만 분기.
HID++ vendor interface는 전환과 무관하게 상시 연결 유지.

**수정 파일**: `handlers.c`, `passthrough.c`, `keyboard.c`

**작업 내용**:
- `passthrough_state.passthrough_active` 플래그는 keyboard/mouse 라우팅에만 영향
- `always_passthrough == true` interface는 플래그와 무관하게 항상 Win11 ↔ 수신기 통신 유지
- Win11→Android 전환 시: keyboard/mouse만 파싱→UART 경로로 전환
- Android→Win11 전환 시: keyboard/mouse를 raw passthrough로 복귀
- 전환 시 USB re-enumeration 불필요 (descriptor는 Win11 측에 항상 유지, HID++ 세션도 끊기지 않음)

---

## 4. 키 리매핑 엔진 (Key Remap Engine)

### 4.1 설계 목표

호스트 소프트웨어 없이 펌웨어 레벨에서 키 입력을 변환하는 범용 리매핑 엔진.
QMK의 핵심 개념을 참고하되, DeskHop의 리소스 제약(RP2040, 264KB SRAM)에 맞게 경량화한다.

| 원칙 | 상세 |
|------|------|
| 호스트 독립 | OS/드라이버 설치 없이 동작. 모든 output에 동일 적용 |
| Output별 독립 설정 | Win11 / Android 각각 다른 리매핑 프로필 적용 가능 |
| Flash 저장 | 설정은 flash에 persistent 저장, web config UI로 편집 |
| 최소 지연 | 리매핑 처리 ≤ 1ms (1000Hz 폴링 주기 내) |

### 4.2 지원 리매핑 타입

| 타입 | 동작 | 사용 예 |
|------|------|---------|
| **SIMPLE** | A → B (1:1 키 교체) | CapsLock → Ctrl, 한/영 키 위치 변경 |
| **TAP_HOLD** | 짧게 = A, 길게(≥threshold) = B | Tab: tap=한/영, hold=Tab |
| **TAP_DANCE** | N회 연속 tap에 따라 다른 키 출력 | 1tap=A, 2tap=B, 3tap=C |
| **COMBO** | 복수 키 동시 입력 → 단일 키 출력 | J+K 동시 → Esc |
| **MODIFIER_MORPH** | modifier 유무에 따라 다른 키 | Shift+Backspace → Delete |
| **MACRO** | 키 시퀀스 일괄 출력 | 트리거 → 문자열/단축키 시퀀스 |

초기 구현은 **SIMPLE + TAP_HOLD**에 집중하고, 나머지는 프레임워크만 확보 후 점진 확장.

### 4.3 데이터 구조

```c
/* key_remap.h */

#define MAX_REMAP_ENTRIES    16
#define MAX_MACRO_LENGTH     32
#define MAX_COMBO_KEYS        4
#define MAX_TAP_DANCE_TAPS    4

typedef enum {
    REMAP_SIMPLE,
    REMAP_TAP_HOLD,
    REMAP_TAP_DANCE,
    REMAP_COMBO,
    REMAP_MODIFIER_MORPH,
    REMAP_MACRO,
} remap_type_t;

typedef struct {
    uint8_t  keycode;               /* output keycode */
    uint8_t  modifier;              /* output modifier mask (0 = none) */
} key_action_t;

typedef struct {
    uint8_t      trigger;           /* intercept할 물리 키 */
    remap_type_t type;
    uint8_t      output_mask;       /* bit0=Output A, bit1=Output B, 0xFF=all */

    union {
        /* REMAP_SIMPLE */
        struct {
            key_action_t replacement;
        } simple;

        /* REMAP_TAP_HOLD */
        struct {
            key_action_t tap_action;
            key_action_t hold_action;
            uint32_t     threshold_us;      /* default 200000 (200ms) */
        } tap_hold;

        /* REMAP_TAP_DANCE */
        struct {
            key_action_t actions[MAX_TAP_DANCE_TAPS];
            uint8_t      max_taps;
            uint32_t     term_us;           /* 다음 tap 대기 시간 */
        } tap_dance;

        /* REMAP_COMBO */
        struct {
            uint8_t      keys[MAX_COMBO_KEYS];
            uint8_t      key_count;
            key_action_t action;
            uint32_t     term_us;
        } combo;

        /* REMAP_MODIFIER_MORPH */
        struct {
            uint8_t      modifier_mask;     /* 이 modifier가 눌려있으면 */
            key_action_t normal_action;     /* modifier 없을 때 */
            key_action_t morphed_action;    /* modifier 있을 때 */
        } mod_morph;

        /* REMAP_MACRO */
        struct {
            key_action_t sequence[MAX_MACRO_LENGTH];
            uint8_t      length;
            uint32_t     delay_us;          /* 키 간 딜레이 */
        } macro;
    };
} remap_entry_t;

typedef struct {
    remap_entry_t entries[MAX_REMAP_ENTRIES];
    uint8_t       count;
} remap_config_t;
```

### 4.4 런타임 상태 관리

각 리매핑 엔트리는 독립적인 런타임 상태를 가진다.

```c
typedef enum {
    RS_IDLE,
    RS_WAITING,       /* tap-hold: threshold 대기 중 */
    RS_HELD,          /* tap-hold: hold 확정 */
    RS_TAP_PENDING,   /* tap-dance: 다음 tap 대기 중 */
} remap_state_t;

typedef struct {
    remap_state_t state;
    uint64_t      timestamp;        /* 상태 전환 시점 */
    uint8_t       tap_count;        /* tap-dance 카운터 */
    bool          consumed;         /* 이번 report에서 처리됨 표시 */
} remap_runtime_t;

/* 전역 엔진 상태 */
typedef struct {
    remap_config_t   config;
    remap_runtime_t  runtime[MAX_REMAP_ENTRIES];
} remap_engine_t;
```

### 4.5 상태 머신 (Tap-Hold)

```
           key down              threshold 경과
  IDLE ──────────────► WAITING ─────────────────► HELD
   ▲                     │                          │
   │    key up           │                          │ key up
   │   (< threshold)     │                          │
   │         ┌───────────┘                          │
   │         ▼                                      │
   │   emit tap_action                     emit hold_action release
   │   (press + release)
   └─────────────────────────────────────────────────┘
```

### 4.6 처리 파이프라인 삽입 지점

```
extract_kbd_data()
     │
     ▼
update_kbd_state()
     │
     ▼
 ┌──────────────────────┐
 │ remap_engine_process()│  ◄── 신규 삽입
 └──────────┬───────────┘
            │
            ▼
check_all_hotkeys()        (DeskHop 기존 핫키는 리매핑 후에 평가)
            │
            ▼
send_key()
```

`remap_engine_process()`는 report를 in-place 수정한다. 반환값으로 report 전달 여부를 결정:

```c
typedef enum {
    REMAP_PASS,       /* report를 그대로 전달 */
    REMAP_MODIFIED,   /* report가 수정됨, 전달 */
    REMAP_CONSUMED,   /* report를 삼킴 (전달하지 않음) */
} remap_result_t;

remap_result_t remap_engine_process(
    remap_engine_t *engine,
    hid_keyboard_report_t *report,
    uint8_t active_output,          /* 현재 active output index */
    device_t *state
);
```

### 4.7 주기적 타이머 처리

Tap-hold의 threshold 초과, tap-dance의 term 만료 등은 키 이벤트가 없어도 시간 경과로 상태가 전환된다.
기존 DeskHop의 `task` 루프에 타이머 체크를 추가:

```c
/* tasks.c 내 주기적 호출 */
void remap_engine_tick(remap_engine_t *engine, device_t *state);
```

이 함수는 1ms 주기로 호출되어, `RS_WAITING` 상태의 엔트리가 threshold를 초과했는지 확인하고 필요 시 report를 생성/큐잉한다.

### 4.8 Passthrough 모드와의 공존

Passthrough 모드에서도 키보드 report는 **항상 파싱 경로**를 유지한다.

**근거**:
- 키 리매핑 처리를 위해 키코드 레벨 접근이 필수
- Logitech 키보드의 vendor-specific 기능(백라이트 등)은 대부분 Options+ ↔ Bolt 수신기 간 HID++ 직접 통신이므로, 키보드 HID report를 passthrough해도 추가 이득 없음
- 마우스만 raw passthrough하면 Logitech 마우스 확장 기능(제스처, DPI, 무한스크롤 등)이 모두 동작

**결론**: 마우스 + HID++ vendor interface = passthrough, 키보드 = 리매핑 엔진 경유.

### 4.9 설정 저장 및 구성

| 항목 | 상세 |
|------|------|
| 저장 위치 | Flash (기존 DeskHop config 영역 확장) |
| 편집 방법 | Web config UI (기존 DeskHop config mode 확장) |
| 기본 프로필 | 빈 상태 (리매핑 없음 = 기존 동작과 동일) |
| 프로필 수 | Output별 1개 (Output A / Output B 각각 독립) |

### 4.10 설정 예시

#### 예시 1: Tab → 한/영 전환 (tap), Tab (hold)

```c
{
    .trigger = HID_KEY_TAB,
    .type    = REMAP_TAP_HOLD,
    .output_mask = 0xFF,                /* 모든 output */
    .tap_hold = {
        .tap_action  = { .keycode = HID_KEY_LANG1, .modifier = 0 },
        .hold_action = { .keycode = HID_KEY_TAB,   .modifier = 0 },
        .threshold_us = 200000,
    },
}
```

#### 예시 2: CapsLock → Left Ctrl (단순 교체)

```c
{
    .trigger = HID_KEY_CAPS_LOCK,
    .type    = REMAP_SIMPLE,
    .output_mask = 0x01,                /* Output A (Win11)만 */
    .simple = {
        .replacement = { .keycode = 0, .modifier = KEYBOARD_MODIFIER_LEFTCTRL },
    },
}
```

#### 예시 3: Right Alt → 한/영 전환 (단순 교체, Android 전용)

```c
{
    .trigger = HID_KEY_ALT_RIGHT,       /* modifier 키도 trigger 가능 */
    .type    = REMAP_SIMPLE,
    .output_mask = 0x02,                /* Output B (Android)만 */
    .simple = {
        .replacement = { .keycode = HID_KEY_LANG1, .modifier = 0 },
    },
}
```

### 4.11 구현 우선순위

| 순서 | 타입 | 근거 |
|------|------|------|
| 1 | SIMPLE | 가장 단순, 즉시 검증 가능 |
| 2 | TAP_HOLD | 핵심 요구사항 (한/영 전환), 상태 머신 기반 |
| 3 | MODIFIER_MORPH | 상태 없이 modifier 조건만 확인, 구현 간단 |
| 4 | COMBO | 복수 키 동시 감지 로직 필요 |
| 5 | TAP_DANCE | tap_hold 확장, 연속 tap 카운팅 |
| 6 | MACRO | 시퀀스 큐잉/딜레이 관리, 가장 복잡 |

---

## 5. Win11 트리플 모니터 대응

### 5.1 문제

Windows KB5003637 이후 HID absolute coordinate가 primary monitor에만 매핑되어 보조 모니터에서 마우스 커서가 동작하지 않음.

### 5.2 현재 DeskHop workaround

`mouse.c`의 `switch_virtual_desktop()`:

```c
case WINDOWS:
    state->relative_mouse = (new_index > 1);  /* 보조 모니터에서 relative fallback */
    break;
```

### 5.3 본 프로젝트 적용 방침

Semi-DDM passthrough 모드에서는 수신기의 원본 mouse report가 Win11에 직접 전달되므로, DeskHop의 absolute/relative 변환 로직을 **거치지 않는다**. Win11은 수신기를 직접 연결한 것과 동일하게 인식하므로, Logitech 마우스 자체의 relative report가 그대로 동작하며 **트리플 모니터 문제가 발생하지 않는다**.

즉 passthrough 구현이 곧 트리플 모니터 해결이다.

---

## 6. 현재 기술 스택 (C 기반)

### 6.1 빌드 환경

| 항목 | 내용 |
|------|------|
| 언어 | C11, C++17 |
| 빌드 시스템 | CMake 3.6+ |
| 컴파일러 | arm-none-eabi-gcc |
| 컴파일러 플래그 | `-Ofast -Wall -mcpu=cortex-m0plus -mtune=cortex-m0plus` |
| SDK | Pico SDK (프로젝트 내 번들) |
| USB 스택 | TinyUSB (프로젝트 내 번들) |
| PIO USB | Pico-PIO-USB (sekigon-gonnoc) |
| 타겟 | `thumbv6m-none-eabi` (Cortex-M0+) |

### 6.2 소스 구조

```
deskhop/
├── src/
│   ├── main.c              # 메인 루프, task 스케줄링
│   ├── setup.c             # 초기화 (clock, UART, DMA, USB, watchdog)
│   ├── keyboard.c          # 키보드 hotkey 처리, report 큐잉
│   ├── mouse.c             # 마우스 좌표 변환, 화면 전환
│   ├── usb.c               # TinyUSB device/host 콜백
│   ├── usb_descriptors.c   # HID report descriptor, configuration descriptor
│   ├── hid_parser.c        # HID descriptor 파서
│   ├── hid_report.c        # HID report 해석/변환
│   ├── handlers.c          # UART 메시지 핸들러
│   ├── protocol.c          # UART 패킷 프로토콜
│   ├── uart.c              # UART DMA 전송
│   ├── tasks.c             # 주기적 task (LED, screensaver 등)
│   ├── ramdisk.c           # config mode USB drive
│   └── include/
│       ├── main.h
│       ├── tusb_config.h       # TinyUSB 설정
│       ├── usb_descriptors.h   # HID descriptor 매크로
│       ├── hid_parser.h        # 파서 구조체
│       ├── handlers.h          # 핸들러 선언
│       ├── pinout.h            # GPIO/UART 핀 매핑
│       └── screen.h            # 화면 좌표/출력 구조체
├── Pico-PIO-USB/               # PIO USB 라이브러리 (submodule)
├── pico-sdk/                   # Pico SDK (submodule)
├── webconfig/                  # 설정 웹 UI 소스
├── disk/                       # FAT 이미지 (config mode)
├── pcb/                        # Gerber 파일
├── case/                       # 3D 프린트 STL
└── CMakeLists.txt
```

### 6.3 신규 추가 파일 (Semi-DDM 구현)

| 파일 | 역할 |
|------|------|
| `src/passthrough.c` | passthrough 상태 관리, descriptor 캡처/동적 생성, 모드 분기 |
| `src/key_remap.c` | 범용 키 리매핑 엔진 (SIMPLE, TAP_HOLD, COMBO 등) |
| `src/include/passthrough.h` | passthrough 구조체, 매크로, 함수 선언 |
| `src/include/key_remap.h` | 리매핑 타입 정의, 설정/런타임 구조체 |

### 6.4 수정 파일 요약

| 파일 | 수정 내용 |
|------|-----------|
| `tusb_config.h` | `CFG_TUD_HID` 확대 (2 → 6) |
| `usb_descriptors.c` | 동적 descriptor 반환 로직 추가, `tud_hid_descriptor_report_cb()` 분기 |
| `usb.c` | `tuh_hid_mount_cb()`에 descriptor 캡처, `tuh_hid_report_received_cb()`에 passthrough 분기, `tud_hid_set_report_cb()`에 output report 전달 |
| `keyboard.c` | `process_keyboard_report()`에 `remap_engine_process()` 삽입 |
| `tasks.c` | `remap_engine_tick()` 주기적 호출 추가 (tap-hold threshold 등 타이머 처리) |
| `handlers.c` | output 전환 시 passthrough 플래그 제어 |
| `CMakeLists.txt` | 신규 소스 파일 추가 |

---

## 7. Rust 포팅 검토

### 7.1 Rust Embedded RP2040 생태계 현황

| 크레이트 | 역할 | 성숙도 |
|----------|------|--------|
| `embassy-rp` | RP2040/RP2350 HAL (GPIO, UART, PIO, DMA, USB, Flash) | 안정 (v0.3+) |
| `embassy-executor` | async 태스크 실행기, 멀티코어 지원 | 안정 |
| `embassy-usb` | USB Device 스택 (CDC, HID, vendor class) | 안정 |
| `embassy-time` | 타이머, Duration, Instant | 안정 |
| `embassy-sync` | async Mutex, Channel, Signal | 안정 |
| `usb-device` / `usbd-hid` | 대안 USB device 스택 (blocking) | 안정 |
| `defmt` + `probe-rs` | 디버깅/로깅 | 안정 |

### 7.2 핵심 종속성 가용 여부

| DeskHop 기능 | C 구현 | Rust 대응 | 상태 |
|-------------|--------|-----------|------|
| USB Device (native) | TinyUSB device | `embassy-usb` | ✅ HID class 지원. `MAX_INTERFACE_COUNT` 기본 4, 환경변수로 확장 가능. 단 interface 구성은 빌드 시 결정 (런타임 동적 재구성 불가) |
| USB Host (PIO) | Pico-PIO-USB + TinyUSB host | **PIO 기반 없음** | ❌ 최대 난관 (상세 §7.3) |
| USB Host (native) | — | `cotton-usb-host` (pdh11) | ⚠️ RP2040 네이티브 USB 컨트롤러 전용. PIO 아님. DeskHop 용도에는 부적합 (native port는 device로 사용 중) |
| UART + DMA | Pico SDK hardware_uart/dma | `embassy-rp` UART + DMA | ✅ |
| Multicore | pico_multicore | `embassy-rp` multicore executor | ✅ |
| Flash R/W | hardware_flash | `embassy-rp` Flash driver | ✅ |
| PIO 프로그래밍 | PIO ASM (usb_tx.pio, usb_rx.pio) | `embassy-rp` PIO driver + `pio-rs` | ✅ PIO 인프라는 있으나 USB 프로토콜 구현은 없음 |
| Watchdog | hardware_watchdog | `embassy-rp` Watchdog | ✅ |

### 7.3 최대 난관: PIO USB Host

**Rust 생태계에 PIO 기반 USB Host 스택은 존재하지 않는다** (2026.03 기준).

- `embassy-usb`: device 전용. Host 기능은 GitHub Issue #3295에서 제안되었으나 **STM32G0B1 전용 프로토타입**만 존재하며, RP2040용 구현이나 PIO 기반 구현은 없음. 미머지 상태.
- `cotton-usb-host` (pdh11): RP2040용 async USB host 스택이나 **네이티브 USB 컨트롤러** 전용. PIO 기반이 아니므로 DeskHop 아키텍처(native=device, PIO=host)에 사용 불가.
- Pico-PIO-USB C 라이브러리에 대한 Rust FFI 바인딩, 포트, 래퍼는 crates.io 및 GitHub에 없음.

**선택지 비교**:

| 선택지 | 장점 | 단점 | 작업량 |
|--------|------|------|--------|
| **FFI 바인딩** (Pico-PIO-USB + TinyUSB host를 C로 유지, Rust에서 호출) | 검증된 USB host 동작 보장 | unsafe 코드 다수, 빌드 복잡도 증가, C/Rust 경계 디버깅 어려움 | 중 |
| **순수 Rust 재구현** (PIO USB 프로토콜 전체) | 완전한 safety, embassy async 통합 | USB FS 프로토콜 + PIO 프로그래밍 전체를 재구현해야 함. 검증 어려움 | 극상 |
| **하이브리드** (Core 1에서 C PIO-USB, Core 0에서 Rust embassy) | 각 코어가 독립 실행, 기존 검증 코드 재활용 | IPC 설계 필요, 두 빌드 시스템 관리 | 상 |

#### FFI 하이브리드의 구체적 충돌 지점

| 리소스 | Embassy-rp | Pico SDK + PIO-USB | 충돌 |
|--------|-----------|-------------------|------|
| PIO 블록 | move semantics (`Peripherals::take()`) | 런타임 `pio_sm_claim()` | PIO 블록 1개를 C 전용으로 예약, Embassy에서 해당 블록 접근 금지 필요 |
| DMA 채널 | 독립 할당 레지스트리 | 독립 `dma_claim_unused_channel()` | 이중 할당 위험. 수동 파티셔닝 필요 |
| 인터럽트 | `cortex-m-rt` — 벡터당 핸들러 1개 | Pico SDK IRQ 핸들러 | PIO IRQ를 C 측에 전담, Embassy 측에서 해당 IRQ 미사용 |
| 초기화 | `embassy_rp::init()` — 클럭, 리셋 전체 설정 | `stdio_init_all()` 등 | C 코드를 "library mode"로 빌드 (init 없음, vector table 없음). 클럭은 Embassy가 설정한 것을 C가 수용 |
| Critical section | `embassy-rp` 구현 | Pico SDK `spin_lock` | 동시 사용 시 deadlock 가능. 하나로 통일하거나 코어별 분리 |

### 7.4 Rust 포팅 시 이점

| 항목 | 상세 |
|------|------|
| 메모리 안전성 | 버퍼 오버플로, use-after-free 등 USB 파싱 취약점 구조적 방지 |
| 타입 시스템 | HID report descriptor → Rust enum/struct 매핑으로 파싱 오류 컴파일타임 검출 |
| async/await | embassy의 cooperative multitasking으로 Core 0의 device+host+UART+키 리매핑 태스크를 깔끔하게 스케줄링 |
| 안전한 동시성 | `embassy-sync`의 Channel/Signal로 코어 간 통신, data race 컴파일타임 방지 |
| 패키지 관리 | Cargo로 의존성 관리 (CMake + submodule 대비 개선) |
| 테스트 | `#[cfg(test)]` 기반 유닛 테스트, HID 파서 로직을 호스트에서 테스트 가능 |

### 7.5 Rust 포팅 시 위험

| 항목 | 상세 |
|------|------|
| PIO USB Host 부재 | 전체 포팅의 blocking issue. FFI 없이는 불가. 공개된 FFI 래퍼도 없음 |
| embassy-usb 런타임 제약 | interface 수/구성을 `builder.build()` 전에 결정해야 함. TinyUSB처럼 런타임 dynamic descriptor 교체가 불가능하여 passthrough의 동적 descriptor 생성 패턴에 부적합 |
| 바이너리 크기 | Rust의 monomorphization + panic handler로 인해 C 대비 Flash 사용량 증가 가능 (RP2040 Flash 2MB, 현재 사용 ~120KB이므로 여유 있음) |
| 커뮤니티 규모 | DeskHop upstream은 C 기반. Rust 포크는 upstream 추적이 어려워짐 |
| 학습 곡선 | embedded Rust + embassy + PIO 동시 학습 |
| FFI 복잡도 | C/Rust 하이브리드 시 PIO, DMA, IRQ 리소스 파티셔닝과 빌드 시스템 통합(CMake + Cargo)이 주요 삽질 요소 |

### 7.6 권장 전략

**단기 (Semi-DDM 구현)**: C 기반으로 DeskHop 위에 구현.
검증된 TinyUSB + Pico-PIO-USB 스택을 그대로 활용하고, passthrough 모듈과 키 리매핑 엔진만 추가. 가장 빠른 결과물 확보 경로.

**중기 (프로토타입 포팅)**: Rust로 application layer만 분리 포팅.
Core 0에서 Rust embassy (device USB + UART + 키 리매핑 + 상태 관리), Core 1에서 C PIO-USB host (FFI 바인딩). 하이브리드 구조로 Rust 이점을 부분 확보.

**장기 (Full Rust)**: PIO USB Host의 Rust 구현이 커뮤니티에서 등장하면 전체 포팅.
또는 RP2350 (Pico 2) 전환 시 네이티브 USB host 지원 가능성 검토.

### 7.7 Rust 하이브리드 빌드 구조 (참고)

```
deskhop-rs/
├── Cargo.toml
├── build.rs                    # cc crate로 C PIO-USB 빌드
├── src/
│   ├── main.rs                 # embassy entry, Core 0 executor
│   ├── passthrough.rs          # Semi-DDM 상태 관리
│   ├── key_remap.rs          # 키 리매핑 엔진
│   ├── usb_device.rs           # embassy-usb HID device
│   ├── uart_bridge.rs          # UART DMA 통신
│   └── ffi/
│       ├── mod.rs
│       └── pio_usb_host.rs     # Pico-PIO-USB C FFI 바인딩
├── c_src/
│   ├── pio_usb_wrapper.c       # C glue code
│   └── pio_usb_wrapper.h
└── memory.x                    # 링커 스크립트
```

---

## 8. 구현 로드맵

| 단계 | 내용 | 예상 기간 | 산출물 |
|------|------|-----------|--------|
| **P1** | Bolt 수신기 descriptor 덤프 | 1주 | hex dump 로그, interface 구조 분석 문서 |
| **P2** | 고정 descriptor passthrough | 2주 | Win11에서 수신기 전체 interface 인식 확인 |
| **P3** | HID++ 상시 양방향 통신 | 1주 | Android active 시에도 Options+ 연결 유지 확인 |
| **P4** | 동적 descriptor 생성 | 2주 | 런타임 캡처 → 동적 device descriptor |
| **P5** | 모드 분기 + 전환 | 1주 | 핫키로 Win11 ↔ Android 전환 |
| **P6** | 키 리매핑 엔진 (SIMPLE + TAP_HOLD) | 2주 | 범용 리매핑 동작, flash 저장, web config 편집 |
| **P7** | 통합 테스트 + 안정화 | 2주 | 장시간 사용 안정성, 엣지 케이스 처리 |
| **P8** | (선택) Rust 하이브리드 프로토타입 | 4주+ | Core 0 Rust + Core 1 C 빌드 확인 |

---

## 9. 검증 체크리스트

### 9.1 Passthrough 기능

- [ ] Win11 Device Manager에서 Bolt 수신기 interface 전부 인식
- [ ] Logitech Options+가 수신기 인식 및 연결 상태 표시
- [ ] 마우스 기본 동작 (이동, 클릭, 스크롤, 뒤로/앞으로)
- [ ] 마우스 확장 동작 (제스처 버튼, 썸휠, DPI 전환)
- [ ] Options+에서 설정 변경 (DPI, 버튼 매핑 등)
- [ ] 트리플 모니터 전체에서 마우스 정상 동작
- [ ] 키보드 기본 동작 (전체 키 입력)
- [ ] 키보드 Consumer Control (미디어 키)

### 9.2 HID++ 상시 연결 (Always-On)

- [ ] Android active 상태에서 Options+가 수신기를 "connected"로 표시
- [ ] Android active 상태에서 Options+ DPI/버튼 설정 변경 가능
- [ ] Android active 상태에서 마우스 배터리 잔량 Options+에서 조회 가능
- [ ] Output 전환 전후로 Options+ 세션이 끊기지 않음 (재연결 없음)
- [ ] HID++ short report (7 bytes) 양방향 전달 확인
- [ ] HID++ long report (20 bytes) 양방향 전달 확인

### 9.3 전환 기능

- [ ] 핫키(`Left CTRL + Caps Lock`)로 Win11 ↔ Android 전환
- [ ] 전환 후 Win11 측 USB 연결 유지 (re-enumeration 없음)
- [ ] 전환 후 Android 측 정상 HID 입력
- [ ] 전환 후 Options+ 연결 상태 유지

### 9.4 키 리매핑 엔진

- [ ] SIMPLE: 1:1 키 교체 동작 확인
- [ ] SIMPLE: modifier 키 교체 동작 확인 (예: CapsLock → Ctrl)
- [ ] TAP_HOLD: tap(< threshold) 시 tap_action 출력
- [ ] TAP_HOLD: hold(≥ threshold) 시 hold_action 출력
- [ ] TAP_HOLD: 빠른 연속 타이핑 시 오동작 없음
- [ ] output_mask: Output별 독립 적용 확인 (A만, B만, 양쪽)
- [ ] 리매핑 엔트리가 0개일 때 기존 동작과 동일 (패스스루)
- [ ] DeskHop 기존 핫키와 충돌 없음 (리매핑 후 핫키 평가 순서)
- [ ] Flash 저장/로드 정상 동작
- [ ] Web config UI에서 리매핑 편집 가능

### 9.5 안정성

- [ ] 24시간 연속 사용 시 행업/크래시 없음
- [ ] 수신기 물리적 분리/재연결 후 정상 복귀
- [ ] Watchdog 정상 동작
- [ ] Android 태블릿 저전력 모드 → 복귀 시 정상 동작

---

## 10. 참고 자료

### 10.1 프로젝트 기반

| 자료 | URL |
|------|-----|
| DeskHop 원본 | https://github.com/hrvach/deskhop |
| DeskHop 릴리스 (v0.77) | https://github.com/hrvach/deskhop/releases |
| Pico-PIO-USB | https://github.com/sekigon-gonnoc/Pico-PIO-USB |
| TinyUSB | https://github.com/hathach/tinyusb |

### 10.2 TinyUSB 동적 Descriptor 관련

| 자료 | URL |
|------|-----|
| TinyUSB Issue #802 — report descriptor 길이 문제 | https://github.com/hathach/tinyusb/issues/802 |
| TinyUSB Issue #2214 — 런타임 interface 할당 제안 | https://github.com/hathach/tinyusb/issues/2214 |
| TinyUSB Discussion #3242 — 동적 re-enumeration | https://github.com/hathach/tinyusb/discussions/3242 |
| GP2040-CE (동적 descriptor 선행 사례) | https://github.com/OpenStickCommunity/GP2040-CE |

### 10.3 Logitech Bolt / HID++

| 자료 | URL |
|------|-----|
| Logitech HID++ 2.0 Protocol Spec (Draft) | https://lekensteyn.nl/files/logitech/logitech_hidpp_2.0_specification_draft_2012-06-04.pdf |
| Solaar — Logitech 수신기 관리 (Bolt PID 정의) | https://github.com/pwr-Solaar/Solaar |
| logiops — Bolt 지원 이슈 (interface 구조 논의) | https://github.com/PixlOne/logiops/issues/300 |
| fwupd Logitech HID++ 플러그인 문서 | https://fwupd.github.io/libfwupdplugin/logitech-hidpp-README.html |

### 10.4 Rust Embedded / Embassy

| 자료 | URL |
|------|-----|
| Embassy (Rust) | https://github.com/embassy-rs/embassy |
| embassy-rp docs | https://docs.embassy.dev/embassy-rp/ |
| embassy-usb (crates.io) | https://crates.io/crates/embassy-usb |
| Embassy Issue #3295 — USB Host 지원 제안 (STM32 전용) | https://github.com/embassy-rs/embassy/issues/3295 |
| cotton-usb-host (RP2040 native USB host, Rust) | https://docs.rs/cotton-usb-host |
| pio-rs (PIO 프로그래밍 크레이트) | https://github.com/rp-rs/pio-rs |

### 10.5 하드웨어 사양

| 자료 | URL |
|------|-----|
| RP2040 Datasheet | https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf |
| USB HID Specification 1.11 | https://www.usb.org/document-library/device-class-definition-hid-111 |
| QMK Firmware (키 리매핑 참고) | https://docs.qmk.fm/ |

### 10.6 DDM / KVM 기술

| 자료 | URL |
|------|-----|
| DDM 기술 설명 (ConnectPRO) | https://www.connectpro.com/blogs/news/dynamic-device-mapping-ddm-and-how-it-works-in-a-kvm-switch |
| ConnectPRO DDM 특허 주장 (번호 미공개) | https://connectpro.com/new-patent-awarded-for-device-mapping-function-new-built-in-technology-looks-to-change-the-face-of-kvm-switches/ |
| Semi-DDM / Full DDM 비교 | https://kvmpro.blogspot.com/2012/12/classes-of-usb-kvm-switches.html |
| TESmart Passthrough/DDM 설명 | https://support.tesmart.com/hc/en-us/articles/25795807586457 |

---

## 부록 A. 문서 변경 이력

| 버전 | 날짜 | 변경 내용 |
|------|------|-----------|
| 0.1 | 2026-03-18 | 초안 작성 — HW 개요, 기존 아키텍처 분석, Semi-DDM 설계, Tap-Hold 모듈, Rust 포팅 검토, 로드맵 |
| 0.1a | 2026-03-18 | 태블릿 정보(Lenovo K10 Pro) 추가 (§1.4) |
| 0.1b | 2026-03-18 | HID++ 상시 양방향 통신 설계 반영 — interface 레벨 라우팅 분리 (§3.2, §3.3, §3.4 P3/P5, §9.2) |
| 0.1c | 2026-03-18 | Tap-Hold 모듈 → 범용 키 리매핑 엔진으로 재설계 (§4 전면 개편) |
| 0.2 | 2026-03-18 | 7개 영역 팩트체크 반영: UART baudrate 수정 (921600→3686400), Bolt 수신기 3 interface 구조 확정 (DJ 제거), PIO-USB SM/메모리 상세, TinyUSB 동적 descriptor 제약 및 선행 사례, Embassy-rs USB host 부재 확인 (Issue #3295 STM32 전용), RP2040 SRAM/Flash 예산 추가, DDM 특허 미검증 주의사항, 태블릿 모델 번호 경고 |

## 부록 B. 팩트체크 요약

v0.2에서 수정된 오류 및 보강 사항:

| 원래 기술 | 수정 | 근거 |
|-----------|------|------|
| UART baudrate 921,600 baud | **3,686,400 baud** | DeskHop `serial.h`의 `SERIAL_BAUDRATE` 정의. 120MHz 클럭에서 divider ratio ~32.55 |
| Bolt 수신기 4 interface (keyboard, mouse, HID++ short, HID++ long/DJ) | **3 interface** (keyboard, mouse, HID++ vendor). **DJ report 미사용** | logiops, Solaar, fwupd 소스 코드. Linux 커널 `hid-logitech-dj`가 Bolt를 처리하지 않음 |
| Bolt PID 미기재 | **VID=0x046D, PID=0xC548**, Full-Speed, 98mA, 내부 hub 없음 | Linux 커널 `USB_DEVICE_ID_LOGITECH_BOLT_RECEIVER`, Solaar `base_usb.py` |
| PIO-USB 리소스 미상세 | **SM 3개 + 명령어 메모리 32 words** (PIO 블록 1개). Host 제한: 장치 4, endpoint 32, hub port 8 | Pico-PIO-USB 소스 코드 및 README |
| TinyUSB 동적 descriptor "가능" | 가능하나 **`wReportLength` 일치 필수** (Issue #802). Re-enumeration은 `tud_disconnect()` + 200ms + `tud_connect()` (Discussion #3242) | TinyUSB GitHub issues/discussions |
| Embassy-rs USB Host "없음" | 확인: **device 전용**. Issue #3295는 STM32 전용 프로토타입, RP2040 무관. `cotton-usb-host`는 네이티브 USB 전용 (PIO 아님) | embassy-rs 리포지토리, docs.rs |
| RP2040 SRAM/Flash 미상세 | SRAM ~30-50KB/264KB 사용, Flash ~120KB+α/2MB. `CFG_TUD_HID` 2→6 증가 시 추가 SRAM ~832B | TinyUSB `hid_device.c` 구조체 크기 분석, DeskHop Issue #77 |
| DDM "ConnectPRO 특허" | 특허 주장은 있으나 **번호 미공개, USPTO 검색 불가**. 마케팅 용어로 취급 | ConnectPRO 블로그, USPTO/Google Patents 검색 결과 |
| 태블릿 모델 TB331FU | **TB331FU = Tab M11**, K10 Pro와 별개 제품. K10 Pro 정확한 모델 번호 미확인 (중국/수출 전용) | Amazon/eBay/부품 판매 사이트 교차 확인 |

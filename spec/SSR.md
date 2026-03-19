# DeskHop Semi-DDM 펌웨어 소프트웨어 요구사항 명세서 (SSR)

| 항목 | 내용 |
|------|------|
| 문서 ID | DESKHOP-SSR-001 |
| 버전 | 1.0 |
| 최종 수정 | 2026-03-19 |
| 상태 | Draft |
| 관련 문서 | TECHSTACK.md (v0.2) |

---

## 1. 개요

### 1.1 목적

본 문서는 DeskHop PCB v1.1 기반 Semi-DDM USB passthrough 펌웨어의 소프트웨어 요구사항을 정의한다.
기존 DeskHop 오픈소스 펌웨어(v0.77)를 기반으로 확장하며, 세 가지 핵심 기능을 추가한다:

1. **Semi-DDM USB Passthrough** — Win11 output에 Logitech Bolt 수신기의 원본 HID descriptor/report를 투명하게 전달
2. **HID++ Always-On 양방향 통신** — output 전환과 무관하게 HID++ vendor interface의 상시 연결 유지
3. **키 리매핑 엔진** — 호스트 소프트웨어 없이 펌웨어 레벨에서 동작하는 범용 키 입력 변환

### 1.2 범위

| 범위 내 | 범위 외 |
|---------|---------|
| DeskHop PCB v1.1 Pico A (Win11) 펌웨어 수정 | PCB 하드웨어 회로 변경 |
| DeskHop PCB v1.1 Pico B (Android) 펌웨어 수정 | Logitech Options+ 소프트웨어 수정 |
| Logitech Bolt 수신기(PID 0xC548) 대상 passthrough | Unifying 수신기, 기타 vendor 수신기 |
| 키보드 HID report 리매핑 | 마우스 report 리매핑 |
| Web config UI 확장 (리매핑 설정) | 새로운 config UI 프레임워크 |
| 기존 DeskHop 기능 유지 (하위 호환) | RP2350/Pico 2 포팅 |

### 1.3 용어 및 약어

| 용어 | 정의 |
|------|------|
| Output A | Win11 노트북에 연결된 Pico (passthrough 대상) |
| Output B | Lenovo K10 Pro Android 태블릿에 연결된 Pico |
| Active output | 현재 키보드/마우스 입력이 전달되는 output |
| Passthrough | 수신기의 USB HID report를 파싱 없이 그대로 컴퓨터에 전달하는 모드 |
| Semi-DDM | active output에만 실제 장치 descriptor를 노출하고, inactive에는 emulation을 유지하는 KVM 방식 |
| HID++ | Logitech 전용 vendor-specific HID 프로토콜 (short: 7B, long: 20B) |
| Always-On | HID++ vendor interface가 active output과 무관하게 상시 양방향 통신을 유지하는 동작 |
| Bolt 수신기 | Logitech Logi Bolt USB 수신기 (VID=0x046D, PID=0xC548) |

### 1.4 대상 하드웨어

| 항목 | 사양 |
|------|------|
| 보드 | DeskHop PCB v1.1 (Raspberry Pi Pico × 2, TI ISO7721DR, TPD4E1U06DBVR) |
| MCU | RP2040 (Cortex-M0+ dual-core, 264KB SRAM, 2MB Flash) |
| 시스템 클럭 | 120 MHz |
| 수신기 | Logitech Bolt (PID 0xC548), 3 HID interface, Full-Speed, 98mA |
| Output A | Win11 노트북 (트리플 모니터) |
| Output B | Lenovo K10 Pro 10.6" (Helio G80, Android 12, USB-C 2.0 OTG) |

---

## 2. 시스템 컨텍스트

### 2.1 시스템 구성

```
                          ┌──────────────────────────────────┐
                          │        DeskHop PCB v1.1          │
                          │                                  │
 [Logitech Bolt          │  ┌─────────────┐  UART+ISO  ┌─────────────┐
  수신기] ──── USB-A ─────┤──│  Pico A      │◄──────────►│  Pico B      │
                          │  │  (Output A)  │  7721DR   │  (Output B) │
                          │  │  PIO=Host    │ 3.6864Mbps│  PIO=Host   │
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

### 2.2 Bolt 수신기 USB Interface 구조

| Interface | Protocol | Endpoint | Report IDs | 용도 | 라우팅 정책 |
|-----------|----------|----------|------------|------|------------|
| 0 | Boot Keyboard | EP 0x81 | 없음 (15B bitmap) | 키보드 | Active output 의존 |
| 1 | Boot Mouse | EP 0x82 | 0x02 (mouse), 0x03 (consumer), 0x04 (system) | 마우스 + 미디어 | Active output 의존 |
| 2 | Vendor (HID++) | EP 0x83 | 0x10 (short 7B), 0x11 (long 20B) | HID++ 제어 | **Always-On** (상시 Win11) |

### 2.3 동작 모드

| 모드 | Active Output | Keyboard | Mouse | HID++ Vendor |
|------|--------------|----------|-------|-------------|
| **Passthrough** | Win11 (A) | 키 리매핑 엔진 경유 → Win11 | Raw passthrough → Win11 | Passthrough → Win11 |
| **Legacy** | Android (B) | 키 리매핑 엔진 경유 → 파싱 → UART → Android | 파싱 → UART → Android | **Always-On → Win11** |

---

## 3. 기능 요구사항 (Functional Requirements)

### 3.1 Semi-DDM USB Passthrough

#### FR-PT-001: Host Descriptor 캡처

| 항목 | 내용 |
|------|------|
| ID | FR-PT-001 |
| 우선순위 | 필수 |
| 설명 | Pico A의 PIO-USB Host가 Bolt 수신기 enumeration 시, 모든 HID interface의 raw report descriptor를 캡처하여 내부 버퍼에 저장해야 한다. |
| 입력 | `tuh_hid_mount_cb()` 콜백, `tuh_descriptor_get_hid_report()` 결과 |
| 출력 | `passthrough_iface_t` 구조체 배열 (interface 수, descriptor 데이터, 길이, protocol, endpoint 정보) |
| 수용 조건 | Bolt 수신기의 3개 HID interface (keyboard 67B, mouse 133B, vendor HID++) 모두 캡처 성공 |

#### FR-PT-002: 동적 Device Descriptor 생성

| 항목 | 내용 |
|------|------|
| ID | FR-PT-002 |
| 우선순위 | 필수 |
| 설명 | 캡처된 descriptor를 기반으로 Pico A의 USB Device 측 configuration descriptor와 HID report descriptor를 런타임에 구성해야 한다. |
| 제약 | TinyUSB `tud_descriptor_configuration_cb()`에서 동적 버퍼 반환. 각 interface의 `wReportLength`가 캡처된 `desc_len`과 정확히 일치해야 한다. |
| 수용 조건 | Win11 Device Manager에서 Bolt 수신기와 동일한 수의 HID interface가 인식된다. |

#### FR-PT-003: USB Re-enumeration

| 항목 | 내용 |
|------|------|
| ID | FR-PT-003 |
| 우선순위 | 필수 |
| 설명 | 수신기 host enumeration 완료 후, device 측 descriptor를 교체하고 Win11에 re-enumeration을 트리거해야 한다. |
| 절차 | `tud_disconnect()` → descriptor 버퍼 구성 → 200ms 대기 → `tud_connect()` |
| 수용 조건 | Re-enumeration 후 Win11이 새 descriptor로 장치를 정상 인식한다. |

#### FR-PT-004: Mouse Report Passthrough (Input)

| 항목 | 내용 |
|------|------|
| ID | FR-PT-004 |
| 우선순위 | 필수 |
| 설명 | Win11이 active output일 때, mouse interface (Interface 1)의 input report를 파싱 없이 raw 그대로 `tud_hid_n_report()`로 Win11에 전달해야 한다. |
| 수용 조건 | 이동, 클릭 (8버튼), 수직/수평 스크롤, consumer control (미디어 키), system control이 모두 직접 연결과 동일하게 동작한다. |

#### FR-PT-005: Mouse 확장 기능

| 항목 | 내용 |
|------|------|
| ID | FR-PT-005 |
| 우선순위 | 필수 |
| 설명 | Passthrough 모드에서 Logitech MX 시리즈의 확장 기능(제스처 버튼, 썸휠, DPI 전환, MagSpeed 무한스크롤)이 Options+와 연동하여 동작해야 한다. |
| 전제 | FR-PT-004 (raw passthrough) + FR-AO-002 (HID++ output report 전달) |
| 수용 조건 | VID/PID 전환(FR-PT-009) 후 Options+에서 DPI 변경, 버튼 리매핑 설정이 정상 적용되고, 제스처 버튼이 동작한다. |

#### FR-PT-006: 트리플 모니터 지원

| 항목 | 내용 |
|------|------|
| ID | FR-PT-006 |
| 우선순위 | 필수 |
| 설명 | Passthrough 모드에서 Win11 트리플 모니터 환경에서 마우스 커서가 전체 화면에 걸쳐 정상 동작해야 한다. |
| 근거 | Bolt 수신기의 원본 relative mouse report가 직접 전달되므로, Windows KB5003637의 absolute coordinate 제한 영향을 받지 않는다. |
| 수용 조건 | 3개 모니터 모두에서 마우스 커서 이동, 클릭, 드래그가 정상 동작한다. |

#### FR-PT-007: Keyboard Report 파이프라인

| 항목 | 내용 |
|------|------|
| ID | FR-PT-007 |
| 우선순위 | 필수 |
| 설명 | 키보드 interface (Interface 0)의 report는 passthrough 모드에서도 **항상 파싱 경로**를 유지한다. 파싱 → 키 리매핑 엔진 → DeskHop 핫키 체크 → send_key() 순서로 처리한다. |
| 근거 | 키 리매핑 엔진이 키코드 레벨 접근을 필요로 하며, Logitech 키보드의 vendor 기능은 HID++ interface를 통해 별도 처리된다. |
| 수용 조건 | 모든 키 입력이 정상 동작하고, 리매핑 규칙이 적용된다. |

#### FR-PT-008: Legacy 모드 (Android Active)

| 항목 | 내용 |
|------|------|
| ID | FR-PT-008 |
| 우선순위 | 필수 |
| 설명 | Android가 active output일 때, keyboard/mouse report는 기존 DeskHop 방식으로 파싱 후 UART를 통해 Pico B에 전달하고, Pico B가 고정 HID descriptor로 Android에 출력한다. |
| 수용 조건 | Android 태블릿에서 키보드 입력, 마우스 이동/클릭/스크롤이 정상 동작한다. |

#### FR-PT-009: 조건부 VID/PID 전환

| 항목 | 내용 |
|------|------|
| ID | FR-PT-009 |
| 우선순위 | 필수 |
| 설명 | Passthrough 활성화 시 USB Device Descriptor의 VID/PID를 upstream 디바이스(Bolt 수신기)와 동일하게 노출해야 한다. Logitech VID(0x046D) 감지 시 제조사/제품 string descriptor도 "Logitech" / "USB Receiver"로 전환한다. |
| 전제 | FR-PT-001 (descriptor 캡처) + FR-PT-003 (re-enumeration) |
| 구현 | `tuh_hid_mount_cb()`에서 `tuh_vid_pid_get()`로 VID/PID 캡처 → `passthrough_state_t.upstream_vid/pid`에 저장 → re-enumeration 시 `tud_descriptor_device_cb()`에서 upstream VID/PID 반환 |
| 수용 조건 | Re-enumeration 후 호스트 시스템 리포트에서 VID=0x046D, PID=0xC548로 표시되고, Logi Options+가 디바이스를 인식한다. |

#### FR-PT-010: Unmount 시 VID/PID 원복

| 항목 | 내용 |
|------|------|
| ID | FR-PT-010 |
| 우선순위 | 필수 |
| 설명 | Upstream 디바이스(Bolt 수신기) 분리 시, passthrough를 비활성화하고 DeskHop 원래 VID/PID(0x1209/0xC000)로 자동 재열거해야 한다. |
| 절차 | `tuh_hid_umount_cb()` → `passthrough_remove_device()` → iface_count==0 → `active=false`, VID/PID 클리어 → `tud_disconnect()` → 200ms → `tud_connect()` |
| 수용 조건 | Bolt 분리 후 호스트에서 DeskHop VID/PID로 다시 인식되고, 기본 키보드/마우스 기능이 정상 동작한다. |

---

### 3.2 HID++ Always-On 양방향 통신

#### FR-AO-001: HID++ 프로토콜 메시지 상시 전달

| 항목 | 내용 |
|------|------|
| ID | FR-AO-001 |
| 우선순위 | 필수 |
| 설명 | HID++ **프로토콜 메시지**(Options+ query/response)는 active output과 **무관하게** 항상 Win11 ↔ 수신기 간 전달해야 한다. 프로토콜 메시지와 입력 이벤트는 HID++ 2.0의 sw_id 필드(byte[3] 하위 4비트)로 구분한다: sw_id≠0은 프로토콜 응답, sw_id=0은 입력 이벤트. |
| 수용 조건 | Android active 상태에서 Options+가 수신기를 "connected"로 표시하고 설정 변경이 가능하다. |

#### FR-AO-002: HID++ Output Report 상시 전달

| 항목 | 내용 |
|------|------|
| ID | FR-AO-002 |
| 우선순위 | 필수 |
| 설명 | Win11(Options+)이 HID++ vendor interface로 보내는 output report는 active output과 **무관하게** 항상 Bolt 수신기에 전달해야 한다. |
| 구현 | `tud_hid_set_report_cb()`에서 큐잉 → `passthrough_task()`에서 `tuh_control_xfer()`로 전송. wLength는 full report 크기(report_id 포함). |
| 수용 조건 | Android active 상태에서 Options+ DPI 변경, 버튼 리매핑, 배터리 잔량 조회가 정상 동작한다. |

#### FR-AO-005: HID++ 입력 이벤트의 Active Output 라우팅

| 항목 | 내용 |
|------|------|
| ID | FR-AO-005 |
| 우선순위 | 필수 |
| 설명 | Logitech MX 시리즈 마우스의 고급 입력(HiRes Scroll, 사이드/썸/제스처 버튼)은 HID++ vendor interface를 통해 전달된다. 이러한 **입력 이벤트**는 active output에만 전달되어야 하며, 비활성 output에 입력이 누출되어서는 안 된다. |
| 근거 | 실기 테스트에서 확인: Feature 0x0E (HiRes Scroll), Feature 0x09 (Reprog Controls) 등의 HID++ 입력 이벤트가 `always_passthrough=true`로 인해 항상 Win11에 전달되어, Android active 시 휠/특수 버튼이 Win11에서 동작하는 문제 발생. |
| 구분 기준 | HID++ 2.0 report에서 sw_id (byte[3]의 하위 4비트): sw_id=0 → unsolicited 입력 이벤트, sw_id≠0 → 프로토콜 응답. |
| 수용 조건 | Android active 시 휠 스크롤, 사이드 버튼, 썸 버튼, 제스처 버튼이 Win11에서 동작하지 않는다. Win11 active 시에는 모든 입력이 정상 동작한다. |

#### FR-AO-003: HID++ 세션 연속성

| 항목 | 내용 |
|------|------|
| ID | FR-AO-003 |
| 우선순위 | 필수 |
| 설명 | Output 전환(Win11 ↔ Android) 전후로 HID++ 통신 세션이 끊기지 않아야 한다. Options+의 재연결 과정(device re-discovery)이 발생하지 않아야 한다. |
| 수용 조건 | 전환 직후 Options+에서 장치 상태가 즉시 표시되며, "Searching for device" 또는 연결 끊김 표시가 나타나지 않는다. |

#### FR-AO-004: Interface 유형별 라우팅 분기

| 항목 | 내용 |
|------|------|
| ID | FR-AO-004 |
| 우선순위 | 필수 |
| 설명 | `tuh_hid_report_received_cb()`에서 interface 유형에 따라 라우팅을 분기해야 한다: (1) `always_passthrough == true` → 항상 Win11, (2) keyboard → 리매핑 엔진 경유 후 active output, (3) mouse → active output에 따라 passthrough 또는 파싱. |
| 수용 조건 | 세 유형의 report가 각각 의도된 경로로 전달되며, 혼선이 없다. |

---

### 3.3 Output 전환

#### FR-SW-001: 핫키 전환

| 항목 | 내용 |
|------|------|
| ID | FR-SW-001 |
| 우선순위 | 필수 |
| 설명 | 기존 DeskHop 핫키 `Left CTRL + Caps Lock`으로 Win11 ↔ Android 간 active output을 전환해야 한다. |
| 수용 조건 | 핫키 입력 후 1초 이내에 active output이 전환된다. |

#### FR-SW-002: 전환 시 Win11 USB 연결 유지

| 항목 | 내용 |
|------|------|
| ID | FR-SW-002 |
| 우선순위 | 필수 |
| 설명 | Win11 → Android 전환 시 Pico A의 USB device 연결이 유지되어야 한다. USB disconnect/re-enumeration이 발생하지 않아야 한다. |
| 근거 | HID++ Always-On 및 Options+ 세션 유지를 위해 필수. |
| 수용 조건 | 전환 전후로 Win11 Device Manager에서 장치 목록 변동이 없다. |

#### FR-SW-003: 전환 시 키보드/마우스 라우팅 변경

| 항목 | 내용 |
|------|------|
| ID | FR-SW-003 |
| 우선순위 | 필수 |
| 설명 | 전환 시 keyboard/mouse report의 라우팅 대상만 변경한다. HID++ vendor interface 라우팅은 변경하지 않는다 (FR-AO-003). |
| 수용 조건 | 전환 직후 새로운 active output에서 키보드/마우스 입력이 즉시 동작한다. |

#### FR-SW-004: 마우스 자동 전환 비활성 지원

| 항목 | 내용 |
|------|------|
| ID | FR-SW-004 |
| 우선순위 | 선택 |
| 설명 | 기존 DeskHop의 마우스 화면 경계 감지 자동 전환을 비활성화하고 핫키 전환만 사용할 수 있어야 한다. |
| 구현 | DeskHop의 기존 `switch_lock` 기능 또는 gaming mode 활용. |
| 수용 조건 | 마우스를 화면 끝으로 이동해도 output 전환이 발생하지 않는다. |

---

### 3.4 키 리매핑 엔진

#### FR-RM-001: SIMPLE 리매핑

| 항목 | 내용 |
|------|------|
| ID | FR-RM-001 |
| 우선순위 | 필수 |
| 설명 | 단일 키코드를 다른 키코드로 1:1 교체해야 한다. 일반 키와 modifier 키 모두 대상으로 가능해야 한다. |
| 예시 | CapsLock → Left Ctrl, Right Alt → LANG1 |
| 수용 조건 | 매핑된 trigger 키 입력 시 replacement 키코드가 OS에 전달된다. |

#### FR-RM-002: TAP_HOLD 리매핑

| 항목 | 내용 |
|------|------|
| ID | FR-RM-002 |
| 우선순위 | 필수 |
| 설명 | 키를 짧게 누르면(< threshold) tap_action을 출력하고, 길게 누르면(≥ threshold) hold_action을 출력해야 한다. |
| 상태 머신 | IDLE → (key down) → WAITING → (key up < threshold) → emit tap / (time ≥ threshold) → HELD → (key up) → release hold |
| 기본 threshold | 200ms (설정 가능) |
| 예시 | Tab: tap=LANG1(한/영), hold=Tab |
| 수용 조건 | threshold 미만 입력 시 tap_action만 발생하고, threshold 이상 유지 시 hold_action이 발생한다. |

#### FR-RM-003: Output별 독립 적용

| 항목 | 내용 |
|------|------|
| ID | FR-RM-003 |
| 우선순위 | 필수 |
| 설명 | 각 리매핑 엔트리에 `output_mask`를 설정하여 Output A, Output B, 또는 양쪽 모두에 독립적으로 적용할 수 있어야 한다. |
| 수용 조건 | Output A 전용 리매핑이 Output B에서 적용되지 않으며, 그 역도 성립한다. |

#### FR-RM-004: 리매핑 엔진 파이프라인 위치

| 항목 | 내용 |
|------|------|
| ID | FR-RM-004 |
| 우선순위 | 필수 |
| 설명 | 리매핑 엔진은 `extract_kbd_data()` → `update_kbd_state()` 이후, `check_all_hotkeys()` 이전에 실행되어야 한다. DeskHop 기존 핫키는 리매핑 결과에 대해 평가한다. |
| 수용 조건 | 리매핑과 DeskHop 핫키가 충돌 없이 동작한다. |

#### FR-RM-005: 빈 설정 시 패스스루

| 항목 | 내용 |
|------|------|
| ID | FR-RM-005 |
| 우선순위 | 필수 |
| 설명 | 리매핑 엔트리가 0개일 때 키보드 report는 수정 없이 그대로 전달되어야 한다. 기존 DeskHop 동작과 완전히 동일해야 한다. |
| 수용 조건 | 리매핑 설정 없이 모든 키가 원래대로 동작한다. |

#### FR-RM-006: 타이머 기반 상태 전환

| 항목 | 내용 |
|------|------|
| ID | FR-RM-006 |
| 우선순위 | 필수 |
| 설명 | TAP_HOLD의 threshold 초과 등 시간 기반 상태 전환을 위해 주기적 타이머 체크를 수행해야 한다. 키 이벤트가 없어도 상태 전환이 발생해야 한다. |
| 구현 | `remap_engine_tick()`을 main loop에서 1ms 주기로 호출. |
| 수용 조건 | 키를 누른 채 threshold 시간이 경과하면 hold_action이 자동으로 발생한다. |

#### FR-RM-007: 설정 영속 저장

| 항목 | 내용 |
|------|------|
| ID | FR-RM-007 |
| 우선순위 | 필수 |
| 설명 | 리매핑 설정은 Flash에 저장하여 전원 차단 후에도 유지되어야 한다. 기존 DeskHop config 영역을 확장한다. |
| 제약 | 최대 16개 리매핑 엔트리. |
| 수용 조건 | 전원 재투입 후 저장된 리매핑 설정이 자동 로드되어 동작한다. |

#### FR-RM-008: Web Config UI 편집

| 항목 | 내용 |
|------|------|
| ID | FR-RM-008 |
| 우선순위 | 선택 (P6 이후) |
| 설명 | DeskHop의 기존 web config mode (WebHID 기반)를 확장하여 리매핑 엔트리의 추가/수정/삭제를 GUI에서 수행할 수 있어야 한다. |
| 수용 조건 | Chromium 브라우저에서 리매핑 설정을 편집하고 저장할 수 있다. |

#### FR-RM-009: 확장 리매핑 타입 (향후)

| 항목 | 내용 |
|------|------|
| ID | FR-RM-009 |
| 우선순위 | 선택 (향후 확장) |
| 설명 | 데이터 구조는 TAP_DANCE, COMBO, MODIFIER_MORPH, MACRO 타입을 수용할 수 있도록 union 기반으로 설계한다. 초기 구현은 SIMPLE + TAP_HOLD만 포함한다. |
| 수용 조건 | 향후 타입 추가 시 `remap_entry_t` 구조체 변경 없이 union 멤버만 추가하면 된다. |

---

### 3.5 하위 호환성

#### FR-BC-001: 기존 DeskHop 기능 유지

| 항목 | 내용 |
|------|------|
| ID | FR-BC-001 |
| 우선순위 | 필수 |
| 설명 | 기존 DeskHop v0.77의 모든 기능(핫키 전환, 화면 경계 전환, screensaver, gaming mode, LED 피드백, firmware upgrade, web config)이 정상 동작해야 한다. |
| 수용 조건 | DeskHop README에 기술된 모든 기능이 수정 후에도 동작한다. |

#### FR-BC-002: Bolt 이외 수신기 호환

| 항목 | 내용 |
|------|------|
| ID | FR-BC-002 |
| 우선순위 | 필수 |
| 설명 | Bolt 수신기가 연결되지 않은 경우(일반 유선/무선 키보드·마우스) 기존 DeskHop 동작으로 fallback해야 한다. Passthrough 모드는 비활성 상태로 유지한다. |
| 수용 조건 | 일반 USB 키보드/마우스로 기존과 동일하게 동작한다. |

---

## 4. 비기능 요구사항 (Non-Functional Requirements)

### 4.1 성능

| ID | 항목 | 요구사항 |
|----|------|---------|
| NFR-P-001 | Passthrough 지연 | Host report 수신 → Device report 전송 간 추가 지연 ≤ 1ms |
| NFR-P-002 | 키 리매핑 처리 시간 | `remap_engine_process()` 단일 호출 ≤ 100μs |
| NFR-P-003 | Output 전환 시간 | 핫키 입력 → 새 output에서 첫 입력 전달 ≤ 50ms |
| NFR-P-004 | Re-enumeration 시간 | `tud_disconnect()` → Win11 장치 인식 완료 ≤ 3초 (부팅 시 1회만 발생) |
| NFR-P-005 | 마우스 폴링 레이트 | 1000 Hz 유지 (기존 DeskHop 수준) |

### 4.2 안정성

| ID | 항목 | 요구사항 |
|----|------|---------|
| NFR-S-001 | 연속 동작 | 24시간 연속 사용 시 행업/크래시 없음 |
| NFR-S-002 | Watchdog | 기존 DeskHop watchdog 유지. 크래시 시 자동 재부팅 및 정상 복귀 |
| NFR-S-003 | 수신기 분리/재연결 | Bolt 수신기 물리적 분리 후 재연결 시 자동으로 descriptor 캡처 및 passthrough 재개 |
| NFR-S-004 | 태블릿 저전력 복귀 | Android 태블릿 절전 모드 → 복귀 시 USB HID 연결 자동 복구 |

### 4.3 리소스

| ID | 항목 | 요구사항 | 예산 |
|----|------|---------|------|
| NFR-R-001 | SRAM | 추가 SRAM 사용량 ≤ 10 KB | 기존 ~30-50 KB / 총 264 KB. 여유 ~200 KB |
| NFR-R-002 | Flash | 추가 Flash 사용량 ≤ 50 KB | 기존 ~120 KB / 총 2 MB. 여유 ~1.7 MB |
| NFR-R-003 | DMA | 추가 DMA 채널 사용 없음 (기존 3채널 유지) | 총 12채널, 9채널 여유 |
| NFR-R-004 | PIO | 추가 PIO SM 사용 없음 (기존 3 SM 유지) | PIO 블록 1개 점유 (SM 3개 + 명령어 32 words) |
| NFR-R-005 | USB Endpoint | Device 측 endpoint ≤ 10개 | 총 16개. Bolt 3 interface + 기존 DeskHop 2-4개 |

### 4.4 보안

| ID | 항목 | 요구사항 |
|----|------|---------|
| NFR-SEC-001 | 정보 격리 | 기존 DeskHop의 보안 원칙 유지: Output A ↔ Output B 간 정보 공유 없음. HID++ 데이터는 Win11 ↔ 수신기 간만 전달, Android 측으로 누출 없음 |
| NFR-SEC-002 | Config mode | 기존 DeskHop의 config mode 보안 유지: 명시적 핫키 진입, 비활성 타이머, report 크기 검증 |
| NFR-SEC-003 | Input 전용 | 장치가 자발적으로 keystroke를 생성하지 않는다 (기존 원칙 유지). 리매핑 엔진의 출력도 사용자 입력에 대한 변환에 한정한다. |

### 4.5 유지보수

| ID | 항목 | 요구사항 |
|----|------|---------|
| NFR-M-001 | 빌드 호환 | 기존 DeskHop 빌드 환경(CMake + arm-none-eabi-gcc)으로 빌드 가능 |
| NFR-M-002 | 단일 바이너리 | Pico A/B 구분 없이 단일 .uf2 이미지로 배포. 보드 역할은 autoprobe로 자동 결정 (기존 방식 유지) |
| NFR-M-003 | 펌웨어 업그레이드 | 기존 DeskHop OTA 업그레이드 (config mode USB drive 복사) 호환 유지 |
| NFR-M-004 | 코딩 스타일 | DeskHop upstream 코딩 스타일 준수 (C11, 영문 주석, TinyUSB 콜백 패턴) |

---

## 5. 인터페이스 요구사항

### 5.1 USB Host Interface (Pico A ↔ Bolt 수신기)

| ID | 요구사항 |
|----|---------|
| IF-UH-001 | PIO-USB Host (GP14/GP15)로 Bolt 수신기(Full-Speed)를 enumerate |
| IF-UH-002 | 3개 HID interface 모두에 대해 `tuh_hid_receive_report()` 지속 호출로 interrupt IN transfer 유지 |
| IF-UH-003 | HID++ output report: `tuh_hid_set_report()`로 control transfer 전송 |

### 5.2 USB Device Interface (Pico A ↔ Win11)

| ID | 요구사항 |
|----|---------|
| IF-UD-001 | Native USB (RHPORT 0)로 Win11에 HID composite device로 인식 |
| IF-UD-002 | Passthrough 모드: 캡처된 descriptor 기반 동적 configuration/report descriptor 노출 |
| IF-UD-003 | Legacy 모드: 기존 DeskHop 고정 descriptor 노출 |
| IF-UD-004 | `CFG_TUD_HID = 6`으로 최대 HID instance 확보 (Bolt 3개 + 여유) |

### 5.3 UART Interface (Pico A ↔ Pico B)

| ID | 요구사항 |
|----|---------|
| IF-UA-001 | UART0, 3,686,400 baud, 8N1, DMA 기반 송수신 |
| IF-UA-002 | 기존 DeskHop 고정 길이 패킷 프로토콜 유지 |
| IF-UA-003 | Galvanic isolation (TI ISO7721DR / ADuM1201) 경유 |

### 5.4 USB Device Interface (Pico B ↔ Android)

| ID | 요구사항 |
|----|---------|
| IF-UB-001 | Native USB로 Android에 표준 HID device (keyboard + mouse) 인식 |
| IF-UB-002 | 기존 DeskHop 고정 descriptor 사용 (passthrough 미적용) |
| IF-UB-003 | USB-C OTG 연결, 5V 전원은 태블릿에서 공급 (최대 500mA) |

### 5.5 사용자 인터페이스

| ID | 요구사항 |
|----|---------|
| IF-UI-001 | LED 피드백: 기존 DeskHop LED 동작 유지 (active output 표시, 핫키 인식 확인) |
| IF-UI-002 | Web config: 기존 config mode (Left Ctrl + Right Shift + C + O) 진입. Chromium/Chrome WebHID |
| IF-UI-003 | Web config 확장: 리매핑 엔트리 CRUD (추가/수정/삭제/저장) UI |

---

## 6. 제약사항

### 6.1 하드웨어 제약

| ID | 제약 | 영향 |
|----|------|------|
| CON-HW-001 | RP2040 USB endpoint 16개 | Device 측 HID interface 수 제한. Bolt 3 + 기존 2-4 = 최대 7개 사용 가능 |
| CON-HW-002 | PIO 블록 1개가 PIO-USB에 전용 | PIO 기반 추가 기능(WS2812 LED 등) 사용 시 나머지 블록 사용 |
| CON-HW-003 | UART 단일 채널 (isolated) | Pico 간 통신 대역폭 3.6864 Mbps 고정. Raw report passthrough에는 충분하나 대용량 데이터 전송 불가 |
| CON-HW-004 | Android 태블릿 USB-C 1포트 | DeskHop 연결 시 유선 충전 불가 |

### 6.2 소프트웨어 제약

| ID | 제약 | 영향 |
|----|------|------|
| CON-SW-001 | TinyUSB `CFG_TUD_HID` 컴파일타임 고정 | 최대 HID instance 수를 사전에 결정해야 함 (6으로 설정) |
| CON-SW-002 | TinyUSB report descriptor 길이 = `wReportLength` | Configuration descriptor의 `wReportLength`와 `tud_hid_descriptor_report_cb()` 반환 버퍼 크기가 정확히 일치해야 함 |
| CON-SW-003 | Re-enumeration 시 200ms 지연 필수 | 수신기 enumeration → device descriptor 구성 → re-enumeration 사이에 최소 200ms 대기 |
| CON-SW-004 | Bolt 수신기 전용 최적화 | Passthrough는 PID 0xC548 (Bolt)에 대해 검증. 타 수신기는 fallback(FR-BC-002) |
| CON-SW-005 | DeskHop upstream 의존 | 기존 DeskHop 코드 베이스(v0.77) 위에 구축. Upstream 업데이트 시 merge 충돌 가능 |

### 6.3 프로토콜 제약

| ID | 제약 | 영향 |
|----|------|------|
| CON-PR-001 | Bolt는 DJ report 미사용 | Unifying 수신기와 다른 프로토콜 구조. DJ 기반 로직은 구현하지 않음 |
| CON-PR-002 | HID++ 양방향 통신 | Options+의 feature request에 수신기가 응답하려면 output report(PC→수신기) 경로도 필수 |
| CON-PR-003 | USB Full-Speed 전용 | Pico-PIO-USB는 High-Speed 미지원. Bolt는 Full-Speed이므로 문제 없음 |

---

## 7. 요구사항 추적 매트릭스

### 7.1 기능 요구사항 → 구현 Phase 매핑

| 요구사항 | Phase | 소스 파일 |
|---------|-------|-----------|
| FR-PT-001 | P1 | `usb.c`, `passthrough.c` |
| FR-PT-002 | P2, P4 | `usb_descriptors.c`, `passthrough.c` |
| FR-PT-003 | P4 | `passthrough.c` |
| FR-PT-004 | P2 | `usb.c` |
| FR-PT-005 | P3 | `usb.c` |
| FR-PT-006 | P2 | (passthrough 구현 시 자동 해결) |
| FR-PT-007 | P2 | `keyboard.c`, `key_remap.c` |
| FR-PT-008 | P5 | `usb.c`, `handlers.c` |
| FR-AO-001 | P3 | `usb.c`, `passthrough.c` |
| FR-AO-002 | P3 | `usb.c`, `passthrough.c` |
| FR-AO-003 | P5 | `handlers.c`, `passthrough.c` |
| FR-AO-004 | P3, P5 | `usb.c` |
| FR-SW-001 | P5 | `keyboard.c`, `handlers.c` |
| FR-SW-002 | P5 | `handlers.c` |
| FR-SW-003 | P5 | `handlers.c`, `passthrough.c` |
| FR-SW-004 | — | 기존 DeskHop 기능 활용 |
| FR-RM-001 | P6 | `key_remap.c` |
| FR-RM-002 | P6 | `key_remap.c` |
| FR-RM-003 | P6 | `key_remap.c` |
| FR-RM-004 | P6 | `keyboard.c` |
| FR-RM-005 | P6 | `key_remap.c` |
| FR-RM-006 | P6 | `key_remap.c`, `tasks.c` |
| FR-RM-007 | P6 | `key_remap.c`, flash 저장 로직 |
| FR-RM-008 | P6+ | `webconfig/` |
| FR-RM-009 | — | 데이터 구조 설계 시 반영 |
| FR-BC-001 | 전체 | 회귀 테스트 |
| FR-BC-002 | P5 | `passthrough.c` |

### 7.2 구현 Phase → 검증 항목 매핑

| Phase | 내용 | 검증 항목 |
|-------|------|-----------|
| P1 | Descriptor 캡처/덤프 | FR-PT-001 |
| P2 | 고정 descriptor passthrough | FR-PT-002, FR-PT-004, FR-PT-006, FR-PT-007 |
| P3 | HID++ 상시 양방향 통신 | FR-PT-005, FR-AO-001, FR-AO-002, FR-AO-004 |
| P4 | 동적 descriptor 생성 | FR-PT-002, FR-PT-003 |
| P5 | 모드 분기 + 전환 | FR-PT-008, FR-AO-003, FR-SW-001~003, FR-BC-002 |
| P6 | 키 리매핑 엔진 | FR-RM-001~009 |
| P7 | 통합 테스트 + 안정화 | NFR-S-001~004, FR-BC-001 |

---

## 8. 검증 계획

### 8.1 Phase별 수용 테스트

#### P1: Descriptor 캡처

| TC-ID | 테스트 | 판정 기준 |
|-------|--------|-----------|
| TC-P1-01 | Bolt 수신기 연결 후 CDC 로그 확인 | 3개 interface descriptor hex dump 출력 |
| TC-P1-02 | Interface 0 descriptor 크기 | 67 bytes |
| TC-P1-03 | Interface 1 descriptor 크기 | 133 bytes |
| TC-P1-04 | Interface 2 descriptor에 Report ID 0x10, 0x11 포함 | hex dump에서 확인 |

#### P2: Passthrough (고정)

| TC-ID | 테스트 | 판정 기준 |
|-------|--------|-----------|
| TC-P2-01 | Win11 Device Manager 장치 인식 | HID 장치 3개 표시 |
| TC-P2-02 | 마우스 이동/클릭 | 정상 동작 |
| TC-P2-03 | 마우스 8버튼 | 뒤로/앞으로 포함 전체 동작 |
| TC-P2-04 | 수직/수평 스크롤 | 정상 동작 |
| TC-P2-05 | 키보드 전체 키 | 모든 키 입력 전달 확인 |
| TC-P2-06 | Consumer Control (미디어 키) | 볼륨, 재생/일시정지 등 |
| TC-P2-07 | 트리플 모니터 마우스 | 3개 모니터 전체에서 커서 이동 |

#### P3: HID++ Always-On

| TC-ID | 테스트 | 판정 기준 |
|-------|--------|-----------|
| TC-P3-01 | Win11 active: Options+ 수신기 인식 | "Connected" 표시 |
| TC-P3-02 | Win11 active: DPI 변경 | 설정 적용 확인 |
| TC-P3-03 | Win11 active: 제스처 버튼 | 동작 확인 |
| TC-P3-04 | **Android active**: Options+ 상태 | "Connected" 유지 |
| TC-P3-05 | **Android active**: DPI 변경 | 설정 적용 확인 |
| TC-P3-06 | **Android active**: 배터리 잔량 조회 | Options+에서 표시 |
| TC-P3-07 | HID++ short report (7B) 전달 | Wireshark/USBPcap으로 확인 |
| TC-P3-08 | HID++ long report (20B) 전달 | Wireshark/USBPcap으로 확인 |

#### P5: 전환

| TC-ID | 테스트 | 판정 기준 |
|-------|--------|-----------|
| TC-P5-01 | 핫키로 Win11→Android 전환 | Android에서 키보드/마우스 입력 동작 |
| TC-P5-02 | 핫키로 Android→Win11 전환 | Win11에서 키보드/마우스 입력 동작 |
| TC-P5-03 | 전환 후 Win11 USB 연결 유지 | Device Manager 장치 목록 불변 |
| TC-P5-04 | 전환 후 Options+ 세션 유지 | 재연결 없이 즉시 상태 표시 |
| TC-P5-05 | 10회 연속 전환 | 매 전환마다 정상 동작 |

#### P6: 키 리매핑

| TC-ID | 테스트 | 판정 기준 |
|-------|--------|-----------|
| TC-P6-01 | SIMPLE: CapsLock→Ctrl | CapsLock 입력 시 Ctrl 발생 |
| TC-P6-02 | SIMPLE: modifier 키 교체 | 정상 동작 |
| TC-P6-03 | TAP_HOLD: tap (< 200ms) | tap_action 발생 |
| TC-P6-04 | TAP_HOLD: hold (≥ 200ms) | hold_action 발생 |
| TC-P6-05 | TAP_HOLD: 빠른 연속 타이핑 | 오동작 없음 |
| TC-P6-06 | output_mask: A 전용 | B에서 미적용 확인 |
| TC-P6-07 | 빈 설정 | 모든 키 원래대로 동작 |
| TC-P6-08 | DeskHop 핫키 호환 | Left Ctrl + Caps Lock 등 기존 핫키 정상 |
| TC-P6-09 | Flash 저장/로드 | 전원 재투입 후 설정 유지 |

#### P7: 안정성

| TC-ID | 테스트 | 판정 기준 |
|-------|--------|-----------|
| TC-P7-01 | 24시간 연속 사용 | 행업/크래시 없음 |
| TC-P7-02 | 수신기 분리 → 재연결 | 자동 복귀, descriptor 재캡처 |
| TC-P7-03 | Watchdog 동작 | 강제 행업 유도 시 자동 재부팅 |
| TC-P7-04 | 태블릿 절전 → 복귀 | USB HID 자동 복구 |
| TC-P7-05 | Bolt 미연결 시 fallback | 일반 키보드/마우스로 기존 동작 |

---

## 9. 구현 로드맵

| Phase | 내용 | 관련 요구사항 | 예상 기간 |
|-------|------|-------------|-----------|
| **P1** | Bolt 수신기 descriptor 캡처/덤프 | FR-PT-001 | 1주 |
| **P2** | 고정 descriptor passthrough | FR-PT-002, 004, 006, 007 | 2주 |
| **P3** | HID++ 상시 양방향 통신 | FR-PT-005, FR-AO-001~004 | 1주 |
| **P4** | 동적 descriptor 생성 | FR-PT-002, 003 | 2주 |
| **P5** | 모드 분기 + 전환 | FR-PT-008, FR-AO-003, FR-SW-001~003, FR-BC-002 | 1주 |
| **P6** | 키 리매핑 엔진 (SIMPLE + TAP_HOLD) | FR-RM-001~009 | 2주 |
| **P7** | 통합 테스트 + 안정화 | NFR-S-001~004, FR-BC-001 | 2주 |

---

## 부록 A. 데이터 구조 요약

### A.1 Passthrough 상태

```c
#define MAX_PASSTHROUGH_IFACES  6
#define MAX_HID_DESC_SIZE       512

typedef struct {
    uint8_t  dev_addr;
    uint8_t  instance;              /* host 측 */
    uint8_t  device_instance;       /* device 측 (Win11 노출) */
    uint8_t  ep_in;
    uint8_t  itf_protocol;          /* KEYBOARD / MOUSE / NONE */
    uint16_t desc_len;
    uint8_t  desc[MAX_HID_DESC_SIZE];
    uint16_t max_report_size;
    uint8_t  report_id;
    bool     always_passthrough;    /* HID++ vendor: true */
} passthrough_iface_t;

typedef struct {
    uint8_t              iface_count;
    passthrough_iface_t  ifaces[MAX_PASSTHROUGH_IFACES];
    bool                 enumeration_done;
    bool                 passthrough_active;
} passthrough_state_t;
```

### A.2 키 리매핑 설정

```c
#define MAX_REMAP_ENTRIES    16

typedef enum {
    REMAP_SIMPLE,
    REMAP_TAP_HOLD,
    REMAP_TAP_DANCE,       /* 향후 */
    REMAP_COMBO,           /* 향후 */
    REMAP_MODIFIER_MORPH,  /* 향후 */
    REMAP_MACRO,           /* 향후 */
} remap_type_t;

typedef struct {
    uint8_t  keycode;
    uint8_t  modifier;
} key_action_t;

typedef struct {
    uint8_t      trigger;
    remap_type_t type;
    uint8_t      output_mask;   /* bit0=A, bit1=B, 0xFF=all */
    union { /* SIMPLE, TAP_HOLD, ... */ };
} remap_entry_t;
```

### A.3 신규/수정 파일 목록

| 파일 | 유형 | 역할 |
|------|------|------|
| `src/passthrough.c` | 신규 | Passthrough 상태 관리, descriptor 캡처/동적 생성, 모드 분기 |
| `src/key_remap.c` | 신규 | 키 리매핑 엔진 |
| `src/include/passthrough.h` | 신규 | Passthrough 구조체, 함수 선언 |
| `src/include/key_remap.h` | 신규 | 리매핑 타입, 구조체 정의 |
| `src/include/tusb_config.h` | 수정 | `CFG_TUD_HID` 2 → 6 |
| `src/usb_descriptors.c` | 수정 | 동적 descriptor 반환 로직 |
| `src/usb.c` | 수정 | Descriptor 캡처, passthrough 분기, output report 전달 |
| `src/keyboard.c` | 수정 | `remap_engine_process()` 삽입 |
| `src/tasks.c` | 수정 | `remap_engine_tick()` 주기적 호출 |
| `src/handlers.c` | 수정 | Output 전환 시 passthrough 플래그 제어 |
| `CMakeLists.txt` | 수정 | 신규 소스 파일 추가 |

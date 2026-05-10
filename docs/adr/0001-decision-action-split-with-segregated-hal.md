# Decision-action split with segregated HAL traits

The Rust port (`src-rust/`) deliberately enforces a three-layer boundary: `domain/` is pure-logic-only and forbids `use crate::hal` (rule stated in `domain/mod.rs`); `hal/` exposes ~13 narrow traits (`Timer`, `ReportQueue`, `PacketQueue`, `PeerLink`, `ConfigStore`, `UsbDevice`, `Indicator`, `DmaRx`, `Trace`, …) instead of one monolithic `Hal`; `service/` consumes domain decisions and a precise subset of HAL traits to produce side effects (`fn f(state, hal: &impl ReportQueue + Timer)`).

Every operation that involves both a decision and a side effect is split across two files: domain returns an enum/struct describing the action (e.g. `dispatch::DispatchAction`, `hid_routing::calculate_device_idx`, `keyboard::HotkeyAction`); the matching service function applies it via the minimal HAL traits it needs (e.g. `packet_dispatch`, `router::ReportRouter`, `hotkey_dispatch`).

Chosen over the obvious alternatives — a single `Hal` trait, or letting domain call HAL directly — because (a) the firmware must be testable on the host without a Pico, (b) `MockHal` becomes a single struct that implements all 13 traits, so each service test names exactly the surface it touches, and (c) the C codebase already passed `device_t*` everywhere; segregated traits give the Rust side a clean break from that pattern instead of mirroring it.

The cost is real: 22 files in `domain/`, paired naming (`domain/dispatch.rs` ↔ `service/packet_dispatch.rs`, `domain/hid_routing.rs` ↔ `service/router.rs`), and an extra hop for any new feature. Accepted as the price of host-testable firmware.

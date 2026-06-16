# 4. Rely on the host's Options+ for HID++ control diversion

## Status

Accepted (2026-06-17)

## Context

In passthrough mode DeskHop forwards an attached Logitech mouse's (MX Master)
HID++ events to the active host. The reprogrammable controls — standard buttons
plus the special SmartShift (CID `0xC4`) and gesture/thumb (CID `0xC3`) keys —
are only actionable when "diverted": reported through the ReprogControls feature
(HID++ `0x1B04`) as a `divertedButtonsEvent` (function 0, a CID bitmap) or
`analyticsKeyEvent` (function 2), instead of as standard HID input. Diversion is
configured by the host's Logitech Options+ via `setCidReporting` (a HID++
SET_REPORT), which DeskHop passes through to the device.

Two firmware features consume these events: the SmartShift double-click → output
switch, and the Android gesture-button remap (tap → Alt+Tab, hold → app drawer).
The gesture remap must identify ReprogControls' feature index
(`fi_reprog_controls`) to tell its fn=0 events apart from the HiRes-scroll and
thumbwheel fn=0 streams. (SmartShift needs no index — it scans the bitmap for
`0xC4` directly.)

DeskHop has two ways to obtain what it needs. Passively: learn the feature index
from observed events. Actively: query the device's feature table on mount
(IRoot / IFeatureSet) and/or send `setCidReporting` to divert the controls
itself — i.e. do Options+'s job. The active path entails control transfers to the
device on mount, which on the RP2040 must run on Core1 and must not race the host
stack, plus divert-lifecycle state.

The trigger for recording this: the gesture remap silently no-opped even with
Options+ active, because the index was learned only from fn=2 events while real
mice emit the diverted buttons as fn=0. The debug `scan` command dumps the
feature table but does not record indices into runtime state.

## Decision

We will rely on the active host's Logitech Options+ to divert the MX controls.
DeskHop will not originate `setCidReporting`, and will not run an active
feature-discovery handshake on device mount.

We will discover `fi_reprog_controls` passively, by autolearning it from observed
input events — both the fn=2 `analyticsKeyEvent` and the fn=0
`divertedButtonsEvent` carrying a known ReprogControls CID (a standard button, or
`0xC3` / `0xC4`) — while excluding the already-known HiRes-scroll and thumbwheel
indices so their fn=0 deltas cannot alias a CID. The learn step runs before the
gesture remap, so the first diverted press both learns the index and remaps.

The debug `scan` command stays a manual diagnostic that only dumps the feature
table; it does not populate runtime state.

## Consequences

Positive: the firmware stays a passive forwarder — no control-transfer traffic on
mount, no Core0/Core1 race to manage, and no risk of DeskHop's divert commands
fighting the host's Options+. Passive learning self-heals across board reboots (it
re-learns from the first diverted event), and the same mechanism makes
fn=0-diverted standard buttons (Back/Forward) convert correctly on the inactive
output.

Negative: the gesture remap and the per-output HID++ button conversion work only
when an Options+-equipped host has already diverted the controls. A setup with no
Options+ anywhere — notably a pure-Android target, or a host without Options+
installed — gets no diversion, so the thumb-button remap does nothing and DeskHop
cannot make it work on its own. `fi_reprog_controls` is volatile (resets on each
board reboot) and is re-learned only after the first diverted event seen that
boot.

Deferred alternative (not chosen): make DeskHop an active HID++ configurator —
query IRoot / IFeatureSet on mount to learn feature indices deterministically, and
send `setCidReporting` to divert `0xC3` / `0xC4` itself — which would enable
standalone operation with no host Options+ (the Android use case). This is
deferred for its added complexity: Core1 control transfers on mount, feature-table
and divert-lifecycle state, and coordination with a host that may also be
diverting. If standalone Android support becomes a requirement, supersede this ADR
and build on the existing `scan` state machine, extended to record the indices it
already queries.

Implemented by the passive fn=0 autolearn change on
`fix/gesture-button-autolearn-fn0`; the independent SmartShift double-click
release-gate is on `fix/smartshift-double-click-release-gate`.

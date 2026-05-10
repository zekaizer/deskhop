# Core split: consumers on core0, producers on core1

The RP2040 has two cores. Tasks are assigned by data direction: **core0** runs the consumer/sink side (USB Device send to PC, UART TX, all `process_*_queue` drainers, watchdog kick); **core1** runs the producer/source side (USB Host poll, UART RX/`packet_receiver`, screensaver, firmware-upgrade, heartbeat). The split was set in the original C codebase and the Rust port preserved it deliberately rather than redesigning it.

The reason is queue contention: every cross-core inter-core queue (`kbd_queue`, `mouse_queue`, `hid_queue_out`, `uart_tx_queue`) is filled by core1 and drained by core0, so each queue has exactly one producer core and exactly one consumer core. No SPSC/MPSC trickery needed and no two tasks race on the same end of any queue. New tasks are placed on whichever core matches their direction; if a task touches both directions, it indicates the design has drifted and either the task or the queue layout needs revisiting.

The one shared field that crosses the boundary outside a queue is `core1_last_loop_pass` (core1 writes, core0 reads for hang-detection). Torn reads on this u64 are accepted as benign; see the SAFETY note in `src-rust/src/lib.rs:83-86`.

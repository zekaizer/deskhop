// Service layer — orchestrates domain logic with HAL traits.
// All public functions are generic over HAL trait bounds.

pub mod backend;
pub mod config_api;
pub mod frontend;
pub mod fw_upgrade;
pub mod hotkey_dispatch;
pub mod led;
pub mod msg_bridge;
pub mod output;
pub mod passthrough_service;
pub mod packet_dispatch;
pub mod peer_log;
pub mod router;
pub mod tasks;
pub mod usb;

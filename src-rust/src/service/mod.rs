// Service layer — orchestrates domain logic with HAL traits.
// All public functions are generic over HAL trait bounds.

pub mod backend;
pub mod config_api;
pub mod frontend;
pub mod fw_upgrade;
pub mod msg_bridge;
pub mod router;
pub mod tasks;

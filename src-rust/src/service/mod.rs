// Service layer — orchestrates domain logic with HAL traits.
// All public functions are generic over HAL trait bounds.

pub mod backend;
pub mod frontend;
pub mod msg_bridge;
pub mod router;
pub mod tasks;

//! The game model (PLAN.md §5, Phase 3): every item location, enemy, boss and
//! grace, extracted from the player's own game files.

pub mod elden_ring;
pub mod model;
pub mod sources;

pub use model::Model;
pub use sources::{MapId, Sources};

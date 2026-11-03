//! Malenia (docs/spec/enemies.md §3.2). She is one body, and an event in her
//! arena's own script sets off her second stage, so it goes where she goes: see
//! `phases`, which moves her event and Godrick's the same way.
//!
//! Her health is held at 1 until that event runs (`4[14]` with test 5, at most,
//! against 1), so without it a fight with her could not end. That is why she keeps
//! to arenas, and why the other bodies held at 1 stay home.

use fs_emevd::{Instruction, catalogue};

use crate::Spot;

pub const HER_ENTITY: u32 = 15_000_800;

/// Whether a spot is the one she stands on in the unmodded game. Other rows of
/// her character exist, but only this one is the fight.
pub fn is_home(spot: &Spot) -> bool {
    spot.entity == Some(HER_ENTITY)
}

/// An instruction that does nothing: wait for no frames.
pub fn nothing() -> Instruction {
    let wait = catalogue::named("wait_frames");
    Instruction {
        bank: wait.bank,
        id: wait.id,
        args: 0i32.to_le_bytes().to_vec(),
        layer: None,
    }
}

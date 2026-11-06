//! The fog gate randomizer (Nova, 2026-11-06; `docs/fog-gates.md`): boss fogs and
//! dungeon mouths lead somewhere else, both ways, from a seed. Separate from the item
//! randomizer, it can run with it: the item proof then walks the doors as shuffled.

pub mod doors;

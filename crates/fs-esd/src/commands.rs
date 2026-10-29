//! What the game's own talk commands do, as far as its scripts show it. Each
//! was found by following every call ([`crate::calls::resolve`]) in the 358
//! talk scripts found by map name and seeing which tables the plain numbers
//! that reach an argument are rows of (build 25080141; shares of distinct
//! values). `fs-tool/tests/real_talk.rs` re-checks them.

/// A command's bank and id.
pub type Id = (i32, i32);

/// Gives the player an item lot: argument 0 is a row of ItemLotParam_map in
/// 120 of its 126 distinct values (201 uses). Among them are lots no event
/// script or map gives, such as the Spectral Steed Whistle's (100000).
pub const AWARD_ITEM_LOT: Id = (1, 104);

/// Opens a shop on a range of ShopLineupParam rows: argument 0 the first row,
/// argument 1 the last (100000 to 100024, 100050 to 100074, …). 37 of the 47
/// distinct first rows are rows of the table; the ranges cover 550 of its
/// 1,296 rows.
pub const OPEN_SHOP: Id = (1, 22);

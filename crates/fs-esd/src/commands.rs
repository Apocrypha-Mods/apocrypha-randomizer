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

/// Turns an event flag on or off: argument 0 the flag, argument 1 the state
/// (1 on, 0 off). Sellen's script sets 1044369244 here, the flag that puts
/// Glintstone Cometshard on sale (ShopLineupParam 100057).
pub const SET_EVENT_FLAG: Id = (1, 11);

/// Takes an item from the player: argument 0 its type (3 for goods, as in
/// ShopLineupParam), argument 1 its id, argument 2 how many (-1 for all).
/// Sellen's script takes the Conspectus Scroll (8850) here, just before
/// setting the flag that puts Glintstone Cometshard on sale.
pub const TAKE_ITEM: Id = (1, 52);

/// Adds an entry to the talk menu: argument 0 the number the choice reads back
/// as (function 23), argument 1 its text, then -1. Miriel's script compares
/// function 23 with the same numbers it adds here. `5[0x13]` adds it when its
/// extra first argument holds.
pub const MENU_ENTRY: Id = (1, 0x13);

/// Adds an entry to the talk menu as [`MENU_ENTRY`] does, with two more
/// arguments: the Twin Maiden Husks' lists of bell bearings and of the stock
/// they open (`1[0x95](99, 26000004, -1, 9999999, 0)`, "Leave", last).
pub const MENU_ENTRY_COUNTED: Id = (1, 0x95);

/// Empties the talk menu: in each of the 68 menus that lead to a shop
/// ([`crate::menu::shop_menus`], 2.7.2), one state runs it, and the machine
/// comes back to that state after the shop closes, before the menu's entries
/// are added again.
pub const CLEAR_MENU: Id = (1, 0x14);

/// Shows the talk menu built so far: in the same menus one state runs it, and
/// a state after it tests the choice (function 23).
pub const SHOW_MENU: Id = (1, 0x0A);
/// Shows the menu as [`SHOW_MENU`] does, for menus whose entries are added by
/// [`MENU_ENTRY_COUNTED`]: the Twin Maiden Husks' second bell-bearing menu
/// (`t600001110` machine 0x7FFFFFF5, Kalé's and eight more), `1[0x96](1, 1)`.
pub const SHOW_MENU_COUNTED: Id = (1, 0x96);

/// Sets loop counter `k` (argument 0) to argument 1; function 100 reads it back.
/// Menus are filled in loops that start a counter at 0 and add 1 until it passes
/// a bound.
pub const SET_COUNTER: Id = (1, 0x64);

/// Opens a shop on a range of ShopLineupParam rows: argument 0 the first row,
/// argument 1 the last (100000 to 100024, 100050 to 100074, …). 37 of the 47
/// distinct first rows are rows of the table; the ranges cover 550 of its
/// 1,296 rows.
pub const OPEN_SHOP: Id = (1, 22);

/// Opens a dragon communion altar's exchange on a range of ShopLineupParam
/// rows (first, last): the Church of Dragon Communion's two scripts and the
/// Grand Altar's, whose rows are incantations (Dragonclaw, Bayle's Tyranny).
pub const OPEN_DRAGON_COMMUNION: Id = (1, 135);

/// Opens Seluvis's puppets on a range of ShopLineupParam rows (first, last):
/// one script, two uses.
pub const OPEN_PUPPET_SHOP: Id = (1, 144);

/// Opens an exchange on a range of ShopLineupParam rows: argument 0 a kind
/// (1 for the remembrance lists of Finger Reader Enia and the Walking
/// Mausoleum, 0 for Seluvis's puppets), then the first and last rows.
pub const OPEN_EXCHANGE: Id = (1, 146);

/// Every command that makes a range of ShopLineupParam rows available, with
/// where its first and last rows are in the arguments.
pub const SHOP_RANGES: [(Id, usize, usize); 4] = [
    (OPEN_SHOP, 0, 1),
    (OPEN_DRAGON_COMMUNION, 0, 1),
    (OPEN_PUPPET_SHOP, 0, 1),
    (OPEN_EXCHANGE, 1, 2),
];

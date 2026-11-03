//! Talk menus that lead to a shop, read and added to.
//!
//! The menus that lead to the named merchants' shops are built one way (68
//! menus on 2.7.2; `fs-tool/tests/real_talk.rs` re-checks it): one state
//! empties the menu ([`CLEAR_MENU`]), entries are added ([`MENU_ENTRY`], or with
//! bank 5 when their first argument holds), one state shows the menu
//! ([`SHOW_MENU`]), and a later one tests the choice (function 23) and moves to
//! a state for each entry. When the shop closes the machine goes back to the
//! state that empties the menu. Some open the shop themselves; some call a
//! machine that does, passing it the range (Moore's `t415006100`).
//!
//! [`add_gift`] adds an entry that gives an item lot once: a state that awards
//! the lot and turns a flag on, then waits for the item's notice to close as the
//! game's own gifts do (`1[0x68](60400)`, `1[0xb](…, 1)`, then `f25(63) == 0`),
//! and goes back to empty the menu, so the entry is gone the next time it is
//! built.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use fs_bytes::{Result, format_err};

use crate::calls::CALL_BANK;
use crate::choice::CHOICE;
use crate::commands::{
    AWARD_ITEM_LOT, CLEAR_MENU, MENU_ENTRY, MENU_ENTRY_COUNTED, OPEN_SHOP, SET_EVENT_FLAG,
    SHOW_MENU, SHOW_MENU_COUNTED,
};
use crate::cond::{Atom, FLAG, requires};
use crate::expr::{self, END, Place, all, call_is, number};
use crate::{Command, Esd, Group};

/// Function 25 with 63: whether the notice of an item received is still open.
/// The game's gifts wait for `f25(63) == 0` after awarding a lot.
const NOTICE: (i32, i32) = (25, 63);
/// Bank 5 runs the bank 1 command of the same id when its first argument holds.
const WHEN_BANK: i32 = 5;
/// The choice the game's menus give "Leave".
const LEAVE: i32 = 99;
/// How deep calls are followed to find a shop.
const MAX_DEPTH: usize = 8;
/// The first choice number tried for a new entry.
const FIRST_FREE: i32 = 70;

/// A menu that leads to a shop: its machine, and the states (indices in
/// [`Esd::states`]) that empty it, show it and read the choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopMenu {
    /// The machine's index in [`Esd::groups`].
    pub group: usize,
    pub clear: usize,
    pub show: usize,
    pub choose: usize,
    /// The ShopLineupParam ranges it opens, itself or through machines it
    /// calls that have no menu of their own.
    pub ranges: BTreeSet<(i32, i32)>,
}

/// The states of a machine running a command on entry.
fn running(esd: &Esd, g: &Group, command: (i32, i32)) -> Vec<usize> {
    esd.states_of(g)
        .iter()
        .filter(|s| {
            esd.commands_in(s.on_enter)
                .iter()
                .any(|c| (c.bank, c.id) == command)
        })
        .filter_map(|s| esd.index_of(s))
        .collect()
}

/// Whether an expression starts by reading the choice: `f23()`.
fn reads_choice(e: &[u8]) -> bool {
    e.starts_with(&[expr::int(CHOICE)[0], 0x84])
}

/// The states that empty, show and read a machine's menu, when each is one.
fn menu_states(esd: &Esd, g: &Group) -> Option<(usize, usize, usize)> {
    let one = |v: Vec<usize>| (v.len() == 1).then(|| v[0]);
    let clear = one(running(esd, g, CLEAR_MENU))?;
    let mut shows = running(esd, g, SHOW_MENU);
    shows.extend(running(esd, g, SHOW_MENU_COUNTED));
    let show = one(shows)?;
    let choose = one(esd
        .states_of(g)
        .iter()
        .filter(|s| {
            esd.conditions_in(s.conditions)
                .iter()
                .any(|c| esd.expression(c.evaluator).is_some_and(reads_choice))
        })
        .filter_map(|s| esd.index_of(s))
        .collect())?;
    Some((clear, show, choose))
}

/// Every command a state runs: on entry, exit and update, and as conditions pass.
fn commands_of<'a>(esd: &'a Esd, s: &'a crate::State) -> Vec<&'a Command> {
    let mut out: Vec<&Command> = [s.on_enter, s.on_exit, s.on_update]
        .into_iter()
        .flat_map(|sp| esd.commands_in(sp))
        .collect();
    let mut todo = esd.conditions_in(s.conditions);
    while let Some(c) = todo.pop() {
        out.extend(esd.commands_in(c.on_pass));
        todo.extend(esd.conditions_in(c.subconditions));
    }
    out
}

/// An argument's number, from the expression or the call's own arguments.
fn value(e: Option<&[u8]>, passed: &[Option<i32>]) -> Option<i32> {
    let e = e?;
    expr::constant(e).or_else(|| passed.get(expr::argument(e)?).copied().flatten())
}

/// The shop ranges machine `g` opens, called with `passed`, following calls into
/// machines that have no menu of their own.
fn ranges_from(
    esd: &Esd,
    g: &Group,
    passed: &[Option<i32>],
    has_menu: &HashSet<i64>,
    seen: &mut HashSet<(i64, Vec<Option<i32>>)>,
    depth: usize,
    out: &mut BTreeSet<(i32, i32)>,
) {
    if depth > MAX_DEPTH || !seen.insert((g.id, passed.to_vec())) {
        return;
    }
    for s in esd.states_of(g) {
        for c in commands_of(esd, s) {
            let args = esd.command_args(c);
            if (c.bank, c.id) == OPEN_SHOP {
                if let (Some(a), Some(b)) = (
                    value(args.first().copied().flatten(), passed),
                    value(args.get(1).copied().flatten(), passed),
                ) {
                    out.insert((a, b));
                }
            } else if c.bank == CALL_BANK && !has_menu.contains(&i64::from(c.id)) {
                let Some(target) = esd.groups.iter().find(|t| t.id == i64::from(c.id)) else {
                    continue;
                };
                let next: Vec<Option<i32>> = args.iter().map(|a| value(*a, passed)).collect();
                ranges_from(esd, target, &next, has_menu, seen, depth + 1, out);
            }
        }
    }
}

/// Every menu of a script that leads to a shop.
pub fn shop_menus(esd: &Esd) -> Vec<ShopMenu> {
    let trios: Vec<Option<(usize, usize, usize)>> =
        esd.groups.iter().map(|g| menu_states(esd, g)).collect();
    let has_menu: HashSet<i64> = esd
        .groups
        .iter()
        .zip(&trios)
        .filter(|(_, t)| t.is_some())
        .map(|(g, _)| g.id)
        .collect();
    let mut out = Vec::new();
    for (gi, (g, trio)) in esd.groups.iter().zip(&trios).enumerate() {
        let Some((clear, show, choose)) = *trio else {
            continue;
        };
        let mut ranges = BTreeSet::new();
        ranges_from(esd, g, &[], &has_menu, &mut HashSet::new(), 0, &mut ranges);
        if !ranges.is_empty() {
            out.push(ShopMenu {
                group: gi,
                clear,
                show,
                choose,
                ranges,
            });
        }
    }
    out
}

/// The menu entries of a machine: (the command's place among its state's
/// commands on entry, the state, its choice number, its first argument when
/// bank 5 tests one).
fn entries<'a>(esd: &'a Esd, g: &Group) -> Vec<(usize, usize, i32, Option<&'a [u8]>)> {
    let mut out = Vec::new();
    for s in esd.states_of(g) {
        let Some(si) = esd.index_of(s) else { continue };
        for (pos, c) in esd.commands_in(s.on_enter).iter().enumerate() {
            let a = esd.command_args(c);
            let is_entry = |id: i32| id == MENU_ENTRY.1 || id == MENU_ENTRY_COUNTED.1;
            let (choice, test) = match c.bank {
                1 if is_entry(c.id) => (a.first().copied().flatten(), None),
                WHEN_BANK if is_entry(c.id) => {
                    (a.get(1).copied().flatten(), a.first().copied().flatten())
                }
                _ => continue,
            };
            if let Some(v) = choice.and_then(expr::constant) {
                out.push((pos, si, v, test));
            }
        }
    }
    out
}

/// The choice numbers some menu entry of the script gives.
fn choices_used(esd: &Esd) -> BTreeSet<i32> {
    esd.groups
        .iter()
        .flat_map(|g| entries(esd, g))
        .map(|(_, _, v, _)| v)
        .collect()
}

/// Whether a choice test compares the choice with `v`: `(r0 = f23()) == v`,
/// `r0 == v` or `f23() == v`.
fn tests_choice(e: &[u8], v: i32) -> bool {
    let mut tail = expr::int(v);
    tail.extend([0x95, END]);
    let f23 = [expr::int(CHOICE)[0], 0x84];
    let heads: [&[u8]; 3] = [&[f23[0], f23[1], 0xA7], &[0xAF], &[f23[0], f23[1], 0xA6]];
    heads
        .iter()
        .any(|h| e.len() == h.len() + tail.len() && e.starts_with(h) && e.ends_with(&tail))
}

/// For each menu entry offered only while flags are on, the shop ranges its
/// choice opens: the flags, and each range. Read from the choice test for the
/// entry's number along every way on, short of the menu again, as the Twin
/// Maiden Husks' menus go (entry 1 while 11109713 is on, then 100050 to 100074;
/// Moore's entry passes two flag tests on the way to 102250 to 102289).
pub fn shops_behind_flags(esd: &Esd) -> Vec<(BTreeSet<u32>, (i32, i32))> {
    let mut out = Vec::new();
    for m in shop_menus(esd) {
        let g = &esd.groups[m.group];
        let choose = &esd.states[m.choose];
        for (_, _, v, test) in entries(esd, g) {
            let Some(test) = test else { continue };
            let atoms = requires(test);
            if atoms.is_empty() || atoms.iter().any(|a| !matches!(a, Atom::Flag(_, true))) {
                continue;
            }
            let flags: BTreeSet<u32> = atoms
                .iter()
                .filter_map(|a| match a {
                    Atom::Flag(f, true) => Some(*f),
                    _ => None,
                })
                .collect();
            for c in esd.conditions_in(choose.conditions) {
                if !esd
                    .expression(c.evaluator)
                    .is_some_and(|e| tests_choice(e, v))
                {
                    continue;
                }
                // Every way on from the choice, short of the menu again.
                let menu = [m.clear, m.show, m.choose];
                let mut seen = HashSet::new();
                let mut todo: Vec<(usize, usize)> = esd
                    .state_at(c.next_state)
                    .map(|(i, _)| (i, 0))
                    .into_iter()
                    .collect();
                while let Some((i, depth)) = todo.pop() {
                    if menu.contains(&i) || depth > MAX_DEPTH || !seen.insert(i) {
                        continue;
                    }
                    let s = &esd.states[i];
                    for cmd in esd.commands_in(s.on_enter) {
                        let a = esd.command_args(cmd);
                        if (cmd.bank, cmd.id) == OPEN_SHOP
                            && let (Some(x), Some(y)) = (
                                a.first().copied().flatten().and_then(expr::constant),
                                a.get(1).copied().flatten().and_then(expr::constant),
                            )
                            && !out.contains(&(flags.clone(), (x, y)))
                        {
                            out.push((flags.clone(), (x, y)));
                        }
                    }
                    for next in esd.conditions_in(s.conditions) {
                        if let Some((j, _)) = esd.state_at(next.next_state) {
                            todo.push((j, depth + 1));
                        }
                    }
                }
            }
        }
    }
    out
}

/// The flags each shop range is opened behind, from [`shops_behind_flags`].
pub fn ranges_by_flag(esd: &Esd) -> BTreeMap<u32, BTreeSet<(i32, i32)>> {
    let mut out: BTreeMap<u32, BTreeSet<(i32, i32)>> = BTreeMap::new();
    for (flags, range) in shops_behind_flags(esd) {
        if let Some(first) = flags.first() {
            out.entry(*first).or_default().insert(range);
        }
    }
    out
}

/// What a new entry gives and says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gift {
    /// Its text, an EventTextForTalk id.
    pub text: i32,
    /// The ItemLotParam_map row awarded.
    pub lot: i32,
    /// Turned on when it is given; the entry is offered while it is off.
    pub flag: u32,
    /// Flags that hide the entry too while any of them is on.
    pub hidden_by: Vec<u32>,
}

/// Add `gift` to the menu of machine `group` (found again by [`shop_menus`],
/// as indices move), before its "Leave" when that is added in the machine, else
/// just before the menu is shown. Returns the entry's choice number.
pub fn add_gift(esd: &mut Esd, group: usize, gift: &Gift) -> Result<i32> {
    let m = shop_menus(esd)
        .into_iter()
        .find(|m| m.group == group)
        .ok_or_else(|| format_err(format!("machine {group} has no shop menu")))?;
    let used = choices_used(esd);
    let choice = (FIRST_FREE..LEAVE)
        .find(|v| !used.contains(v))
        .ok_or_else(|| format_err("no free choice number"))?;
    let g = &esd.groups[group];
    let id = esd.states_of(g).iter().map(|s| s.id).max().unwrap_or(0) + 1;
    let leave = entries(esd, g)
        .into_iter()
        .find(|(_, _, v, _)| *v == LEAVE)
        .map(|(pos, si, _, _)| (si, pos));
    let show_pos = esd
        .commands_in(esd.states[m.show].on_enter)
        .iter()
        .position(|c| [SHOW_MENU, SHOW_MENU_COUNTED].contains(&(c.bank, c.id)))
        .unwrap_or(0);
    let (entry_state, entry_pos) = leave.unwrap_or((m.show, show_pos));

    // The gift's state: award, turn the flag on, wait for the notice, go back.
    let new = esd.add_state(group, id)?;
    esd.insert_command(
        new,
        0,
        AWARD_ITEM_LOT.0,
        AWARD_ITEM_LOT.1,
        &[number(gift.lot)],
    )?;
    let flag = i32::try_from(gift.flag).map_err(|_| format_err("a flag past i32"))?;
    esd.insert_command(
        new,
        1,
        SET_EVENT_FLAG.0,
        SET_EVENT_FLAG.1,
        &[number(flag), number(1)],
    )?;
    esd.insert_condition(
        new,
        0,
        Some(m.clear),
        &all(&[call_is(Place::Condition, NOTICE.0, &[NOTICE.1], 0)]),
    )?;

    // The entry, while every flag is off.
    let mut off = vec![call_is(Place::Argument, FLAG, &[flag], 0)];
    for f in &gift.hidden_by {
        let f = i32::try_from(*f).map_err(|_| format_err("a flag past i32"))?;
        off.push(call_is(Place::Argument, FLAG, &[f], 0));
    }
    esd.insert_command(
        entry_state,
        entry_pos,
        WHEN_BANK,
        MENU_ENTRY.1,
        &[all(&off), number(choice), number(gift.text), number(-1)],
    )?;

    // Its choice, tested first.
    esd.insert_condition(
        m.choose,
        0,
        Some(new),
        &all(&[call_is(Place::Condition, CHOICE, &[], choice)]),
    )?;
    Ok(choice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Table;
    use crate::{NO_SPAN, Span, State};

    /// A merchant's menu as the game builds one, in one machine: 0 moves to 1;
    /// 1 empties the menu; 2 adds "Purchase" (1) and "Leave" (99); 3 shows it;
    /// 4 reads the choice, 1 to 5, else 6; 5 opens 100250 to 100274 and goes
    /// back to 1; 6 ends.
    fn merchant() -> Esd {
        let mut esd = Esd::read(&empty().write().unwrap()).unwrap();
        esd.groups.push(Group {
            id: 0x7FFF_FFFF,
            states: NO_SPAN,
            states_again: -1,
        });
        esd.insert(Table::States, 0, 7).unwrap();
        let first = esd.offset_of(Table::States, 0);
        esd.groups[0].states = Span {
            offset: first,
            count: 7,
        };
        esd.groups[0].states_again = first;
        for (i, s) in esd.states.iter_mut().enumerate() {
            s.id = i as i64;
        }
        let one = vec![0x41, END];
        esd.insert_condition(0, 0, Some(1), &one).unwrap();
        esd.insert_command(1, 0, 1, CLEAR_MENU.1, &[]).unwrap();
        esd.insert_condition(1, 0, Some(2), &one).unwrap();
        esd.insert_command(
            2,
            0,
            1,
            MENU_ENTRY.1,
            &[number(1), number(23_330_000), number(-1)],
        )
        .unwrap();
        esd.insert_command(
            2,
            1,
            1,
            MENU_ENTRY.1,
            &[number(99), number(20_000_009), number(-1)],
        )
        .unwrap();
        esd.insert_condition(2, 0, Some(3), &one).unwrap();
        esd.insert_command(3, 0, 1, SHOW_MENU.1, &[number(1)])
            .unwrap();
        esd.insert_condition(3, 0, Some(4), &one).unwrap();
        // (r0 = f23()) == 1 -> 5, else 6.
        let first_test = vec![0x57, 0x84, 0xA7, 0x41, 0x95, END];
        esd.insert_condition(4, 0, Some(5), &first_test).unwrap();
        esd.insert_condition(4, 1, Some(6), &one).unwrap();
        esd.insert_command(5, 0, 1, OPEN_SHOP.1, &[number(100_250), number(100_274)])
            .unwrap();
        esd.insert_condition(5, 0, Some(1), &one).unwrap();
        esd
    }

    fn empty() -> Esd {
        Esd {
            unk18: 6,
            hash: [0; 16],
            groups: vec![],
            states: vec![],
            conditions: vec![],
            commands: vec![],
            args: vec![],
            condition_list: vec![],
            expressions: vec![],
            name: "t333006000".encode_utf16().collect(),
            nameless: false,
        }
    }

    fn state(esd: &Esd, i: usize) -> &State {
        &esd.states[i]
    }

    #[test]
    fn a_merchants_menu_is_found_with_its_shop() {
        let esd = merchant();
        let menus = shop_menus(&esd);
        assert_eq!(menus.len(), 1);
        let m = &menus[0];
        assert_eq!((m.clear, m.show, m.choose), (1, 3, 4));
        assert_eq!(m.ranges, BTreeSet::from([(100_250, 100_274)]));
    }

    #[test]
    fn a_gift_entry_awards_its_lot_once_and_goes_back_to_the_menu() {
        let mut esd = merchant();
        let gift = Gift {
            text: 26_009_820,
            lot: 11_109_820,
            flag: 11_109_820,
            hidden_by: vec![11_109_719],
        };
        let choice = add_gift(&mut esd, 0, &gift).unwrap();
        assert_eq!(choice, FIRST_FREE);
        let esd = Esd::read(&esd.write().unwrap()).unwrap();
        // The new state, last of the machine.
        let new = 7;
        assert_eq!(state(&esd, new).id, 7);
        let cmds = esd.commands_in(state(&esd, new).on_enter);
        assert_eq!(
            cmds.iter().map(|c| (c.bank, c.id)).collect::<Vec<_>>(),
            vec![AWARD_ITEM_LOT, SET_EVENT_FLAG]
        );
        assert_eq!(
            esd.command_args(&cmds[0])[0].and_then(expr::constant),
            Some(11_109_820)
        );
        let back = esd.conditions_in(state(&esd, new).conditions);
        assert_eq!(back.len(), 1);
        assert_eq!(esd.state_at(back[0].next_state).map(|(i, _)| i), Some(1));
        assert_eq!(
            esd.expression(back[0].evaluator),
            Some(&[0x59, 0x7F, 0x85, 0xA6, 0x40, 0x95, END][..])
        );
        // The entry, before "Leave", hidden by either flag.
        let menu = esd.commands_in(state(&esd, 2).on_enter);
        assert_eq!(menu.len(), 3);
        assert_eq!((menu[1].bank, menu[1].id), (WHEN_BANK, MENU_ENTRY.1));
        let a = esd.command_args(&menu[1]);
        assert_eq!(
            requires(a[0].unwrap()),
            vec![Atom::Flag(11_109_820, false), Atom::Flag(11_109_719, false)]
        );
        assert_eq!(a[1].and_then(expr::constant), Some(choice));
        assert_eq!(a[2].and_then(expr::constant), Some(26_009_820));
        // Its choice, tested first, to the new state; the others as they were.
        let tests = esd.conditions_in(state(&esd, 4).conditions);
        assert_eq!(tests.len(), 3);
        assert!(tests_choice(
            esd.expression(tests[0].evaluator).unwrap(),
            choice
        ));
        assert_eq!(esd.state_at(tests[0].next_state).map(|(i, _)| i), Some(new));
        assert_eq!(esd.state_at(tests[1].next_state).map(|(i, _)| i), Some(5));
        assert_eq!(esd.state_at(tests[2].next_state).map(|(i, _)| i), Some(6));
        // Still one shop menu, with the same states.
        let m = &shop_menus(&esd)[0];
        assert_eq!((m.clear, m.show, m.choose), (1, 3, 4));
        // A second gift takes the next number.
        let mut esd = esd;
        assert_eq!(add_gift(&mut esd, 0, &gift).unwrap(), FIRST_FREE + 1);
    }

    #[test]
    fn a_shop_behind_a_flag_is_read_from_its_entry() {
        let mut esd = merchant();
        // Make "Purchase" a bank-5 entry offered while 11109719 is on.
        let menu = esd.states[2].on_enter;
        let first = esd.commands_in(menu)[0].clone();
        let at = esd.commands.iter().position(|c| *c == first).unwrap();
        esd.commands[at].bank = WHEN_BANK;
        let mut args: Vec<Vec<u8>> = vec![all(&[call_is(Place::Argument, FLAG, &[11_109_719], 1)])];
        args.extend(esd.command_args(&first).iter().map(|a| a.unwrap().to_vec()));
        // Rebuild the command with its test first.
        esd.commands[at].args = NO_SPAN;
        let a0 = esd.args.len();
        esd.insert(Table::Args, a0, args.len()).unwrap();
        esd.commands[at].args = Span {
            offset: esd.offset_of(Table::Args, a0),
            count: args.len() as i64,
        };
        for (i, e) in args.iter().enumerate() {
            esd.args[a0 + i] = esd.push_expression(e);
        }
        let esd = Esd::read(&esd.write().unwrap()).unwrap();
        assert_eq!(
            shops_behind_flags(&esd),
            vec![(BTreeSet::from([11_109_719]), (100_250, 100_274))]
        );
        assert_eq!(
            ranges_by_flag(&esd)[&11_109_719],
            BTreeSet::from([(100_250, 100_274)])
        );
    }
}

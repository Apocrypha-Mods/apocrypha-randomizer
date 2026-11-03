//! Items handed over by picking them from a talk menu, where the item and the
//! flag are worked out from the choice rather than written in the script.
//! [`crate::handover`] reads the plain cases; this reads the case Miriel's
//! script (`t330006000`) is built on.
//!
//! One machine fills a menu in a loop: entry `I + n`, offered while the player
//! holds goods `J + n` and flag `F + n` is off, for `n` from 0 until a counter
//! passes `N`. Another, once the player has chosen entry `s` ([`CHOICE`]), takes
//! goods `G + s` and turns on flag `H + s`. The numbers reach both machines as
//! call arguments, so every chain of calls from a machine nothing calls is
//! followed with the numbers it passes.
//!
//! A trade is read only where the two agree: the goods taken for entry `s` are
//! the goods the entry was offered for, and the flag turned on is the flag the
//! entry waited to be off. Miriel's scrolls (8850 to 8854, flags 1037469300 to
//! 1037469304), prayerbooks (8855 to 8865, from 1037469305) and Academy Scroll
//! (8866, 1037469316) pair this way.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::calls::CALL_BANK;
use crate::commands::{MENU_ENTRY, MENU_ENTRY_COUNTED, SET_COUNTER, SET_EVENT_FLAG, TAKE_ITEM};
use crate::cond::{FLAG, HOLDS};
use crate::expr::{ARG, END, INT};
use crate::handover::Handover;
use crate::{Esd, Group, State};

/// Function 23: the menu entry the player chose.
pub const CHOICE: i32 = 23;
/// Function 100: a loop counter's value, set by [`SET_COUNTER`].
pub const COUNTER: i32 = 100;
/// Bank 5 runs the bank 1 command of the same id when its first argument holds:
/// `5[0x13](cond, …)` adds the menu entry `1[0x13](…)` would.
const WHEN_BANK: i32 = 5;
/// Goods, in TAKE_ITEM's and function 47's numbering.
const GOODS: i64 = 3;
/// How far calls are followed; the game's scripts go 5 deep.
const MAX_DEPTH: usize = 16;
/// The longest menu loop read.
const MAX_ENTRIES: i64 = 256;

/// A number as far as the script shows it: `c + sel × choice + ctr × counter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Linear {
    c: i64,
    sel: i64,
    ctr: i64,
}

impl Linear {
    const fn of(c: i64) -> Self {
        Linear { c, sel: 0, ctr: 0 }
    }

    fn constant(self) -> Option<i64> {
        (self.sel == 0 && self.ctr == 0).then_some(self.c)
    }
}

/// An expression with what is known filled in.
#[derive(Debug, Clone)]
enum Node {
    Lin(Linear),
    Call(i32, Vec<Node>),
    Op(u8, Box<Node>, Box<Node>),
    Other,
}

const ADD: u8 = 0x8C;
const SUB: u8 = 0x8E;
const MUL: u8 = 0x8F;
const GT: u8 = 0x94;
const EQ: u8 = 0x95;
const AND: u8 = 0x98;

/// An expression read with the call's numbers in `args`. What is not a number
/// the scripts compute this way reads as [`Node::Other`].
fn parse(e: &[u8], args: &[Option<i32>]) -> Option<Node> {
    let mut stack: Vec<Node> = Vec::new();
    let mut i = 0;
    while i < e.len() {
        let b = e[i];
        i += 1;
        match b {
            END => break,
            INT => {
                let v = i32::from_le_bytes(e.get(i..i + 4)?.try_into().ok()?);
                stack.push(Node::Lin(Linear::of(i64::from(v))));
                i += 4;
            }
            0x80 => {
                i += 4;
                stack.push(Node::Other);
            }
            0x81 => {
                i += 8;
                stack.push(Node::Other);
            }
            _ if b < 0x80 => stack.push(Node::Lin(Linear::of(i64::from(b) - 64))),
            0x84..=0x8A => {
                let n = usize::from(b - 0x84);
                let a = stack.split_off(stack.len().checked_sub(n)?);
                let Node::Lin(f) = stack.pop()? else {
                    return None;
                };
                let f = i32::try_from(f.constant()?).ok()?;
                stack.push(match (f, &a[..]) {
                    (CHOICE, []) => Node::Lin(Linear {
                        c: 0,
                        sel: 1,
                        ctr: 0,
                    }),
                    (COUNTER, [_]) => Node::Lin(Linear {
                        c: 0,
                        sel: 0,
                        ctr: 1,
                    }),
                    _ => Node::Call(f, a),
                });
            }
            ADD | SUB | MUL | 0x91..=0x96 | AND | 0x99 => {
                let r = stack.pop()?;
                let l = stack.pop()?;
                stack.push(match (b, &l, &r) {
                    (ADD, Node::Lin(x), Node::Lin(y)) => Node::Lin(Linear {
                        c: x.c + y.c,
                        sel: x.sel + y.sel,
                        ctr: x.ctr + y.ctr,
                    }),
                    (SUB, Node::Lin(x), Node::Lin(y)) => Node::Lin(Linear {
                        c: x.c - y.c,
                        sel: x.sel - y.sel,
                        ctr: x.ctr - y.ctr,
                    }),
                    (MUL, Node::Lin(x), Node::Lin(y)) => match (x.constant(), y.constant()) {
                        (Some(k), _) => Node::Lin(Linear {
                            c: k * y.c,
                            sel: k * y.sel,
                            ctr: k * y.ctr,
                        }),
                        (_, Some(k)) => Node::Lin(Linear {
                            c: k * x.c,
                            sel: k * x.sel,
                            ctr: k * x.ctr,
                        }),
                        _ => Node::Other,
                    },
                    (ADD | SUB | MUL, _, _) => Node::Other,
                    _ => Node::Op(b, Box::new(l), Box::new(r)),
                });
            }
            // After a call, register stores and the "and" marker: the stack is left alone.
            0xA6..=0xAE | 0xB7 => {}
            0xAF..=0xB6 | 0xB9 | 0xBA => stack.push(Node::Other),
            ARG => {
                let Node::Lin(k) = stack.pop()? else {
                    return None;
                };
                let v = usize::try_from(k.constant()?)
                    .ok()
                    .and_then(|k| args.get(k).copied().flatten());
                stack.push(v.map_or(Node::Other, |v| Node::Lin(Linear::of(i64::from(v)))));
            }
            _ => return None,
        }
    }
    match &stack[..] {
        [_] => stack.pop(),
        _ => None,
    }
}

fn linear(e: Option<&[u8]>, args: &[Option<i32>]) -> Option<Linear> {
    match parse(e?, args)? {
        Node::Lin(l) => Some(l),
        _ => None,
    }
}

/// The parts of `n` joined by "and".
fn conjuncts<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    match n {
        Node::Op(AND, l, r) => {
            conjuncts(l, out);
            conjuncts(r, out);
        }
        _ => out.push(n),
    }
}

/// `call == v`, as (call, v).
fn compared(n: &Node) -> Option<(&Node, i64)> {
    match n {
        Node::Op(EQ, l, r) => match (&**l, &**r) {
            (c @ Node::Call(..), Node::Lin(v)) | (Node::Lin(v), c @ Node::Call(..)) => {
                Some((c, v.constant()?))
            }
            _ => None,
        },
        _ => None,
    }
}

/// What a menu entry's condition asks: goods held (`f47(3, g, 4, 1, 0) == 1`) and
/// a flag off (`f15(f) == 0`), each as a number in the loop's counter.
fn entry_terms(cond: &Node) -> Option<(Linear, Linear)> {
    let mut parts = Vec::new();
    conjuncts(cond, &mut parts);
    let (mut goods, mut flag) = (None, None);
    for p in parts {
        match compared(p) {
            Some((Node::Call(HOLDS, a), 1)) => {
                if let [Node::Lin(t), Node::Lin(g), Node::Lin(cmp), Node::Lin(n), _] = &a[..]
                    && t.constant() == Some(GOODS)
                    && cmp.constant() == Some(4)
                    && n.constant() == Some(1)
                {
                    goods = Some(*g);
                }
            }
            Some((Node::Call(FLAG, a), 0)) => {
                if let [Node::Lin(f)] = &a[..] {
                    flag = Some(*f);
                }
            }
            _ => {}
        }
    }
    Some((goods?, flag?))
}

/// One entry a menu offers: its number, the goods it asks to be held and the
/// flag it asks to be off.
type Entry = (i64, i64, i64);

/// The entries a machine's menu loop offers with the numbers it was called with,
/// added by bank 5's form of the commands `ids`.
fn menu(esd: &Esd, g: &Group, args: &[Option<i32>], ids: &[i32]) -> Vec<Entry> {
    let states = esd.states_of(g);
    // The counters the machine starts at 0.
    let started: HashSet<i64> = states
        .iter()
        .flat_map(|s| esd.commands_in(s.on_enter))
        .filter(|c| (c.bank, c.id) == SET_COUNTER)
        .filter_map(|c| {
            let a = esd.command_args(c);
            let k = linear(*a.first()?, args)?.constant()?;
            (linear(*a.get(1)?, args)?.constant()? == 0).then_some(k)
        })
        .collect();
    let mut out = Vec::new();
    for s in states {
        // The loop's end: `f100(k) > N` leaves it.
        let last = esd.conditions_in(s.conditions).iter().find_map(|c| {
            match parse(esd.expression(c.evaluator)?, args)? {
                Node::Op(EQ, l, r) => match (&*l, &*r) {
                    (Node::Op(GT, x, n), Node::Lin(one)) if one.constant() == Some(1) => {
                        match (&**x, &**n) {
                            (Node::Lin(x), Node::Lin(n))
                                if *x
                                    == (Linear {
                                        c: 0,
                                        sel: 0,
                                        ctr: 1,
                                    }) =>
                            {
                                n.constant()
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            }
        });
        let Some(last) = last.filter(|n| (0..MAX_ENTRIES).contains(n)) else {
            continue;
        };
        if started.is_empty() {
            continue;
        }
        for c in esd.commands_in(s.on_enter) {
            if c.bank != WHEN_BANK || !ids.contains(&c.id) {
                continue;
            }
            let a = esd.command_args(c);
            let (Some(Some(cond)), Some(index)) =
                (a.first(), a.get(1).and_then(|e| linear(*e, args)))
            else {
                continue;
            };
            let Some((goods, flag)) = parse(cond, args).as_ref().and_then(entry_terms) else {
                continue;
            };
            if [index, goods, flag]
                .iter()
                .any(|l| l.sel != 0 || l.ctr != 1)
            {
                continue;
            }
            for n in 0..=last {
                out.push((index.c + n, goods.c + n, flag.c + n));
            }
        }
    }
    out
}

/// What a machine's trades take and turn on for a choice `s`: goods `G + s`
/// and flag `H + s`, as (G, H).
fn trades(esd: &Esd, g: &Group, args: &[Option<i32>]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let states = esd.states_of(g);
    for s in states {
        let Some(goods) = taken(esd, s, args) else {
            continue;
        };
        // On with no choice between, as in [`crate::handover`].
        let mut seen = HashSet::new();
        let mut at = Some(s);
        while let Some(st) = at.filter(|st| seen.insert(esd.index_of(st))) {
            for c in esd.commands_in(st.on_enter) {
                if (c.bank, c.id) != SET_EVENT_FLAG {
                    continue;
                }
                let a = esd.command_args(c);
                let (Some(f), Some(on)) = (
                    a.first().and_then(|e| linear(*e, args)),
                    a.get(1).and_then(|e| linear(*e, args)),
                ) else {
                    continue;
                };
                if on.constant() == Some(1) && f.sel == 1 && f.ctr == 0 {
                    out.push((goods.c, f.c));
                }
            }
            at = next(esd, st);
        }
    }
    out
}

/// The goods a state takes for the choice, when that is what it takes.
fn taken(esd: &Esd, s: &State, args: &[Option<i32>]) -> Option<Linear> {
    esd.commands_in(s.on_enter)
        .iter()
        .filter(|c| (c.bank, c.id) == TAKE_ITEM)
        .find_map(|c| {
            let a = esd.command_args(c);
            let kind = linear(*a.first()?, args)?.constant()?;
            let g = linear(*a.get(1)?, args)?;
            (kind == GOODS && g.sel == 1 && g.ctr == 0).then_some(g)
        })
}

/// The state a state always moves on to: its only condition, when that is
/// always true.
fn next<'a>(esd: &'a Esd, s: &State) -> Option<&'a State> {
    match esd.conditions_in(s.conditions)[..] {
        [c] if esd.expression(c.evaluator) == Some(&[0x41, END][..]) => {
            esd.state_at(c.next_state).map(|(_, s)| s)
        }
        _ => None,
    }
}

/// Every machine reached from one nothing calls, with the numbers it is called
/// with.
fn reached(esd: &Esd) -> Vec<(&Group, Vec<Option<i32>>)> {
    let groups: HashMap<i64, &Group> = esd.groups.iter().map(|g| (g.id, g)).collect();
    let called: HashSet<i64> = esd
        .commands
        .iter()
        .filter(|c| c.bank == CALL_BANK)
        .map(|c| i64::from(c.id))
        .collect();
    let mut seen: HashSet<(i64, Vec<Option<i32>>)> = HashSet::new();
    let mut todo: Vec<(&Group, Vec<Option<i32>>, usize)> = esd
        .groups
        .iter()
        .filter(|g| !called.contains(&g.id))
        .map(|g| (g, Vec::new(), 0))
        .collect();
    let mut out = Vec::new();
    while let Some((g, args, depth)) = todo.pop() {
        if !seen.insert((g.id, args.clone())) {
            continue;
        }
        for s in esd.states_of(g) {
            let mut cmds: Vec<&crate::Command> = [s.on_enter, s.on_exit, s.on_update]
                .into_iter()
                .flat_map(|sp| esd.commands_in(sp))
                .collect();
            let mut conds = esd.conditions_in(s.conditions);
            while let Some(c) = conds.pop() {
                cmds.extend(esd.commands_in(c.on_pass));
                conds.extend(esd.conditions_in(c.subconditions));
            }
            for c in cmds.into_iter().filter(|c| c.bank == CALL_BANK) {
                let Some(&target) = groups.get(&i64::from(c.id)) else {
                    continue;
                };
                let passed: Vec<Option<i32>> = esd
                    .command_args(c)
                    .into_iter()
                    .map(|e| {
                        let v = linear(e, &args)?.constant()?;
                        i32::try_from(v).ok()
                    })
                    .collect();
                if depth < MAX_DEPTH {
                    todo.push((target, passed, depth + 1));
                }
            }
        }
        out.push((g, args));
    }
    out
}

/// Every item a script's menus take for a flag, read where an entry's offer and
/// the trade for its choice agree.
pub fn handovers(esd: &Esd) -> Vec<Handover> {
    handovers_by(esd, &[MENU_ENTRY.1])
}

/// The same, with menus whose entries are added by [`MENU_ENTRY_COUNTED`] too:
/// the Twin Maiden Husks take bell bearings so, goods `8910 + n` for flag
/// `11109710 + n` and goods `2008900 + n` for flag `11109790 + n`.
pub fn offers(esd: &Esd) -> Vec<Handover> {
    handovers_by(esd, &[MENU_ENTRY.1, MENU_ENTRY_COUNTED.1])
}

fn handovers_by(esd: &Esd, ids: &[i32]) -> Vec<Handover> {
    let mut entries: BTreeSet<Entry> = BTreeSet::new();
    let mut deals: BTreeSet<(i64, i64)> = BTreeSet::new();
    for (g, args) in reached(esd) {
        entries.extend(menu(esd, g, &args, ids));
        deals.extend(trades(esd, g, &args));
    }
    let mut found: BTreeMap<i32, BTreeSet<u32>> = BTreeMap::new();
    for &(index, goods, off) in &entries {
        for &(g, h) in &deals {
            if g + index == goods
                && h + index == off
                && let (Ok(goods), Ok(flag)) = (i32::try_from(goods), u32::try_from(off))
            {
                found.entry(goods).or_default().insert(flag);
            }
        }
    }
    found
        .into_iter()
        .map(|(goods, flags)| Handover { goods, flags })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: i32) -> Vec<u8> {
        let mut b = vec![INT];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }

    #[test]
    fn a_choice_and_a_loop_counter_read_as_numbers() {
        // (arg(1) + f23()) - arg(0), with arguments 1 and 8850.
        let e = [0x41, ARG, 0x40 + 23, 0x84, 0x8C, 0x40, ARG, 0x8E, END];
        let got = linear(Some(&e), &[Some(1), Some(8850)]);
        assert_eq!(
            got,
            Some(Linear {
                c: 8849,
                sel: 1,
                ctr: 0
            })
        );
        // An argument not known is not a number.
        assert_eq!(linear(Some(&e), &[Some(1)]), None);
    }

    #[test]
    fn a_menu_entry_asks_for_the_goods_held_and_the_flag_off() {
        // f47(3, 8850 + f100(0), 4, 1, 0) == 1 && f15(1037469300 + f100(0)) == 0
        let mut e = vec![0x40 + 47, 0x43];
        e.extend(int(8850));
        e.extend(int(100));
        e.extend([0x40, 0x85, 0x8C, 0x44, 0x41, 0x40, 0x89, 0x41, EQ]);
        e.extend([0x4F]);
        e.extend(int(1_037_469_300));
        e.extend(int(100));
        e.extend([0x40, 0x85, 0x8C, 0x85, 0x40, EQ, AND, END]);
        let n = parse(&e, &[]).unwrap();
        let (g, f) = entry_terms(&n).unwrap();
        assert_eq!((g.c, g.ctr, f.c, f.ctr), (8850, 1, 1_037_469_300, 1));
    }
}

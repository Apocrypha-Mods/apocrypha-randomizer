//! Items handed over in talk: a state that takes an item from the player
//! ([`TAKE_ITEM`]), and the flags turned on ([`SET_EVENT_FLAG`]) along the
//! run of states that lead into it, or follow it, with no choice between.
//! Sellen's script takes the Conspectus Scroll and then, in the next state,
//! sets the flag that puts Glintstone Cometshard and Star Shower on sale;
//! Corhyn's sets the flag for Radagon's Rings of Light and then takes the
//! Golden Order Principia.
//!
//! Only plain numbers are read: an item or flag passed down a call is not
//! followed.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::commands::{SET_EVENT_FLAG, TAKE_ITEM};
use crate::expr::{END, constant};
use crate::{Esd, State};

/// ShopLineupParam's type for goods, which TAKE_ITEM's first argument shares.
const GOODS: i32 = 3;
/// The expression that is always true: the small integer 1.
const TRUE: [u8; 2] = [0x41, END];

/// One item taken, and the flags that are turned on as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handover {
    /// A row of EquipParamGoods.
    pub goods: i32,
    pub flags: BTreeSet<u32>,
}

/// Every item the script takes on its own (one to a state), with the flags
/// set on the way to it and after it.
pub fn handovers(esd: &Esd) -> Vec<Handover> {
    let mut found: BTreeMap<i32, BTreeSet<u32>> = BTreeMap::new();
    for (i, state) in esd.states.iter().enumerate() {
        let here = taken(esd, state);
        let set_here = set(esd, state);
        if here.len() > 1 || (here.is_empty() && set_here.is_empty()) {
            continue;
        }
        // Walk on with no choice until another item is taken.
        let mut flags = set_here;
        let mut seen = HashSet::from([i]);
        let mut at = next(esd, state);
        let mut reached = None;
        while let Some((_, s)) = at.filter(|(j, _)| seen.insert(*j)) {
            if let [goods] = taken(esd, s)[..] {
                reached = Some(goods);
                break;
            }
            if !taken(esd, s).is_empty() {
                break;
            }
            flags.extend(set(esd, s));
            at = next(esd, s);
        }
        match (&here[..], reached) {
            // A take, and what follows it.
            (&[goods], _) => found.entry(goods).or_default().extend(flags),
            // Flags set on the way to a take.
            (&[], Some(goods)) => found.entry(goods).or_default().extend(flags),
            _ => {}
        }
    }
    found
        .into_iter()
        .filter(|(_, flags)| !flags.is_empty())
        .map(|(goods, flags)| Handover { goods, flags })
        .collect()
}

fn args(esd: &Esd, s: &State, command: (i32, i32)) -> Vec<Vec<Option<i32>>> {
    esd.commands_in(s.on_enter)
        .iter()
        .filter(|c| (c.bank, c.id) == command)
        .map(|c| {
            esd.command_args(c)
                .iter()
                .map(|e| e.and_then(constant))
                .collect()
        })
        .collect()
}

/// The goods a state takes.
fn taken(esd: &Esd, s: &State) -> Vec<i32> {
    args(esd, s, TAKE_ITEM)
        .into_iter()
        .filter_map(|a| match a[..] {
            [Some(GOODS), Some(id), ..] => Some(id),
            _ => None,
        })
        .collect()
}

/// The flags a state turns on.
fn set(esd: &Esd, s: &State) -> Vec<u32> {
    args(esd, s, SET_EVENT_FLAG)
        .into_iter()
        .filter_map(|a| match a[..] {
            [Some(f), Some(1)] => u32::try_from(f).ok(),
            _ => None,
        })
        .collect()
}

/// The state a state always moves on to, with its index: its only
/// condition, when that is always true.
fn next<'a>(esd: &'a Esd, s: &State) -> Option<(usize, &'a State)> {
    match esd.conditions_in(s.conditions)[..] {
        [c] if esd.expression(c.evaluator) == Some(&TRUE[..]) => esd.state_at(c.next_state),
        _ => None,
    }
}

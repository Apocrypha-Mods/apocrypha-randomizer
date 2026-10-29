//! What holds on every way to a state of a machine, from the machine's first
//! state: the [`Atom`]s of the conditions passed on the way, kept only where
//! every way passes them. A gift given in a state after a test of the flag
//! that Ranni's quest sets is given only once that flag is on.
//!
//! A condition's subconditions are tried once it passes, so a move through a
//! subcondition asks for both. Conditions passed over (an earlier one
//! failing) are not counted: what failing means is not read.

use std::collections::BTreeSet;

use crate::cond::{Atom, requires};
use crate::{Command, Condition, Esd, Group};

/// The atoms that hold on every way to each state of `g` (in the order of
/// [`Esd::states_of`]); `None` for a state no way reaches.
pub fn must(esd: &Esd, g: &Group) -> Vec<Option<BTreeSet<Atom>>> {
    let states = esd.states_of(g);
    let Some(first) = states.first().and_then(|s| esd.index_of(s)) else {
        return Vec::new();
    };
    let mut edges: Vec<(usize, usize, BTreeSet<Atom>)> = Vec::new();
    for (i, s) in states.iter().enumerate() {
        for (c, atoms) in conditions(esd, esd.conditions_in(s.conditions)) {
            if let Some((j, _)) = esd.state_at(c.next_state)
                && let Some(j) = j.checked_sub(first).filter(|&j| j < states.len())
            {
                edges.push((i, j, atoms));
            }
        }
    }
    let mut must: Vec<Option<BTreeSet<Atom>>> = vec![None; states.len()];
    must[0] = Some(BTreeSet::new());
    loop {
        let mut changed = false;
        for (a, b, atoms) in &edges {
            let Some(from) = &must[*a] else {
                continue;
            };
            let way: BTreeSet<Atom> = from.union(atoms).copied().collect();
            let next = match &must[*b] {
                None => way,
                Some(now) => now.intersection(&way).copied().collect(),
            };
            if must[*b].as_ref() != Some(&next) {
                must[*b] = Some(next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    must
}

/// Every condition reachable from `top` through subconditions, with what
/// holds once it passes.
pub fn conditions<'a>(
    esd: &'a Esd,
    top: Vec<&'a Condition>,
) -> Vec<(&'a Condition, BTreeSet<Atom>)> {
    let mut out = Vec::new();
    let mut todo: Vec<(&Condition, BTreeSet<Atom>)> =
        top.into_iter().map(|c| (c, BTreeSet::new())).collect();
    while let Some((c, mut atoms)) = todo.pop() {
        if out.len() > 10_000 {
            break;
        }
        if let Some(e) = esd.expression(c.evaluator) {
            atoms.extend(requires(e));
        }
        for sub in esd.conditions_in(c.subconditions) {
            todo.push((sub, atoms.clone()));
        }
        out.push((c, atoms));
    }
    out
}

/// Every command a state runs, with what holds when it runs beyond the
/// state's own `must`: its on-enter, on-exit and on-update commands ask for
/// nothing more, a condition's on-pass commands for what the condition does.
pub fn commands<'a>(esd: &'a Esd, s: &'a crate::State) -> Vec<(&'a Command, BTreeSet<Atom>)> {
    let mut out: Vec<(&Command, BTreeSet<Atom>)> = [s.on_enter, s.on_exit, s.on_update]
        .into_iter()
        .flat_map(|span| esd.commands_in(span))
        .map(|c| (c, BTreeSet::new()))
        .collect();
    for (c, atoms) in conditions(esd, esd.conditions_in(s.conditions)) {
        out.extend(
            esd.commands_in(c.on_pass)
                .iter()
                .map(|cmd| (cmd, atoms.clone())),
        );
    }
    out
}

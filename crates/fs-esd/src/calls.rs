//! Following calls between a script's state machines.
//!
//! A command in bank 6 does not do anything itself: its id names another
//! state machine of the same script (0x7FFFFFxx), started with the command's
//! arguments, which that machine reads back with [`expr::argument`]. Which
//! machine an id names differs from script to script, so a call means nothing
//! until it is followed. [`resolve`] follows every call from the machines
//! nothing calls, carrying plain numbers down, and reports each of the game's
//! own commands (every other bank) with the numbers it ends up with.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::cond::Atom;
use crate::{Esd, Group, expr, reach};

/// The bank whose commands start another state machine of the script.
pub const CALL_BANK: i32 = 6;
/// How deep calls are followed; the game's scripts go 5 deep.
const MAX_DEPTH: usize = 16;

/// One of the game's commands, with its arguments as far as they are known.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Resolved {
    /// The machine the trace started from: one nothing calls.
    pub root: i64,
    /// The machine the command is in.
    pub group: i64,
    pub bank: i32,
    pub id: i32,
    /// Each argument's value when it is a plain number, written in the
    /// command or passed down by the calls that lead to it.
    pub args: Vec<Option<i32>>,
    /// What holds on every way to the command along this chain of calls
    /// ([`reach`]): the conditions passed in each machine on the way.
    pub needs: BTreeSet<Atom>,
}

/// What one lot's award asks on one chain of calls, with what failing means
/// counted: the [`reach::must_strictly`] of each machine on the way there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StrictWay {
    pub bank: i32,
    pub id: i32,
    pub args: Vec<Option<i32>>,
    pub needs: BTreeSet<Atom>,
}

/// Every one of the game's commands each root machine reaches, as [`resolve`]
/// finds them, each with what holds there once failed conditions count.
pub fn resolve_strictly(esd: &Esd) -> Vec<StrictWay> {
    let groups: HashMap<i64, &Group> = esd.groups.iter().map(|g| (g.id, g)).collect();
    let called: HashSet<i64> = esd
        .commands
        .iter()
        .filter(|c| c.bank == CALL_BANK)
        .map(|c| i64::from(c.id))
        .collect();
    let mut must: HashMap<i64, Vec<Option<BTreeSet<Atom>>>> = HashMap::new();
    let mut seen: HashSet<Call> = HashSet::new();
    let mut out = Vec::new();
    let mut todo: Vec<(Call, usize)> = esd
        .groups
        .iter()
        .filter(|g| !called.contains(&g.id))
        .map(|g| ((g.id, Vec::new(), BTreeSet::new()), 0))
        .collect();
    while let Some(((id, passed, held), depth)) = todo.pop() {
        let g = groups[&id];
        let here = must
            .entry(id)
            .or_insert_with(|| reach::must_strictly(esd, g))
            .clone();
        for (s, here) in esd.states_of(g).iter().zip(here) {
            let here = here.unwrap_or_default();
            for (c, more) in reach::commands_strictly(esd, s) {
                let needs: BTreeSet<Atom> =
                    held.iter().chain(&here).chain(&more).copied().collect();
                let args: Vec<Option<i32>> = esd
                    .command_args(c)
                    .into_iter()
                    .map(|e| {
                        let e = e?;
                        expr::constant(e)
                            .or_else(|| passed.get(expr::argument(e)?).copied().flatten())
                    })
                    .collect();
                if c.bank == CALL_BANK {
                    let target = i64::from(c.id);
                    if groups.contains_key(&target)
                        && depth < MAX_DEPTH
                        && seen.insert((target, args.clone(), needs.clone()))
                    {
                        todo.push(((target, args, needs), depth + 1));
                    }
                    continue;
                }
                out.push(StrictWay {
                    bank: c.bank,
                    id: c.id,
                    args,
                    needs,
                });
            }
        }
    }
    out
}

/// Every one of the game's commands each root machine reaches, once for each
/// set of arguments it is reached with and what holds there.
pub fn resolve(esd: &Esd) -> Vec<Resolved> {
    let groups: HashMap<i64, &Group> = esd.groups.iter().map(|g| (g.id, g)).collect();
    let called: HashSet<i64> = esd
        .commands
        .iter()
        .filter(|c| c.bank == CALL_BANK)
        .map(|c| i64::from(c.id))
        .collect();
    let mut walk = Walk {
        esd,
        groups,
        must: HashMap::new(),
        seen: HashSet::new(),
        out: Vec::new(),
    };
    for g in esd.groups.iter().filter(|g| !called.contains(&g.id)) {
        walk.seen.clear();
        walk.visit(g, &[], &BTreeSet::new(), g.id, 0);
    }
    walk.out
}

/// What one machine's call is: the machine, the numbers it is passed, and
/// what holds where it is called.
type Call = (i64, Vec<Option<i32>>, BTreeSet<Atom>);

struct Walk<'a> {
    esd: &'a Esd,
    groups: HashMap<i64, &'a Group>,
    /// Each machine's [`reach::must`], worked out once.
    must: HashMap<i64, Vec<Option<BTreeSet<Atom>>>>,
    seen: HashSet<Call>,
    out: Vec<Resolved>,
}

impl<'a> Walk<'a> {
    fn visit(
        &mut self,
        g: &'a Group,
        passed: &[Option<i32>],
        held: &BTreeSet<Atom>,
        root: i64,
        depth: usize,
    ) {
        let esd = self.esd;
        let must = self
            .must
            .entry(g.id)
            .or_insert_with(|| reach::must(esd, g))
            .clone();
        for (s, here) in esd.states_of(g).iter().zip(must) {
            // A state no way reaches still runs, as far as is known: from
            // somewhere the trace does not follow.
            let here = here.unwrap_or_default();
            for (c, more) in reach::commands(esd, s) {
                let needs: BTreeSet<Atom> =
                    held.iter().chain(&here).chain(&more).copied().collect();
                let args: Vec<Option<i32>> = esd
                    .command_args(c)
                    .into_iter()
                    .map(|e| {
                        let e = e?;
                        expr::constant(e)
                            .or_else(|| passed.get(expr::argument(e)?).copied().flatten())
                    })
                    .collect();
                if c.bank == CALL_BANK {
                    if let Some(&target) = self.groups.get(&i64::from(c.id))
                        && depth < MAX_DEPTH
                        && self.seen.insert((target.id, args.clone(), needs.clone()))
                    {
                        self.visit(target, &args, &needs, root, depth + 1);
                    }
                    continue;
                }
                self.out.push(Resolved {
                    root,
                    group: g.id,
                    bank: c.bank,
                    id: c.id,
                    args,
                    needs,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Condition, Span, State};

    /// Machine 1 calls machine 2 with 100000; machine 2 runs command 1[104]
    /// on its argument 0, and a condition's pass runs 1[7] on the constant 5.
    fn sample() -> Esd {
        let mut esd = Esd {
            unk18: 6,
            hash: [0; 16],
            groups: vec![],
            states: vec![],
            conditions: vec![],
            commands: vec![],
            args: vec![],
            condition_list: vec![],
            expressions: vec![],
            name: vec![],
            nameless: false,
        };
        let none = Span {
            offset: -1,
            count: 0,
        };
        let n_groups = 2;
        let states_at = 0x48 + n_groups * 0x20;
        let conds_at = states_at + 2 * 0x48;
        let cmds_at = conds_at + 0x38;
        let args_at = cmds_at + 3 * 0x18;
        let list_at = args_at + 3 * 0x10;
        let expr_at = list_at + 8;
        esd.groups = vec![
            Group {
                id: 1,
                states: Span {
                    offset: states_at,
                    count: 1,
                },
                states_again: states_at,
            },
            Group {
                id: 2,
                states: Span {
                    offset: states_at + 0x48,
                    count: 1,
                },
                states_again: states_at + 0x48,
            },
        ];
        esd.states = vec![
            State {
                id: 0,
                conditions: none,
                on_enter: Span {
                    offset: cmds_at,
                    count: 1,
                },
                on_exit: none,
                on_update: none,
            },
            State {
                id: 0,
                conditions: Span {
                    offset: list_at,
                    count: 1,
                },
                on_enter: Span {
                    offset: cmds_at + 0x18,
                    count: 1,
                },
                on_exit: none,
                on_update: none,
            },
        ];
        esd.conditions = vec![Condition {
            next_state: -1,
            on_pass: Span {
                offset: cmds_at + 0x30,
                count: 1,
            },
            subconditions: none,
            evaluator: none,
        }];
        esd.condition_list = vec![conds_at];
        esd.commands = vec![
            Command {
                bank: CALL_BANK,
                id: 2,
                args: Span {
                    offset: args_at,
                    count: 1,
                },
            },
            Command {
                bank: 1,
                id: 104,
                args: Span {
                    offset: args_at + 0x10,
                    count: 1,
                },
            },
            Command {
                bank: 1,
                id: 7,
                args: Span {
                    offset: args_at + 0x20,
                    count: 1,
                },
            },
        ];
        // 100000; argument 0; the constant 5.
        esd.expressions = vec![
            0x82, 0xA0, 0x86, 0x01, 0x00, 0xA1, 0x40, 0xB8, 0xA1, 0x45, 0xA1,
        ];
        esd.args = vec![
            Span {
                offset: expr_at,
                count: 6,
            },
            Span {
                offset: expr_at + 6,
                count: 3,
            },
            Span {
                offset: expr_at + 9,
                count: 2,
            },
        ];
        esd
    }

    #[test]
    fn a_call_carries_its_numbers_into_the_machine_it_starts() {
        // Written and read back, so the spans are known to agree with the
        // layout.
        let esd = Esd::read(&sample().write().unwrap()).unwrap();
        let got = resolve(&esd);
        assert_eq!(
            got,
            vec![
                Resolved {
                    root: 1,
                    group: 2,
                    bank: 1,
                    id: 104,
                    args: vec![Some(100_000)],
                    needs: BTreeSet::new(),
                },
                Resolved {
                    root: 1,
                    group: 2,
                    bank: 1,
                    id: 7,
                    args: vec![Some(5)],
                    needs: BTreeSet::new(),
                },
            ]
        );
    }
}

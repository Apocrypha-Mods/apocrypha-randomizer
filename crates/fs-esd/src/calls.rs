//! Following calls between a script's state machines.
//!
//! A command in bank 6 does not do anything itself: its id names another
//! state machine of the same script (0x7FFFFFxx), started with the command's
//! arguments, which that machine reads back with [`expr::argument`]. Which
//! machine an id names differs from script to script, so a call means nothing
//! until it is followed. [`resolve`] follows every call from the machines
//! nothing calls, carrying plain numbers down, and reports each of the game's
//! own commands (every other bank) with the numbers it ends up with.

use std::collections::{HashMap, HashSet};

use crate::{Condition, Esd, Group, expr};

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
}

/// Every one of the game's commands each root machine reaches, once for each
/// set of arguments it is reached with.
pub fn resolve(esd: &Esd) -> Vec<Resolved> {
    let groups: HashMap<i64, &Group> = esd.groups.iter().map(|g| (g.id, g)).collect();
    let called: HashSet<i64> = esd
        .commands
        .iter()
        .filter(|c| c.bank == CALL_BANK)
        .map(|c| i64::from(c.id))
        .collect();
    let mut out = Vec::new();
    for g in esd.groups.iter().filter(|g| !called.contains(&g.id)) {
        let mut seen = HashSet::new();
        visit(esd, &groups, g, &[], g.id, 0, &mut seen, &mut out);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn visit(
    esd: &Esd,
    groups: &HashMap<i64, &Group>,
    g: &Group,
    passed: &[Option<i32>],
    root: i64,
    depth: usize,
    seen: &mut HashSet<(i64, Vec<Option<i32>>)>,
    out: &mut Vec<Resolved>,
) {
    let mut commands = Vec::new();
    for s in esd.states_of(g) {
        for span in [s.on_enter, s.on_exit, s.on_update] {
            commands.extend(esd.commands_in(span));
        }
        let mut conds: Vec<&Condition> = esd.conditions_in(s.conditions);
        let mut visited = HashSet::new();
        while let Some(c) = conds.pop() {
            if !visited.insert(c as *const Condition) {
                continue;
            }
            commands.extend(esd.commands_in(c.on_pass));
            conds.extend(esd.conditions_in(c.subconditions));
        }
    }
    for c in commands {
        let args: Vec<Option<i32>> = esd
            .command_args(c)
            .into_iter()
            .map(|e| {
                let e = e?;
                expr::constant(e).or_else(|| passed.get(expr::argument(e)?).copied().flatten())
            })
            .collect();
        if c.bank == CALL_BANK {
            if let Some(target) = groups.get(&i64::from(c.id))
                && depth < MAX_DEPTH
                && seen.insert((target.id, args.clone()))
            {
                visit(esd, groups, target, &args, root, depth + 1, seen, out);
            }
            continue;
        }
        out.push(Resolved {
            root,
            group: g.id,
            bank: c.bank,
            id: c.id,
            args,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Span, State};

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
                    args: vec![Some(100_000)]
                },
                Resolved {
                    root: 1,
                    group: 2,
                    bank: 1,
                    id: 7,
                    args: vec![Some(5)]
                },
            ]
        );
    }
}

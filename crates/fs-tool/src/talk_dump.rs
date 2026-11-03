//! Talk scripts written out for reading by hand: each machine's states, the
//! conditions that leave them and the commands they run, with every expression
//! shown by [`fs_esd::cond::show`].

use std::path::PathBuf;

use fs_bytes::Result;
use fs_esd::cond::show;
use fs_esd::{Command, Condition, Esd, Group};

/// Print the machines of the scripts named (all machines, or only `groups`),
/// or with `find`, every script and machine whose expressions hold one of those
/// numbers as a 4-byte word.
pub fn run(game: PathBuf, scripts: &[String], groups: &[i64], find: &[i32]) -> Result<()> {
    let s = gamedata::Sources::open(&game)?;
    for (map, name, esd) in &s.talk {
        if !find.is_empty() {
            for g in &esd.groups {
                if holds(esd, g, find) {
                    println!("{map:?} {name} machine {:#x}", g.id);
                }
            }
            continue;
        }
        if !scripts.iter().any(|x| x == "all" || x == name) {
            continue;
        }
        println!("== {name} ({map:?})");
        for g in esd
            .groups
            .iter()
            .filter(|g| groups.is_empty() || groups.contains(&g.id))
        {
            machine(esd, g);
        }
    }
    Ok(())
}

fn holds(esd: &Esd, g: &Group, find: &[i32]) -> bool {
    let has = |e: Option<&[u8]>| {
        e.is_some_and(|e| {
            e.windows(4)
                .any(|w| find.contains(&i32::from_le_bytes(w.try_into().unwrap())))
        })
    };
    esd.states_of(g).iter().any(|s| {
        let mut cmds: Vec<&Command> = [s.on_enter, s.on_exit, s.on_update]
            .into_iter()
            .flat_map(|sp| esd.commands_in(sp))
            .collect();
        let mut todo = esd.conditions_in(s.conditions);
        while let Some(c) = todo.pop() {
            if has(esd.expression(c.evaluator)) {
                return true;
            }
            cmds.extend(esd.commands_in(c.on_pass));
            todo.extend(esd.conditions_in(c.subconditions));
        }
        cmds.iter()
            .any(|c| esd.command_args(c).into_iter().any(has))
    })
}

fn command(esd: &Esd, c: &Command) -> String {
    let args: Vec<String> = esd
        .command_args(c)
        .into_iter()
        .map(|a| a.map_or("?".into(), show))
        .collect();
    format!("{}[{:#x}]({})", c.bank, c.id, args.join(", "))
}

fn machine(esd: &Esd, g: &Group) {
    println!("machine {:#x}", g.id);
    let first = esd
        .states_of(g)
        .first()
        .and_then(|s| esd.index_of(s))
        .unwrap_or(0);
    for (i, s) in esd.states_of(g).iter().enumerate() {
        println!("  state {i} (id {})", s.id);
        for (what, span) in [
            ("enter", s.on_enter),
            ("exit", s.on_exit),
            ("update", s.on_update),
        ] {
            for c in esd.commands_in(span) {
                println!("    {what} {}", command(esd, c));
            }
        }
        for c in esd.conditions_in(s.conditions) {
            condition(esd, c, first, 4);
        }
    }
}

fn condition(esd: &Esd, c: &Condition, first: usize, indent: usize) {
    let to = esd
        .state_at(c.next_state)
        .map_or("-".into(), |(j, _)| (j as i64 - first as i64).to_string());
    let e = esd.expression(c.evaluator).map_or("?".into(), show);
    println!("{:indent$}if {e} -> {to}", "");
    for cmd in esd.commands_in(c.on_pass) {
        println!("{:indent$}  then {}", "", command(esd, cmd));
    }
    for sub in esd.conditions_in(c.subconditions) {
        condition(esd, sub, first, indent + 2);
    }
}

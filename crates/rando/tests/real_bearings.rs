//! Bell bearings given in talk, on the player's own game (`APOC_ER_STOCK`;
//! skipped when unset): the flags, lot rows and text the seed adds are free in
//! the game, every menu of each merchant's scripts gets the entry, the entry
//! awards the merchant's bearing once, and nothing else in the scripts changes.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use fs_esd::commands::{AWARD_ITEM_LOT, MENU_ENTRY, SET_EVENT_FLAG};
use fs_esd::cond::{Atom, requires};
use fs_esd::menu::shop_menus;
use fs_esd::{Esd, expr};
use gamedata::{GameFiles, Model, Sources, elden_ring};
use rando::bearings;

struct Game {
    files: GameFiles,
    sources: Sources,
    model: Model,
}

fn game() -> Option<&'static Game> {
    static GAME: OnceLock<Option<Game>> = OnceLock::new();
    GAME.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
        let files = GameFiles::open(&dir, &[]).unwrap();
        let sources = Sources::open_with(&dir, &files).unwrap();
        let model = elden_ring::extract(&sources).unwrap();
        Some(Game {
            files,
            sources,
            model,
        })
    })
    .as_ref()
}

macro_rules! game_or_skip {
    () => {
        match game() {
            Some(g) => g,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

/// Whether a four-byte number appears anywhere in some bytes.
fn holds(bytes: &[u8], v: u32) -> bool {
    bytes.windows(4).any(|w| w == v.to_le_bytes())
}

#[test]
fn the_flags_lots_and_text_added_are_free_in_the_game() {
    let g = game_or_skip!();
    let graph = logic::elden_ring();
    let flags: Vec<u32> = graph.bearings.iter().map(|b| b.given).collect();
    assert_eq!(
        flags.iter().collect::<BTreeSet<_>>().len(),
        flags.len(),
        "each merchant a flag of their own"
    );
    for name in g.sources.reg.table_names() {
        let t = g.sources.reg.table(name).unwrap();
        for r in &t.rows {
            for f in &flags {
                assert_ne!(r.id as u32, *f, "{name} has a row {f}");
                assert!(!holds(&r.data, *f), "{name} row {} holds {f}", r.id);
            }
        }
    }
    let events = g
        .sources
        .scripts
        .iter()
        .map(|(_, e)| e)
        .chain([&g.sources.common, &g.sources.common_func]);
    for e in events {
        for ev in &e.events {
            for i in &ev.instructions {
                for f in &flags {
                    assert!(!holds(&i.args, *f), "event {} holds {f}", ev.id);
                }
            }
        }
    }
    for (_, name, esd) in &g.sources.talk {
        for f in &flags {
            assert!(!holds(&esd.expressions, *f), "{name} holds {f}");
        }
    }
    for language in rando::LANGUAGES {
        let path = format!("/msg/{language}/menu_dlc02.msgbnd.dcx");
        let (_, plain) = fs_dcx::decompress(&g.files.read(&path).unwrap()).unwrap();
        let bnd = fs_bnd4::Bnd4::read(&plain).unwrap();
        let talk = bnd
            .files
            .iter()
            .find(|f| f.short_name() == Some("EventTextForTalk.fmg"))
            .unwrap();
        let fmg = fs_fmg::Fmg::read(&talk.data).unwrap();
        assert!(!fmg.entries.contains_key(&bearings::TEXT), "{language}");
        assert!(
            fmg.entries.contains_key(&20_000_009),
            "{language} has Leave"
        );
    }
}

/// A machine written out by what it means: each state's commands and
/// conditions, a condition's next state by its place in the machine.
fn meaning(esd: &Esd, group: usize) -> Vec<String> {
    let g = &esd.groups[group];
    let states = esd.states_of(g);
    let first = states.first().and_then(|s| esd.index_of(s)).unwrap_or(0);
    states
        .iter()
        .map(|s| {
            let mut out = format!("{}:", s.id);
            for span in [s.on_enter, s.on_exit, s.on_update] {
                for c in esd.commands_in(span) {
                    out += &format!(" {}[{}]{:?}", c.bank, c.id, esd.command_args(c));
                }
                out += " |";
            }
            let mut todo: Vec<_> = esd.conditions_in(s.conditions);
            todo.reverse();
            while let Some(c) = todo.pop() {
                let to = esd
                    .state_at(c.next_state)
                    .map(|(i, _)| i as i64 - first as i64);
                out += &format!(" if {:?} -> {to:?}", esd.expression(c.evaluator));
                for p in esd.commands_in(c.on_pass) {
                    out += &format!(" then {}[{}]{:?}", p.bank, p.id, esd.command_args(p));
                }
                let mut subs = esd.conditions_in(c.subconditions);
                subs.reverse();
                todo.extend(subs);
            }
            out
        })
        .collect()
}

#[test]
fn every_merchant_menu_offers_the_bearing_once_and_nothing_else_changes() {
    let g = game_or_skip!();
    let graph = logic::elden_ring();
    let mut reg = g.sources.reg.clone();
    let applied = bearings::apply(&g.sources, &graph, &g.model, &mut reg).unwrap();
    assert_eq!(
        applied.log.len(),
        graph.bearings.len(),
        "{:#?}",
        applied.log
    );
    let given = bearings::given(&graph, &g.model).unwrap();

    // The binders as they will be written read back as the scripts changed.
    let written = bearings::binders(&g.files, &applied).unwrap();
    assert_eq!(written.len(), applied.binders.len());
    for (path, data) in &written {
        let (_, plain) = fs_dcx::decompress(data).unwrap();
        let bnd = fs_bnd4::Bnd4::read(&plain).unwrap();
        let scripts = &applied.binders[&format!("/{path}")];
        for f in &bnd.files {
            let name = f.short_name().unwrap().trim_end_matches(".esd");
            if let Some(esd) = scripts.get(name) {
                assert_eq!(&Esd::read(&f.data).unwrap(), esd, "{name}");
            }
        }
    }

    // The lots.
    let lots = reg.table("ItemLotParam_map").unwrap();
    let layout = fs_param::layout::for_table("ItemLotParam_map").unwrap();
    for b in &given {
        let row = lots
            .rows
            .iter()
            .find(|r| r.id as u32 == b.entry.given)
            .unwrap();
        let f = |n: &str, i| layout.field(n).get(&row.data, i);
        assert_eq!(
            f("item_id", 0),
            i64::from(b.bearing.goods),
            "{}",
            b.entry.item
        );
        assert_eq!(
            (f("item_category", 0), f("count", 0), f("weight", 0)),
            (1, 1, 1000)
        );
        assert_eq!(f("get_item_flag", 0), i64::from(b.entry.given));
        assert!((1..8).all(|s| f("item_id", s) == 0), "{}", b.entry.item);
    }
    assert!(lots.rows.windows(2).all(|w| w[0].id < w[1].id), "sorted");

    // Each script.
    let mut menus_seen = 0;
    for (map, name, before) in &g.sources.talk {
        let path = format!("/script/talk/{map}.talkesdbnd.dcx");
        let after = applied.binders.get(&path).and_then(|s| s.get(name));
        let owner = given.iter().find(|b| {
            *name != b.bearing.script
                && shop_menus(before)
                    .iter()
                    .any(|m| m.ranges.iter().any(|r| b.bearing.ranges.contains(r)))
        });
        let (Some(after), Some(b)) = (after, owner) else {
            assert!(after.is_none() && owner.is_none(), "{name}");
            continue;
        };
        let menus = shop_menus(before);
        let edited: Vec<usize> = menus
            .iter()
            .filter(|m| m.ranges.iter().any(|r| b.bearing.ranges.contains(r)))
            .map(|m| m.group)
            .collect();
        assert_eq!(after.groups.len(), before.groups.len());
        for gi in 0..before.groups.len() {
            let (old, new) = (meaning(before, gi), meaning(after, gi));
            if !edited.contains(&gi) {
                assert_eq!(old, new, "{name} machine {gi} changed");
                continue;
            }
            menus_seen += 1;
            // One state more; the others the same, but for the entry and the test.
            assert_eq!(new.len(), old.len() + 1, "{name} machine {gi}");
            let changed = old.iter().zip(&new).filter(|(a, b)| a != b).count();
            assert!(
                changed <= 2,
                "{name} machine {gi}: {changed} states changed"
            );
            let m = shop_menus(after)
                .into_iter()
                .find(|m| m.group == gi)
                .unwrap();
            // The entry: offered while the gift's flag and the Husks' are off.
            let grp = &after.groups[gi];
            let entry = after
                .states_of(grp)
                .iter()
                .flat_map(|s| after.commands_in(s.on_enter))
                .find(|c| {
                    (c.bank, c.id) == (5, MENU_ENTRY.1)
                        && after
                            .command_args(c)
                            .get(2)
                            .copied()
                            .flatten()
                            .and_then(expr::constant)
                            == Some(bearings::TEXT)
                })
                .unwrap_or_else(|| panic!("{name} machine {gi}: no entry"));
            let args = after.command_args(entry);
            assert_eq!(
                requires(args[0].unwrap()),
                vec![
                    Atom::Flag(b.entry.given, false),
                    Atom::Flag(b.bearing.flag, false)
                ]
            );
            let choice = args[1].and_then(expr::constant).unwrap();
            // Its choice leads to the gift, which goes back to empty the menu.
            let test = after.conditions_in(after.states[m.choose].conditions)[0];
            let mut want = expr::int(23);
            want.extend([0x84, 0xA6]);
            want.extend(expr::int(choice));
            want.extend([0x95, expr::END]);
            assert_eq!(after.expression(test.evaluator), Some(&want[..]));
            let (gift, state) = after.state_at(test.next_state).unwrap();
            let cmds = after.commands_in(state.on_enter);
            assert_eq!(
                cmds.iter().map(|c| (c.bank, c.id)).collect::<Vec<_>>(),
                vec![AWARD_ITEM_LOT, SET_EVENT_FLAG],
                "{name}"
            );
            assert_eq!(
                after.command_args(&cmds[0])[0].and_then(expr::constant),
                Some(b.entry.given as i32)
            );
            assert_eq!(
                after.command_args(&cmds[1])[0].and_then(expr::constant),
                Some(b.entry.given as i32)
            );
            let back = after.conditions_in(after.states[gift].conditions);
            assert_eq!(back.len(), 1);
            assert_eq!(
                after.state_at(back[0].next_state).map(|(i, _)| i),
                Some(m.clear)
            );
        }
    }
    // Every merchant's menus, 40 in 26 scripts: Patches has 13, Bernahl 5,
    // Sellen and Corhyn 4, Moore 1 and so on.
    assert_eq!(menus_seen, 40);
}

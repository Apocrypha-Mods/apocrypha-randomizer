//! What the talk scripts are taken to say, re-checked on the player's own
//! game (`APOC_ER_STOCK`; skipped when unset): the expression readings in
//! `fs-esd/src/expr.rs` and the commands in `fs-esd/src/commands.rs`.

use std::collections::HashSet;
use std::sync::OnceLock;

use fs_esd::calls::{CALL_BANK, Resolved, resolve};
use fs_esd::commands::{AWARD_ITEM_LOT, OPEN_SHOP};
use fs_esd::expr;
use gamedata::Sources;

fn sources() -> Option<&'static Sources> {
    static SOURCES: OnceLock<Option<Sources>> = OnceLock::new();
    SOURCES
        .get_or_init(|| {
            let dir = std::env::var_os("APOC_ER_STOCK")?;
            Some(Sources::open(std::path::Path::new(&dir)).unwrap())
        })
        .as_ref()
}

macro_rules! sources_or_skip {
    () => {
        match sources() {
            Some(s) => s,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

fn rows(s: &Sources, table: &str) -> HashSet<i32> {
    s.reg
        .table(table)
        .unwrap()
        .rows
        .iter()
        .map(|r| r.id)
        .collect()
}

fn resolved(s: &Sources, command: (i32, i32)) -> Vec<Resolved> {
    s.talk
        .iter()
        .flat_map(|(_, _, e)| resolve(e))
        .filter(|r| (r.bank, r.id) == command)
        .collect()
}

/// Distinct plain-number values of one argument, ignoring small ones.
fn values(rs: &[Resolved], arg: usize) -> HashSet<i32> {
    rs.iter()
        .filter_map(|r| r.args.get(arg).copied().flatten())
        .filter(|v| v.abs() > 1000)
        .collect()
}

#[test]
fn every_talk_script_is_read() {
    let s = sources_or_skip!();
    assert_eq!(s.talk.len(), 365, "every ESD in the game");
}

#[test]
fn every_argument_ends_and_calls_name_machines_of_their_own_script() {
    let s = sources_or_skip!();
    let (mut args, mut plain, mut passed, mut calls, mut found) = (0, 0, 0, 0, 0);
    for (_, name, e) in &s.talk {
        let groups: HashSet<i64> = e.groups.iter().map(|g| g.id).collect();
        for c in &e.commands {
            for a in e.command_args(c) {
                let a = a.unwrap_or_else(|| panic!("{name}: an argument outside the expressions"));
                assert_eq!(a.last(), Some(&expr::END), "{name}");
                args += 1;
                plain += usize::from(expr::constant(a).is_some());
                passed += usize::from(expr::argument(a).is_some());
            }
            if c.bank == CALL_BANK {
                calls += 1;
                found += usize::from(groups.contains(&i64::from(c.id)));
            }
        }
    }
    eprintln!(
        "arguments {args}: plain numbers {plain}, passed on {passed}; calls {calls}, to a machine of the script {found}"
    );
    assert_eq!(
        found, calls,
        "every bank-6 command names a machine of its own script"
    );
    assert!(plain * 100 >= args * 70);
}

#[test]
fn award_item_lot_gives_item_lots() {
    let s = sources_or_skip!();
    let v = values(&resolved(s, AWARD_ITEM_LOT), 0);
    let hit = v.intersection(&rows(s, "ItemLotParam_map")).count();
    eprintln!("AWARD_ITEM_LOT: {hit}/{} distinct lots are rows", v.len());
    assert!(hit * 100 >= v.len() * 90);
}

#[test]
fn open_shop_opens_a_range_of_shop_rows() {
    let s = sources_or_skip!();
    let rs = resolved(s, OPEN_SHOP);
    for r in &rs {
        if let [Some(first), Some(last), ..] = r.args[..] {
            assert!(first <= last, "{first} > {last}");
        }
    }
    let v = values(&rs, 0);
    let hit = v.intersection(&rows(s, "ShopLineupParam")).count();
    eprintln!("OPEN_SHOP: {hit}/{} distinct first rows are rows", v.len());
    assert!(hit * 100 >= v.len() * 75);
}

#[test]
fn a_book_chosen_from_a_menu_is_followed_through_calls() {
    let s = sources_or_skip!();
    let pairs = |script: &str| -> Vec<(i32, Vec<u32>)> {
        s.talk
            .iter()
            .filter(|(_, n, _)| n == script)
            .flat_map(|(_, _, e)| fs_esd::choice::handovers(e))
            .map(|h| (h.goods, h.flags.into_iter().collect()))
            .collect()
    };
    // Miriel: the five scrolls, eleven prayerbooks and the Academy Scroll, each
    // with its own flag.
    let miriel = pairs("t330006000");
    let want: Vec<(i32, Vec<u32>)> = (0..17)
        .map(|i| (8850 + i, vec![1_037_469_300 + i as u32]))
        .collect();
    assert_eq!(miriel, want);
    // Seluvis takes the scrolls the same way.
    let seluvis = pairs("t307006000");
    assert!(
        seluvis.contains(&(8850, vec![1_034_509_450])),
        "{seluvis:?}"
    );
    assert!(
        seluvis.contains(&(8866, vec![1_034_509_455])),
        "{seluvis:?}"
    );
    // A script with no such menu gives none.
    assert!(pairs("t316211400").is_empty());
}

#[test]
fn handing_over_a_book_puts_its_spells_on_sale() {
    let s = sources_or_skip!();
    let flags_for = |script: &str, goods: i32| -> Vec<u32> {
        s.talk
            .iter()
            .filter(|(_, n, _)| n == script)
            .flat_map(|(_, _, e)| fs_esd::handover::handovers(e))
            .filter(|h| h.goods == goods)
            .flat_map(|h| h.flags)
            .collect()
    };
    // Sellen takes the scroll first; Corhyn sets the flag first.
    for (script, goods, flag) in [
        ("t316211400", 8850, 1044369244), // Conspectus Scroll: Glintstone Cometshard
        ("t316211400", 8866, 1044369250), // Academy Scroll: Great Glintstone Shard
        ("t351001100", 8862, 11109881),   // Golden Order Principia: Radagon's Rings of Light
        ("t351001100", 8865, 11109884),   // Ancient Dragon Prayerbook
    ] {
        assert_eq!(
            flags_for(script, goods),
            vec![flag],
            "{script} taking {goods}"
        );
    }
    // Each flag goes on sale with the shop rows it names.
    let shop = fs_param::layout::for_table("ShopLineupParam").unwrap();
    let on_sale: HashSet<i64> = s
        .reg
        .table("ShopLineupParam")
        .unwrap()
        .rows
        .iter()
        .map(|r| shop.field("on_sale_flag").get(&r.data, 0))
        .collect();
    assert!(on_sale.contains(&1044369244) && on_sale.contains(&11109881));
}

#[test]
fn conditions_read_as_one_value() {
    let s = sources_or_skip!();
    let (mut ok, mut all, mut asks) = (0, 0, 0);
    for (_, _, esd) in &s.talk {
        for c in &esd.conditions {
            let Some(e) = esd.expression(c.evaluator) else {
                continue;
            };
            all += 1;
            ok += usize::from(fs_esd::cond::understood(e));
            asks += usize::from(!fs_esd::cond::requires(e).is_empty());
        }
    }
    eprintln!("conditions: {ok}/{all} understood, {asks} ask for a flag or an item");
    assert_eq!(ok, all, "every condition reads as one value");
}

#[test]
fn every_shop_menu_is_found_and_takes_a_new_entry() {
    use fs_esd::menu::{Gift, add_gift, shop_menus};
    let s = sources_or_skip!();
    let shops: HashSet<(i32, i32)> = resolved(s, OPEN_SHOP)
        .iter()
        .filter_map(|r| Some((r.args.first().copied()??, r.args.get(1).copied()??)))
        .collect();
    let (mut menus, mut reached) = (0, HashSet::new());
    for (_, name, esd) in &s.talk {
        let found = shop_menus(esd);
        menus += found.len();
        let mut changed = esd.clone();
        for m in &found {
            reached.extend(m.ranges.iter().copied());
            // Whether the machine comes back to empty the menu from some state
            // past the menu itself, as the gift entry's state will.
            let g = &esd.groups[m.group];
            let back = esd.states_of(g).iter().any(|st| {
                let i = esd.index_of(st).unwrap();
                let mut todo = esd.conditions_in(st.conditions);
                let mut to_clear = false;
                while let Some(c) = todo.pop() {
                    to_clear |= esd.state_at(c.next_state).map(|(j, _)| j) == Some(m.clear);
                    todo.extend(esd.conditions_in(c.subconditions));
                }
                ![m.clear, m.show, m.choose].contains(&i)
                    && to_clear
                    && esd.states_of(g).first().and_then(|f| esd.index_of(f)) != Some(i)
            });
            assert!(
                back,
                "{name} machine {:#x} never empties its menu again",
                g.id
            );
            let gift = Gift {
                text: 1,
                lot: 2,
                flag: 3,
                hidden_by: vec![4],
            };
            add_gift(&mut changed, m.group, &gift).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        let back = fs_esd::Esd::read(&changed.write().unwrap()).unwrap();
        assert_eq!(back, changed, "{name}");
        assert_eq!(back.write().unwrap(), changed.write().unwrap());
    }
    eprintln!(
        "{menus} shop menus open {} of {} ranges",
        reached.len(),
        shops.len()
    );
    // 70 since menus shown with a count are read too (82271d1).
    assert_eq!(menus, 70);
    // Every range of the named merchants (100000 to 100499, and the
    // expansion's 102000s) is opened from some menu. On 2.7.2, 7 of 48 are not:
    // a nomadic merchant's 100550 to 100579, and lists in the 122000s to
    // 129000s and 600000s.
    let missed: Vec<_> = shops.difference(&reached).collect();
    assert!(
        missed
            .iter()
            .all(|r| !(100_000..100_500).contains(&r.0) && !(102_000..103_000).contains(&r.0)),
        "{missed:?}"
    );
}

#[test]
fn the_husks_take_bell_bearings_for_flags_that_open_shops() {
    let s = sources_or_skip!();
    let husks = &s.talk.iter().find(|(_, n, _)| n == "t600001110").unwrap().2;
    let offers = fs_esd::choice::offers(husks);
    let flag_of = |goods: i32| {
        offers
            .iter()
            .find(|h| h.goods == goods)
            .map(|h| h.flags.iter().copied().collect::<Vec<_>>())
    };
    assert_eq!(flag_of(8913), Some(vec![11_109_713]), "Sellen's");
    assert_eq!(flag_of(2_008_901), Some(vec![11_109_791]), "Ymir's");
    // The plain reading, which the model's handovers use, does not see them.
    assert!(fs_esd::choice::handovers(husks).is_empty());
    let ranges = fs_esd::menu::ranges_by_flag(husks);
    assert_eq!(ranges[&11_109_713].iter().next(), Some(&(100_050, 100_074)));
    assert_eq!(ranges[&11_109_790].iter().next(), Some(&(102_250, 102_289)));
    // 36 since the husks' menu shown with a count is read too (82271d1).
    assert_eq!(ranges.len(), 36);
}

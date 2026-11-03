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

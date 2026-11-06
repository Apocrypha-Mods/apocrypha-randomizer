//! Placement on the player's own game (`APOC_ER_STOCK`; skipped when unset).

use std::sync::OnceLock;

use gamedata::{Model, Sources, elden_ring};
use logic::{Bound, Graph};

fn sources() -> Option<&'static (Sources, Model)> {
    static S: OnceLock<Option<(Sources, Model)>> = OnceLock::new();
    S.get_or_init(|| {
        let dir = std::env::var_os("APOC_ER_STOCK")?;
        let s = Sources::open(std::path::Path::new(&dir)).unwrap();
        let m = elden_ring::extract(&s).unwrap();
        Some((s, m))
    })
    .as_ref()
}

fn graph() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(logic::elden_ring)
}

macro_rules! or_skip {
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

/// Every quest item some slot gives lands where the walk gets it for sure,
/// in a seed with everything moving, and `check` says so.
#[test]
fn quest_items_can_be_counted_on() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    assert!(m.quest_items.len() > 90, "{}", m.quest_items.len());
    let name = |n: &str| m.quest_items.iter().any(|i| i.name.as_deref() == Some(n));
    for n in ["Irina's Letter", "Rya's Necklace", "Seluvis's Potion"] {
        assert!(name(n), "{n} is not a quest item");
    }
    let keys = b.key_items();
    for seed in 0..20 {
        let plan = items::shuffle(&b, &units, seed, &items::Options::default()).unwrap();
        items::check(&b, &units, &plan).unwrap();
        let placed: Vec<_> = plan.placed.iter().map(|&p| Some(p)).collect();
        let walk = b.walk(&items::gives(&b, &units, &placed, &keys));
        let letter = (0..units.len()).find(|&u| {
            units[plan.placed[u]].item.name.as_deref() == Some("Irina's Letter")
                && units[u].sure()
                && walk.taken[units[u].location]
                && !b.missable(units[u].location)
                && !b.past_no_return(units[u].location)
        });
        assert!(letter.is_some(), "seed {seed}: Irina's Letter is not safe");
    }
}

#[test]
fn seeds_place_every_item_and_can_be_finished() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    assert!(units.len() > 4000, "{} slots", units.len());
    for seed in 0..20 {
        let plan = items::shuffle(&b, &units, seed, &items::Options::default()).unwrap();
        items::check(&b, &units, &plan).unwrap();
        // A gift or shop row that may be missed never holds a key item that
        // moved there.
        for (u, unit) in units.iter().enumerate() {
            let from = &units[plan.placed[u]];
            assert!(
                !b.missable(unit.location)
                    || from.category != items::Category::Key
                    || plan.placed[u] == u,
                "seed {seed}: {:?} ({:?}, from {:?}) on a missable slot {:?} {:?}",
                from.item.name,
                from.category,
                m.locations[from.location].source,
                m.locations[unit.location].source,
                unit.place
            );
        }
    }
}

#[test]
fn a_plan_is_written_into_the_item_lots() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let plan = items::shuffle(&b, &units, 3, &items::Options::default()).unwrap();
    let mut reg = fs_param::Regulation::open(std::path::Path::new(
        &std::env::var_os("APOC_ER_STOCK").unwrap(),
    ))
    .unwrap();
    items::apply(&mut reg, &units, &plan).unwrap();
    let layout = fs_param::layout::for_table("ItemLotParam_map").unwrap();
    let table = reg.table("ItemLotParam_map").unwrap();
    let mut moved = 0;
    for (u, unit) in units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.table == "ItemLotParam_map")
    {
        let row = table.rows.iter().find(|r| r.id == unit.row).unwrap();
        let now = items::item_at(&units, &plan, u);
        assert_eq!(
            layout.field("item_id").get(&row.data, unit.slot),
            i64::from(now.id)
        );
        moved += usize::from(now.id != unit.item.id);
    }
    assert!(moved > 1000, "only {moved} slots changed");
    // The changed tables pack back into a whole regulation file.
    let bytes = reg.write().unwrap();
    assert!(bytes.len() > 1_000_000);
}

#[test]
fn key_bias_puts_key_items_deeper() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let depth = items::depth::depths(&b);
    let mean_key_depth = |bias: &str| {
        let mut o = items::Options::default();
        o.set(true, bias).unwrap();
        let (mut sum, mut n) = (0.0, 0);
        for seed in 0..10 {
            let plan = items::shuffle(&b, &units, seed, &o).unwrap();
            for (u, unit) in units.iter().enumerate() {
                if units[plan.placed[u]].category == items::Category::Key {
                    sum += depth[unit.location];
                    n += 1;
                }
            }
        }
        sum / f64::from(n)
    };
    let (flat, deep) = (mean_key_depth("keys=0"), mean_key_depth("keys=100"));
    eprintln!("mean key-item depth: bias 0 {flat:.3}, bias 100 {deep:.3}");
    assert!(deep > flat + 0.1);
}

#[test]
fn every_category_has_items() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    for c in items::Category::ITEMS {
        let n = units.iter().filter(|u| u.category == c).count();
        eprintln!("{:<13} {n}", c.name());
        assert!(n > 0, "no {} in the shuffled slots", c.name());
    }
}

#[test]
fn every_item_lands_in_a_place_that_takes_it() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let count = |p: fn(&items::Place) -> bool| units.iter().filter(|u| p(&u.place)).count();
    let shops = count(|p| *p == items::Place::Shop);
    let starts = count(|p| matches!(p, items::Place::Start(_)));
    eprintln!("{shops} shop rows, {starts} starting slots");
    assert!(shops > 500 && starts > 50);
    for seed in 0..10 {
        let plan = items::shuffle(&b, &units, seed, &items::Options::default()).unwrap();
        for (u, unit) in units.iter().enumerate() {
            assert!(
                items::fits(unit, &units[plan.placed[u]]),
                "seed {seed}: {:?} {} {} given item {}",
                unit.place,
                unit.row,
                unit.slot,
                units[plan.placed[u]].item.id
            );
        }
    }
}

#[test]
fn shops_and_classes_are_written() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let plan = items::shuffle(&b, &units, 11, &items::Options::default()).unwrap();
    let mut reg = fs_param::Regulation::open(std::path::Path::new(
        &std::env::var_os("APOC_ER_STOCK").unwrap(),
    ))
    .unwrap();
    items::apply(&mut reg, &units, &plan).unwrap();
    let shop = fs_param::layout::for_table("ShopLineupParam").unwrap();
    let init = fs_param::layout::for_table("CharaInitParam").unwrap();
    let (shops, classes) = (
        reg.table("ShopLineupParam").unwrap(),
        reg.table("CharaInitParam").unwrap(),
    );
    for (u, unit) in units.iter().enumerate() {
        let now = items::item_at(&units, &plan, u);
        let got = match unit.place {
            items::Place::Shop => {
                let r = shops.rows.iter().find(|r| r.id == unit.row).unwrap();
                shop.field("equip_id").get(&r.data, 0)
            }
            items::Place::Start(field) => {
                let r = classes.rows.iter().find(|r| r.id == unit.row).unwrap();
                init.field(field).get(&r.data, unit.slot)
            }
            items::Place::Lot | items::Place::Gather => continue,
        };
        assert_eq!(got, i64::from(now.id), "{:?} {}", unit.place, unit.row);
    }
}

#[test]
fn separate_keeps_base_game_and_dlc_apart() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let dlc = |u: usize| items::in_dlc(m, units[u].location);
    let dlc_slots = (0..units.len()).filter(|&u| dlc(u)).count();
    eprintln!("{dlc_slots} of {} slots in the DLC", units.len());
    assert!(dlc_slots > 500);
    let mut o = items::Options::default();
    o.dlc_separate = true;
    for seed in 0..10 {
        let plan = items::shuffle(&b, &units, seed, &o).unwrap();
        for u in 0..units.len() {
            assert_eq!(
                dlc(u),
                dlc(plan.placed[u]),
                "seed {seed}: slot {u} crossed sides"
            );
        }
    }
    // Mixed, items do cross.
    let plan = items::shuffle(&b, &units, 0, &items::Options::default()).unwrap();
    assert!((0..units.len()).any(|u| dlc(u) != dlc(plan.placed[u])));
}

/// The chapel's door asks for the finger, and only the body before it can
/// hold it: seen in play, seed 1 once shut the player in.
#[test]
fn the_finger_stays_before_the_chapel_door() {
    let Some((sources, model)) = sources() else {
        return;
    };
    let bound = Bound::new(graph(), model);
    let units = items::units(model, &bound, &sources.reg).unwrap();
    let finger = units
        .iter()
        .position(|u| u.item.name.as_deref() == Some("Tarnished's Wizened Finger"))
        .unwrap();
    for seed in 0..20 {
        let plan = items::shuffle(&bound, &units, seed, &items::Options::default()).unwrap();
        assert_eq!(plan.placed[finger], finger, "seed {seed}");
    }
}

/// Nova's choices from play: these stay at any slider.
#[test]
fn what_is_fixed_stays() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let plan = items::shuffle(&b, &units, 1, &items::Options::default()).unwrap();
    for name in [
        "Flask of Crimson Tears",
        "Flask of Cerulean Tears",
        "Flask of Wondrous Physick",
        "Spectral Steed Whistle",
        "Crafting Kit",
    ] {
        let at: Vec<usize> = (0..units.len())
            .filter(|&u| units[u].item.name.as_deref() == Some(name))
            .collect();
        assert!(!at.is_empty(), "{name}");
        for u in at {
            assert_eq!(plan.placed[u], u, "{name}");
        }
    }
}

#[test]
fn keepsakes_move_and_are_named_for_the_menu() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let keep: Vec<usize> = (0..units.len())
        .filter(|&u| units[u].slider() == items::Category::Keepsake)
        .collect();
    assert_eq!(keep.len(), 9);
    let pot = keep
        .iter()
        .find(|&&u| units[u].item.name.as_deref() == Some("Cracked Pot"))
        .unwrap();
    assert_eq!(units[*pot].item.count, 3);
    let plan = items::shuffle(&b, &units, 1, &items::Options::default()).unwrap();
    assert!(keep.iter().any(|&u| plan.placed[u] != u));
    assert_eq!(items::keepsake_names(&units, &plan).len(), 9);
    let mut still = items::Options::default();
    still.set(false, "keepsakes=0").unwrap();
    let plan = items::shuffle(&b, &units, 1, &still).unwrap();
    assert!(keep.iter().all(|&u| plan.placed[u] == u));
}

#[test]
fn nodes_trade_only_among_themselves() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let gather = |u: usize| units[u].place == items::Place::Gather;
    assert!((0..units.len()).filter(|&u| gather(u)).count() > 50);
    let plan = items::shuffle(&b, &units, 1, &items::Options::default()).unwrap();
    assert!((0..units.len()).any(|u| gather(u) && plan.placed[u] != u));
    for u in 0..units.len() {
        assert_eq!(gather(u), gather(plan.placed[u]), "{u}");
    }
    let mut still = items::Options::default();
    still.set(false, "gathering=0").unwrap();
    let plan = items::shuffle(&b, &units, 1, &still).unwrap();
    assert!((0..units.len()).all(|u| !gather(u) || plan.placed[u] == u));
}

#[test]
fn npcs_are_dressed_from_the_seed() {
    let (_, m) = or_skip!();
    let game = std::path::PathBuf::from(std::env::var_os("APOC_ER_STOCK").unwrap());
    let run = |o: &items::Options| {
        let mut reg = fs_param::Regulation::open(&game).unwrap();
        let lines = items::dress_npcs(&mut reg, m, 1, o).unwrap();
        (lines, reg.table("CharaInitParam").unwrap())
    };
    let (a, table) = run(&items::Options::default());
    assert!(a.len() > 100, "{}", a.len());
    assert_eq!(a, run(&items::Options::default()).0);
    let init = fs_param::layout::for_table("CharaInitParam").unwrap();
    // A class is nobody in the maps' row, so it is left alone.
    let vagabond = table.rows.iter().find(|r| r.id == 3000).unwrap();
    assert_eq!(init.field("weapon").get(&vagabond.data, 0), 2_000_000);
    let mut still = items::Options::default();
    still.set(false, "npc-gear=0").unwrap();
    assert!(run(&still).0.is_empty());
}

/// Key and quest items go nowhere they can be lost: an area that can shut, an
/// unlimited or dear shop row, or, while enemies move, an enemy's drop.
#[test]
fn what_must_be_had_lands_where_it_stays_within_reach() {
    let Some((s, model)) = sources() else { return };
    let graph = logic::elden_ring();
    let bound = Bound::new(&graph, model);
    let us = items::units(model, &bound, &s.reg).unwrap();
    let mut options = items::Options::default();
    options.enemies_move = true;
    let mut one_off_on_shy = 0;
    for seed in 0..10 {
        let plan = items::shuffle(&bound, &us, seed, &options).unwrap();
        for (u, unit) in us.iter().enumerate() {
            let from = &us[plan.placed[u]];
            if plan.placed[u] != u && from.category == items::Category::Key {
                assert!(!bound.may_close(unit.location), "{:?}", from.item.name);
                assert!(
                    !unit.unlimited && unit.price <= 3000,
                    "{:?}",
                    from.item.name
                );
                assert_ne!(
                    model.locations[unit.location].kind,
                    gamedata::model::Kind::EnemyDrop
                );
            }
            let shy = unit.unlimited || (unit.place == items::Place::Lot && !unit.sure());
            let plentiful = matches!(
                from.category,
                items::Category::Consumable | items::Category::Upgrade
            );
            if plan.placed[u] != u && shy && !plentiful {
                one_off_on_shy += 1;
                eprintln!(
                    "seed {seed}: {:?} on {} row {}",
                    from.item.name, unit.table, unit.row
                );
            }
        }
    }
    assert_eq!(one_off_on_shy, 0);
}

/// A category at 0% stays put in shops too, whatever the merchants' slider says.
#[test]
fn a_category_at_none_stays_in_the_shops() {
    let Some((s, model)) = sources() else { return };
    let graph = logic::elden_ring();
    let bound = Bound::new(&graph, model);
    let us = items::units(model, &bound, &s.reg).unwrap();
    let mut options = items::Options::default();
    options.set(false, "weapons=0,merchants=100").unwrap();
    let plan = items::shuffle(&bound, &us, 3, &options).unwrap();
    for (u, unit) in us.iter().enumerate() {
        if unit.place == items::Place::Shop && unit.category == items::Category::Weapon {
            assert_eq!(plan.placed[u], u, "row {}", unit.row);
        }
    }
    assert!((0..us.len()).any(|u| us[u].place == items::Place::Shop && plan.placed[u] != u));
}

/// A weapon at 0% moves nowhere: not from a shop, a class's hands, a node or an NPC,
/// whatever the places' own sliders say (Nova, 2026-11-03).
#[test]
fn weapons_at_nothing_move_nowhere() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let mut o = items::Options::default();
    o.set(false, "weapons=0").unwrap();
    let weapon = |u: &items::Unit| u.category == items::Category::Weapon;
    let shops = units
        .iter()
        .filter(|u| u.place == items::Place::Shop && weapon(u))
        .count();
    let classes = units
        .iter()
        .filter(|u| matches!(u.place, items::Place::Start(_)) && weapon(u))
        .count();
    // There are weapons in each kind of place to hold still.
    assert!(shops > 50 && classes > 10, "{shops} {classes}");
    let weapons: std::collections::HashSet<String> = units
        .iter()
        .filter(|u| weapon(u))
        .filter_map(|u| u.item.name.clone())
        .collect();
    for seed in 0..5 {
        let plan = items::shuffle(&b, &units, seed, &o).unwrap();
        items::check(&b, &units, &plan).unwrap();
        for (u, unit) in units.iter().enumerate() {
            let from = plan.placed[u];
            assert!(
                from == u || (!weapon(unit) && !weapon(&units[from])),
                "seed {seed}: {:?} {} {} took {:?} (was {:?})",
                unit.place,
                unit.table,
                unit.row,
                units[from].item.name,
                unit.item.name
            );
        }
        let mut reg = s.reg.clone();
        let dressed = items::dress_npcs(&mut reg, m, seed, &o).unwrap();
        assert!(!dressed.is_empty(), "armour still changes");
        for line in &dressed {
            let now = line
                .split(": ")
                .nth(1)
                .unwrap()
                .split(" (was")
                .next()
                .unwrap();
            assert!(
                !weapons.contains(now),
                "seed {seed}: an NPC took a weapon: {line}"
            );
        }
    }
}

/// No one-off item that moves lands on a slot that gives by chance, at every
/// setting tried, and the seeds still place; the counts each side has.
#[test]
fn one_off_items_keep_off_chance_slots() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let not_node = |u: &&items::Unit| u.place != items::Place::Gather;
    let chance = units.iter().filter(|u| items::chance_slot(u)).count();
    let plentiful = units
        .iter()
        .filter(not_node)
        .filter(|u| !items::one_off(u.category))
        .count();
    let one_offs = units
        .iter()
        .filter(not_node)
        .filter(|u| items::one_off(u.category))
        .count();
    let sure = units
        .iter()
        .filter(not_node)
        .filter(|u| !items::chance_slot(u))
        .count();
    let one_off_on_chance = units
        .iter()
        .filter(|u| items::chance_slot(u) && items::one_off(u.category))
        .count();
    eprintln!(
        "slots {}: chance {chance} (holding a one-off {one_off_on_chance}), sure {sure}; items: plentiful {plentiful}, one-off {one_offs}",
        units.len()
    );
    let unlimited: Vec<usize> = (0..units.len()).filter(|&u| units[u].unlimited).collect();
    let plan = items::shuffle(&b, &units, 0, &items::Options::default()).unwrap();
    let multiplied = unlimited
        .iter()
        .filter(|&&u| plan.placed[u] != u && items::one_off(units[plan.placed[u]].category))
        .count();
    eprintln!(
        "rows that never run out: {}; seed 0 puts a one-off item on {multiplied}",
        unlimited.len()
    );
    // On 2.7.2 every shuffled lot's slot holds its row's whole weight: drops by
    // chance are farmable ones, which stay. The rule guards a game or mod that has some.
    assert!(plentiful >= chance, "{chance} {plentiful}");
    for spec in ["", "consumables=0,upgrades=0", "consumables=10", "all=50"] {
        for separate in [false, true] {
            let mut o = items::Options::default();
            o.set(false, spec).unwrap();
            o.dlc_separate = separate;
            for seed in 0..3 {
                let plan = items::shuffle(&b, &units, seed, &o)
                    .unwrap_or_else(|e| panic!("{spec} {separate}: {e}"));
                items::check(&b, &units, &plan).unwrap();
                for (u, unit) in units.iter().enumerate() {
                    let from = plan.placed[u];
                    assert!(
                        from == u
                            || !items::chance_slot(unit)
                            || !items::one_off(units[from].category),
                        "{spec}: seed {seed}: {:?} on a chance slot",
                        units[from].item.name
                    );
                }
            }
        }
    }
}

/// The game's own classes can use what they start with, by the same reading;
/// with usable starting gear every class still can after the shuffle, and
/// without it, some cannot.
#[test]
fn usable_starting_gear_suits_each_class() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let classes = units.iter().filter(|u| u.class.is_some()).count();
    assert!(classes > 40, "{classes} class slots");
    // The Astrologer's staff casts sorceries, her spells take one memory slot each.
    let staff = units
        .iter()
        .find(|u| u.class.is_some() && u.item.id == 33_130_000)
        .unwrap();
    assert_eq!(staff.wants.casts, Some(items::School::Sorcery));
    assert_eq!(staff.wants.stats[2], 16);
    let vanilla = items::Plan {
        seed: 0,
        options: items::Options::default(),
        placed: (0..units.len()).collect(),
    };
    items::usable_start(&units, &vanilla).unwrap();

    let mut on = items::Options::default();
    on.usable_start = true;
    let mut unusable = 0;
    for seed in 0..20 {
        let plan = items::shuffle(&b, &units, seed, &on).unwrap();
        items::usable_start(&units, &plan).unwrap();
        let random = items::shuffle(&b, &units, seed, &items::Options::default()).unwrap();
        unusable += usize::from(items::usable_start(&units, &random).is_err());
    }
    assert!(
        unusable > 10,
        "only {unusable} of 20 random seeds had unusable gear"
    );
}

/// The game's own classes start at a medium roll but for the Vagabond (38.8 of 49.8,
/// a heavy roll); with light starting gear every class does after the shuffle, and
/// without it, some do not.
#[test]
fn light_starting_gear_keeps_a_medium_roll() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    // The Vagabond's armour weighs as the menus say, and the Longsword 3.5.
    let armour = units
        .iter()
        .find(|u| u.class.is_some() && u.item.id == 660_100)
        .unwrap();
    assert_eq!(armour.wants.weight, 106);
    let sword = units
        .iter()
        .find(|u| u.class.is_some() && u.item.id == 2_000_000)
        .unwrap();
    assert_eq!(sword.wants.weight, 35);
    // The Vagabond's endurance of 11 gives 49.7 equip load, 34.8 of it for a medium roll.
    assert_eq!(items::equip_load(11), 497);
    let mut vanilla = items::Plan {
        seed: 0,
        options: items::Options::default(),
        placed: (0..units.len()).collect(),
    };
    vanilla.options.light_start = true;
    let err = items::usable_start(&units, &vanilla).unwrap_err();
    assert!(err.starts_with("class 3000 carries 38.8"), "{err}");

    let mut on = items::Options::default();
    on.usable_start = true;
    on.light_start = true;
    let mut usable = on.clone();
    usable.light_start = false;
    let mut heavy = 0;
    for seed in 0..20 {
        let plan = items::shuffle(&b, &units, seed, &on).unwrap();
        items::usable_start(&units, &plan).unwrap();
        let mut loose = items::shuffle(&b, &units, seed, &usable).unwrap();
        loose.options.light_start = true;
        heavy += usize::from(items::usable_start(&units, &loose).is_err());
    }
    assert!(heavy > 5, "only {heavy} of 20 seeds started heavy");
}

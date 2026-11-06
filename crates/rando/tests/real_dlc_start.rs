//! DLC Start on the player's own game (`APOC_ER_STOCK`; skipped when unset): our flag,
//! event and lots are free, the warp is the arm's own, and a seed writes them.

use std::path::PathBuf;

use fs_emevd::Instruction;
use gamedata::Sources;
use rando::dlc_start::{self, ARM_GRACE, EVENT, Extras, LANDING, LOT, MAP, STARTED};

fn words(i: &Instruction) -> Vec<u32> {
    i.args
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn game() -> Option<Sources> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    Some(Sources::open(&dir).unwrap())
}

#[test]
fn the_warp_is_the_arms_and_ours_are_free() {
    let Some(s) = game() else { return };
    let (_, arm) = s
        .scripts
        .iter()
        .find(|(m, _)| *m == gamedata::MapId([12, 5, 0, 0]))
        .unwrap();
    let ev = arm.events.iter().find(|e| e.id == 12_052_500).unwrap();
    let warp = ev
        .instructions
        .iter()
        .find(|i| (i.bank, i.id) == (2003, 14))
        .unwrap();
    assert_eq!(words(warp), [u32::from_le_bytes(MAP), LANDING, ARM_GRACE]);
    let respawn = ev
        .instructions
        .iter()
        .find(|i| (i.bank, i.id) == (2003, 23))
        .unwrap();
    assert_eq!(words(respawn), [LANDING]);

    for e in s
        .scripts
        .iter()
        .map(|(_, e)| e)
        .chain([&s.common, &s.common_func])
    {
        for ev in &e.events {
            assert_ne!(ev.id, EVENT, "event {EVENT} is the game's");
            for i in &ev.instructions {
                assert!(
                    !words(i).contains(&STARTED),
                    "event {} holds {STARTED}",
                    ev.id
                );
            }
        }
    }
    for (_, name, esd) in &s.talk {
        assert!(
            !esd.expressions
                .windows(4)
                .any(|w| w == STARTED.to_le_bytes()),
            "{name}"
        );
    }
    for layout in fs_param::layout::elden_ring() {
        for field in layout.fields.iter().filter(|f| f.name.contains("flag")) {
            for table in &layout.tables {
                let Ok(t) = s.reg.table(table) else { continue };
                for r in &t.rows {
                    for k in 0..field.count {
                        assert_ne!(
                            field.get(&r.data, k),
                            i64::from(STARTED),
                            "{table} {}",
                            r.id
                        );
                    }
                }
            }
        }
    }
    let mut reg = s.reg.clone();
    dlc_start::lots(&mut reg, &dlc_start::Extras::default()).unwrap();
    let lots = reg.table("ItemLotParam_map").unwrap();
    let ours: Vec<i32> = lots
        .rows
        .iter()
        .map(|r| r.id)
        .filter(|&id| (LOT..LOT + 10).contains(&id))
        .collect();
    assert_eq!(ours, [LOT, LOT + 1]);
    let after = dlc_start::apply(&s.common, &dlc_start::Extras::default()).unwrap();
    let zero = after.events.iter().find(|e| e.id == 0).unwrap();
    assert_eq!(words(&zero.instructions[0])[1], EVENT as u32);
    assert_eq!(
        fs_emevd::Emevd::read(&after.write().unwrap()).unwrap(),
        after
    );
}

/// The extras' lots are free, the goods given are what their names say, the grace's
/// menu takes the shop's entry, the shop holds the base game's gear, and the entry's
/// text is free in every language.
#[test]
fn the_extras_fit_the_game() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let files = gamedata::GameFiles::open(&dir, &[]).unwrap();
    let s = Sources::open_with(&dir, &files).unwrap();
    let all = Extras {
        level: 200,
        flasks: true,
        smithing: true,
        shop: true,
    };
    let mut reg = s.reg.clone();
    dlc_start::lots(&mut reg, &all).unwrap();

    let text = gamedata::sources::text_of(&files.read("/msg/engus/item_dlc02.msgbnd.dcx").unwrap())
        .unwrap();
    let goods = &text["GoodsName"];
    for (id, _, name) in dlc_start::FLASKS {
        assert_eq!(goods.get(&id).map(String::as_str), Some(name), "{id}");
    }
    for (id, name) in dlc_start::SMITHING {
        assert_eq!(goods.get(&id).map(String::as_str), Some(name), "{id}");
    }
    assert_eq!(
        goods.get(&dlc_start::LORDS_RUNE).map(String::as_str),
        Some("Lord's Rune")
    );

    let (_, _, grace) = s
        .talk
        .iter()
        .find(|(_, n, _)| n == dlc_start::GRACE_SCRIPT)
        .unwrap();
    let edited = dlc_start::grace_script(grace, dlc_start::SHOP_FIRST + 10).unwrap();
    let again = fs_esd::Esd::read(&edited.write().unwrap()).unwrap();
    assert_eq!(again.states.len(), grace.states.len() + 2);

    let model = gamedata::elden_ring::extract(&s).unwrap();
    let graph = rando::graph_for(&items::Options::default()).unwrap();
    let bound = logic::Bound::new(&graph, &model);
    let units = items::units(&model, &bound, &s.reg).unwrap();
    let stock = dlc_start::base_equipment(&model, &units, &s.reg).unwrap();
    let counts = dlc_start::stock_counts(&stock);
    eprintln!("{counts:?}");
    assert!(
        counts["Weapon"] > 200 && counts["Protector"] > 300,
        "{counts:?}"
    );
    // The Longsword, plain.
    assert!(stock.contains(&(gamedata::model::ItemKind::Weapon, 2_000_000)));
    let last = dlc_start::shop_rows(&mut reg, &stock).unwrap();
    assert_eq!(last, dlc_start::SHOP_FIRST + stock.len() as i32 - 1);

    for language in rando::LANGUAGES {
        let path = format!("/msg/{language}/menu_dlc02.msgbnd.dcx");
        let (_, plain) = fs_dcx::decompress(&files.read(&path).unwrap()).unwrap();
        let bnd = fs_bnd4::Bnd4::read(&plain).unwrap();
        let talk = bnd
            .files
            .iter()
            .find(|f| f.short_name() == Some("EventTextForTalk.fmg"))
            .unwrap();
        let fmg = fs_fmg::Fmg::read(&talk.data).unwrap();
        assert!(
            !fmg.entries.contains_key(&dlc_start::SHOP_TEXT),
            "{language}"
        );
    }
}

//! Night-only bosses at any hour, on the player's own game (`APOC_ER_STOCK`; skipped
//! when unset): effect 11800 is on Night's Cavalry and their horses alone, and the
//! option clears it from them and from nothing else.

use std::path::PathBuf;

use fs_param::layout::for_table;
use gamedata::Sources;
use rando::night::{self, NIGHT_EFFECT};

#[test]
fn only_night_cavalry_and_their_horses_lose_the_effect() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        eprintln!("APOC_ER_STOCK is not set; skipping");
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let f = for_table("NpcParam").unwrap().field("effect_c");
    let carrying: Vec<i32> = s
        .reg
        .table("NpcParam")
        .unwrap()
        .rows
        .iter()
        .filter(|r| f.get(&r.data, 0) == NIGHT_EFFECT)
        .map(|r| r.id)
        .collect();
    assert_eq!(carrying.len(), 23, "{carrying:?}");
    assert!(
        carrying.iter().all(
            |id| (31_500_000..31_510_000).contains(id) || (31_600_000..31_610_000).contains(id)
        ),
        "{carrying:?}"
    );
    let mut reg = s.reg.clone();
    assert_eq!(night::apply(&mut reg).unwrap(), 23);
    let (before, after) = (
        s.reg.table("NpcParam").unwrap(),
        reg.table("NpcParam").unwrap(),
    );
    for (b, a) in before.rows.iter().zip(&after.rows) {
        if carrying.contains(&b.id) {
            assert_eq!(f.get(&a.data, 0), 0);
            let mut b2 = b.data.clone();
            f.set(&mut b2, 0, 0).unwrap();
            assert_eq!(a.data, b2, "row {} changes only there", b.id);
        } else {
            assert_eq!(a.data, b.data, "row {}", b.id);
        }
    }
}

/// The scripts: the four shacks whose hunter waits on the hour (m60_37_46, Elleh's
/// m60_42_38, m60_43_53, m60_48_41) and the nine maps where Night's Cavalry ride,
/// whose scripts mark them by day, change, and nothing else. Each shack's event that lets its hunter out
/// now holds from 00:00:00 to 23:59:59, every other hour test in the game is as it
/// was, and no script gives the day's mark.
#[test]
fn the_shacks_and_the_bridge_keep_no_hours() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let hours = |script: &fs_emevd::Emevd| -> Vec<(i64, Vec<u8>)> {
        script
            .events
            .iter()
            .flat_map(|e| e.instructions.iter().map(move |i| (e.id, i)))
            .filter(|(_, i)| (i.bank, i.id) == night::HOURS)
            .map(|(id, i)| (id, i.args.clone()))
            .collect()
    };
    let mut changed = Vec::new();
    for (m, script) in &s.scripts {
        let mut out = script.clone();
        if !night::edit_script(&mut out) {
            assert_eq!(&out, script);
            continue;
        }
        changed.push(m.to_string());
        let mut again = out.clone();
        assert!(
            !night::edit_script(&mut again),
            "{m}: a second edit changes nothing"
        );
        for ((id, before), (_, after)) in hours(script).iter().zip(hours(&out)) {
            if before != &after {
                assert_eq!(&after[1..7], &[0, 0, 0, 23, 59, 59], "{m} event {id}");
                // Only the night: 20:00 to 05:30.
                assert_eq!(&before[1..7], &[20, 0, 0, 5, 30, 0], "{m} event {id}");
            }
        }
        let marks = |e: &fs_emevd::Emevd| {
            e.events
                .iter()
                .flat_map(|e| &e.instructions)
                .filter(|i| {
                    (i.bank, i.id) == (2004, 8) && i.args[4..8] == night::DAY_MARK.to_le_bytes()
                })
                .count()
        };
        assert_eq!(marks(&out), 0, "{m}");
    }
    changed.sort();
    assert_eq!(
        changed,
        [
            "m60_36_48_00", // Cavalry
            "m60_37_46_00", // shack
            "m60_39_43_00", // Cavalry
            "m60_39_51_00", // Cavalry
            "m60_42_38_00", // Elleh's shack
            "m60_43_37_00", // Cavalry
            "m60_43_53_00", // shack
            "m60_44_32_00", // Cavalry
            "m60_48_41_00", // shack
            "m60_48_51_00", // Cavalry
            "m60_48_55_00", // the Dragonbarrow pair
            "m60_49_37_00", // Cavalry
            "m60_52_41_00", // Cavalry
        ]
    );
}

/// The birds: 388 and 389 are on 17650 and 17651 alone, and only those two effects
/// change.
#[test]
fn only_the_birds_effects_lose_their_behaviour() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let f = for_table("SpEffectParam").unwrap().field("behaviour");
    let table = s.reg.table("SpEffectParam").unwrap();
    for (id, b) in night::BIRDS {
        let holding: Vec<i32> = table
            .rows
            .iter()
            .filter(|r| f.get(&r.data, 0) == b)
            .map(|r| r.id)
            .collect();
        assert_eq!(holding, [id]);
    }
    let mut reg = s.reg.clone();
    assert_eq!(night::birds(&mut reg).unwrap(), 2);
    let after = reg.table("SpEffectParam").unwrap();
    for (b, a) in table.rows.iter().zip(&after.rows) {
        if night::BIRDS.iter().any(|(id, _)| *id == b.id) {
            assert_eq!(f.get(&a.data, 0), 0);
        } else {
            assert_eq!(a.data, b.data, "effect {}", b.id);
        }
    }
}

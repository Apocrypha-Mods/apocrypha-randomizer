//! Torrent in the woods, music and enemy colours on the player's own game
//! (`APOC_ER_STOCK`; skipped when unset).

use std::collections::BTreeSet;
use std::path::PathBuf;

use fs_param::layout::for_table;
use gamedata::{Sources, elden_ring};
use rando::{colours, music, tweaks};

fn game() -> Option<Sources> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    Some(Sources::open(&dir).unwrap())
}

/// 433 is on the woods' effect and 190 alone, and only the woods' effect changes.
#[test]
fn torrent_is_let_into_the_woods_by_one_number() {
    let Some(s) = game() else { return };
    let f = for_table("SpEffectParam").unwrap().field("behaviour");
    let before = s.reg.table("SpEffectParam").unwrap();
    let holding: Vec<i32> = before
        .rows
        .iter()
        .filter(|r| f.get(&r.data, 0) == tweaks::NO_TORRENT)
        .map(|r| r.id)
        .collect();
    assert_eq!(holding, [190, tweaks::WOODS_EFFECT]);
    // Common event 1070 gives it, from event 0.
    let woods = s.common.events.iter().find(|e| e.id == 1070).unwrap();
    assert!(woods.instructions.iter().any(|i| {
        i.args
            .windows(4)
            .any(|w| w == tweaks::WOODS_EFFECT.to_le_bytes())
    }));
    let mut reg = s.reg.clone();
    tweaks::torrent_in_woods(&mut reg).unwrap();
    let after = reg.table("SpEffectParam").unwrap();
    for (b, a) in before.rows.iter().zip(&after.rows) {
        if b.id == tweaks::WOODS_EFFECT {
            assert_eq!(f.get(&a.data, 0), 0);
        } else {
            assert_eq!(a.data, b.data, "{}", b.id);
        }
    }
}

/// The themes the scripts play are rows of the table, Margit's among them, and the
/// shuffle keeps the same names; area music keeps the same numbers, the Roundtable
/// Hold's among them.
#[test]
fn music_changes_places_and_nothing_is_lost() {
    let Some(s) = game() else { return };
    let mut scripts = vec![&s.common, &s.common_func];
    scripts.extend(s.scripts.iter().map(|(_, e)| e));
    let played = music::played(&scripts);
    assert!(played.contains(&213_000), "Margit's c2130_A");
    let names = |reg: &fs_param::Regulation| -> Vec<Vec<u8>> {
        let mut v: Vec<Vec<u8>> = reg
            .table(music::NAMES)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.data[4..36].to_vec())
            .collect();
        v.sort();
        v
    };
    let mut reg = s.reg.clone();
    let n = music::shuffle_boss(&mut reg, &played, 7).unwrap();
    assert!((40..=80).contains(&n), "{n} themes");
    assert_eq!(names(&reg), names(&s.reg));
    assert_ne!(
        reg.table(music::NAMES).unwrap().rows,
        s.reg.table(music::NAMES).unwrap().rows
    );

    let map = music::shuffle_areas(&mut reg, 7).unwrap();
    assert!(map.contains_key(&111), "the Hold's Bgm_111_Hub");
    let from: BTreeSet<_> = map.keys().collect();
    let to: BTreeSet<_> = map.values().collect();
    assert_eq!(from, to);
    assert!(!map.contains_key(&999));
}

/// Ordinary enemies' rows take a colour, no boss's does, and our rows are free.
#[test]
fn ordinary_enemies_take_a_colour() {
    let Some(s) = game() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let rows = colours::rows_to_colour(&model);
    let mut reg = s.reg.clone();
    let coloured = colours::apply(&mut reg, &rows, 3).unwrap();
    assert!(coloured.len() > 300, "{}", coloured.len());
    // Margit's row is a boss's.
    let margit = model
        .enemies
        .iter()
        .find(|e| e.entity == Some(10_000_850))
        .unwrap()
        .npc_param;
    assert!(!coloured.contains_key(&margit));
    let last = for_table("NpcParam").unwrap().field("effect_last");
    let npc = reg.table("NpcParam").unwrap();
    for r in &npc.rows {
        if let Some(k) = coloured.get(&r.id) {
            assert_eq!(
                last.get(&r.data, 0),
                i64::from(colours::EFFECT_FIRST + *k as i32)
            );
        }
    }
    let phantom = reg.table("PhantomParam").unwrap();
    assert!(phantom.rows.iter().any(|r| r.id == colours::PHANTOM_FIRST));
}

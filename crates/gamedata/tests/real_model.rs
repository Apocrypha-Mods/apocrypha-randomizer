//! The model extracted from the player's own game (`APOC_ER_STOCK`; skipped
//! when unset). The floors are a little under build 25080141's counts, so a
//! change that loses locations fails, and the spot checks are facts anyone who
//! has played can confirm.

use std::sync::OnceLock;

use gamedata::model::{Kind, Location};
use gamedata::{MapId, Model, Sources, elden_ring};

fn model() -> Option<&'static Model> {
    static MODEL: OnceLock<Option<Model>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            let dir = std::env::var_os("APOC_ER_STOCK")?;
            let sources = Sources::open(std::path::Path::new(&dir)).unwrap();
            Some(elden_ring::extract(&sources).unwrap())
        })
        .as_ref()
}

macro_rules! model_or_skip {
    () => {
        match model() {
            Some(m) => m,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

fn of(m: &Model, kind: Kind) -> Vec<&Location> {
    m.locations.iter().filter(|l| l.kind == kind).collect()
}

fn giving<'a>(m: &'a Model, item: &str) -> Vec<&'a Location> {
    m.locations
        .iter()
        .filter(|l| l.items.iter().any(|i| i.name.as_deref() == Some(item)))
        .collect()
}

#[test]
fn every_kind_of_location_is_found() {
    let m = model_or_skip!();
    for (kind, floor) in [
        (Kind::Treasure, 3_400),
        (Kind::EnemyDrop, 120),
        (Kind::Event, 400),
        (Kind::BossReward, 180),
        (Kind::Shop, 1_200),
        (Kind::Starting, 10),
    ] {
        let n = of(m, kind).len();
        eprintln!("{}: {n}", kind.label());
        assert!(n >= floor, "{} {n} < {floor}", kind.label());
    }
}

#[test]
fn one_time_locations_carry_a_flag_and_a_map() {
    let m = model_or_skip!();
    for kind in [
        Kind::Treasure,
        Kind::EnemyDrop,
        Kind::Event,
        Kind::BossReward,
    ] {
        let ls = of(m, kind);
        let flagged = ls.iter().filter(|l| l.flag.is_some()).count();
        let mapped = ls.iter().filter(|l| l.map.is_some()).count();
        eprintln!(
            "{}: {} flagged {flagged} mapped {mapped}",
            kind.label(),
            ls.len()
        );
        assert!(flagged * 100 >= ls.len() * 99, "{}", kind.label());
        assert!(mapped * 100 >= ls.len() * 95, "{}", kind.label());
    }
}

#[test]
fn bosses_and_graces_are_placed_and_named() {
    let m = model_or_skip!();
    let placed = m.bosses.iter().filter(|b| b.map.is_some()).count();
    let named = m.bosses.iter().filter(|b| b.name.is_some()).count();
    assert!(
        placed >= 210 && named >= 210,
        "bosses placed {placed} named {named}"
    );
    let placed = m.graces.iter().filter(|g| g.map.is_some()).count();
    assert!(placed >= 415, "graces placed {placed}");
}

#[test]
fn known_rewards_are_where_they_belong() {
    let m = model_or_skip!();
    let stormveil = MapId([10, 0, 0, 0]);
    let leyndell = MapId([11, 0, 0, 0]);
    for (item, map) in [
        ("Godrick's Great Rune", stormveil),
        ("Morgott's Great Rune", leyndell),
    ] {
        let at: Vec<_> = giving(m, item)
            .into_iter()
            .filter(|l| l.kind == Kind::BossReward)
            .collect();
        assert!(
            at.iter().any(|l| l.map == Some(map)),
            "{item}: {:?}",
            at.iter().map(|l| (l.map, &l.source)).collect::<Vec<_>>()
        );
    }
    // A lot that runs on into the next row: the Great Rune and the
    // Remembrance come together.
    let godrick = giving(m, "Godrick's Great Rune")
        .into_iter()
        .find(|l| l.kind == Kind::BossReward)
        .unwrap();
    assert!(
        godrick
            .items
            .iter()
            .any(|i| i.name.as_deref() == Some("Remembrance of the Grafted"))
    );
    assert!(
        giving(m, "Golden Seed")
            .iter()
            .any(|l| l.kind == Kind::Treasure && l.map == Some(stormveil)),
        "a Golden Seed lies in Stormveil"
    );
}

#[test]
fn maps_are_named_by_their_graces() {
    let m = model_or_skip!();
    assert!(
        m.maps[&MapId([10, 0, 0, 0])]
            .iter()
            .any(|n| n == "Castleward Tunnel")
    );
    assert!(
        m.maps[&MapId([18, 0, 0, 0])]
            .iter()
            .any(|n| n == "Stranded Graveyard")
    );
}

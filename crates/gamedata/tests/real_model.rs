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
        (Kind::Gift, 100),
        (Kind::Gather, 2_000),
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
    // Gifts are left out: the grace menu's are given anywhere.
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

#[test]
fn melinas_gifts_come_from_the_grace_menu() {
    let m = model_or_skip!();
    for item in ["Spectral Steed Whistle", "Rold Medallion"] {
        let at = giving(m, item);
        assert!(
            at.iter()
                .any(|l| l.kind == Kind::Gift && l.map.is_none() && l.flag.is_some()),
            "{item}: {:?}",
            at.iter().map(|l| (l.kind, &l.source)).collect::<Vec<_>>()
        );
    }
}

/// What nothing gives, sorted. A lot numbered after a map that is read and
/// holding an item the model gets nowhere else would be a source not found
/// yet: there is one, the "O Mother" gesture's (2050450510, m61_50_45), and
/// every map such a lot is numbered after is read.
#[test]
fn what_nothing_gives_is_sorted() {
    use gamedata::model::LotSort;
    let m = model_or_skip!();
    let sorted = &m.gaps.flagged_named;
    let unknown = sorted
        .get(&LotSort::MapUnknown)
        .cloned()
        .unwrap_or_default();
    assert!(unknown.len() <= 1, "{unknown:?}");
    assert!(!sorted.contains_key(&LotSort::MapNotRead));
    // "About Sites of Grace", and the Confessor set's lot, looked for in play.
    assert!(sorted[&LotSort::Tutorial].contains(&50000));
    assert!(sorted[&LotSort::MapLeftover].contains(&1036430010));
    assert!(sorted[&LotSort::MapLeftover].len() > 100);
    assert!(m.gaps.treasure_without_lot > 100 && m.gaps.treasure_empty > 40);
    // A treasure with an empty lot has no slot.
    let empty = of(m, Kind::Treasure)
        .into_iter()
        .filter(|l| l.items.is_empty())
        .count();
    assert_eq!(empty, m.gaps.treasure_empty);
}

/// Shop rows whose on-sale flag events and talk turn on: Ymir sells the
/// Glintstone Nails once both bells have rung, and the Twin Maiden Husks
/// resell Black Flame's Protection only while its gift is untaken, so that row
/// may be missed.
#[test]
fn shop_flags_are_read_from_events_and_talk() {
    let m = model_or_skip!();
    let row = |item: &str| {
        of(m, Kind::Shop)
            .into_iter()
            .find(|l| {
                l.items[0].name.as_deref() == Some(item)
                    && l.terms.as_ref().is_some_and(|t| t.ways.is_some())
            })
            .unwrap_or_else(|| panic!("no read row for {item}"))
    };
    let nails = format!("{:?}", row("Glintstone Nails").terms);
    assert!(nails.contains("Hole-Laden Necklace"), "{nails}");
    assert!(!row("Glintstone Nails").missable);
    assert!(row("Black Flame's Protection").missable);
}

#[test]
fn plants_and_nodes_are_gathered() {
    let m = model_or_skip!();
    for item in ["Golden Sunflower", "Knot Resin", "Ghost Glovewort [2]"] {
        assert!(
            giving(m, item)
                .iter()
                .any(|l| l.kind == Kind::Gather && l.map.is_some() && l.flag.is_none()),
            "{item} grows somewhere"
        );
    }
}

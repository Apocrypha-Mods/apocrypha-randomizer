//! The authored graph checked against the player's own game (`APOC_ER_STOCK`;
//! skipped when unset): every name it uses is one the game has, and the
//! vanilla game, walked from the start, reaches every area and the goal.

use std::sync::OnceLock;

use gamedata::{Model, Sources, elden_ring};
use logic::{Bound, Graph};

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

fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(logic::elden_ring)
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

#[test]
fn every_name_in_the_graph_is_one_the_game_has() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    assert!(b.problems.is_empty(), "{:#?}", b.problems);
}

#[test]
fn the_vanilla_game_reaches_every_area_and_the_goal() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let w = b.walk(&b.vanilla());
    let missed: Vec<&str> = graph()
        .areas
        .iter()
        .zip(&w.reached)
        .filter(|(_, r)| !**r)
        .map(|(a, _)| a.id.as_str())
        .collect();
    assert!(missed.is_empty(), "never reached: {missed:?}");
    assert!(w.goal);
}

#[test]
fn a_gate_holds_without_its_key() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let without: Vec<Vec<&str>> = b
        .vanilla()
        .into_iter()
        .map(|g| {
            g.into_iter()
                .filter(|&i| i != "Academy Glintstone Key")
                .collect()
        })
        .collect();
    let w = b.walk(&without);
    let academy = graph()
        .areas
        .iter()
        .position(|a| a.id == "raya_lucaria")
        .unwrap();
    assert!(!w.reached[academy], "the academy opens with no key");
    assert!(w.goal, "the academy is not needed to finish");
}

#[test]
fn every_shop_row_has_a_merchant_or_is_ruled_out() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let loose = m
        .locations
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            l.kind == gamedata::model::Kind::Shop
                && !b.location_unused[*i]
                && b.location_area[*i].is_none()
        })
        .count();
    assert_eq!(loose, 0, "shop rows with no area");
}

#[test]
fn kale_sells_the_crafting_kit_in_limgrave() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let limgrave = graph()
        .areas
        .iter()
        .position(|a| a.id == "limgrave")
        .unwrap();
    let (i, _) = m
        .locations
        .iter()
        .enumerate()
        .find(|(_, l)| {
            l.kind == gamedata::model::Kind::Shop
                && l.items
                    .iter()
                    .any(|it| it.name.as_deref() == Some("Crafting Kit"))
                && l.also.len() + 1 > 1
        })
        .expect("a Crafting Kit sold in more than one place");
    assert_eq!(
        b.location_area[i],
        Some(limgrave),
        "first place: where Kalé stands"
    );
}

/// The terms of the shop row selling `item`, read through the bound graph.
fn terms_of<'a>(b: &'a Bound, item: &str, kind_row: impl Fn(i32) -> bool) -> &'a logic::Terms<'a> {
    let (i, _) = b
        .model
        .locations
        .iter()
        .enumerate()
        .find(|(_, l)| {
            l.shop_row().is_some_and(&kind_row)
                && l.items.iter().any(|it| it.name.as_deref() == Some(item))
        })
        .unwrap_or_else(|| panic!("no shop row sells {item}"));
    &b.location_terms[i]
}

#[test]
fn shop_rows_wait_for_what_puts_them_on_sale() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let any = |_| true;
    // Enia's armour once its boss is beaten.
    let radahn = logic::find_boss(m, "Starscourge Radahn").unwrap();
    assert_eq!(
        terms_of(&b, "Radahn's Redmane Helm", any).bosses,
        vec![radahn]
    );
    // Remembrance trades spend the remembrance.
    assert_eq!(
        terms_of(&b, "Axe of Godrick", any).items,
        vec!["Remembrance of the Grafted"]
    );
    // Dragon communion spends Dragon Hearts.
    assert!(
        terms_of(&b, "Dragonclaw", any)
            .items
            .contains(&"Dragon Heart")
    );
    // Sellen sells the Cometshard for the Conspectus Scroll (read from her
    // script), the Husks the smithing stones for their bell bearing (written
    // in the graph).
    assert!(
        terms_of(&b, "Glintstone Cometshard", |r| r == 100057)
            .any_of
            .contains(&"Conspectus Scroll")
    );
    let stones = terms_of(&b, "Smithing Stone [1]", |r| r == 101808);
    assert_eq!(stones.needs.len(), 1);
    assert_eq!(
        stones.needs[0].items,
        vec!["Smithing-Stone Miner's Bell Bearing [1]"]
    );
}

/// Rows whose on-sale flag no boss or handover explains: Seluvis's scroll
/// spells wait for their scrolls and take no key item, as he can be gone before
/// they are bought; Ymir's Glintstone Nails wait for the two bells. Few rows are
/// left never counted.
#[test]
fn shop_rows_on_flags_events_and_talk_set() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let phalanx = (0..m.locations.len())
        .find(|&i| m.locations[i].shop_row() == Some(100283))
        .unwrap();
    assert_eq!(
        b.location_terms[phalanx].needs[0].items,
        vec!["Royal House Scroll"]
    );
    assert!(b.missable(phalanx) && !m.locations[phalanx].missable);
    let nails = terms_of(&b, "Glintstone Nails", |r| r == 102302);
    assert!(nails.unexplained.is_none());
    let ways = nails.ways.as_ref().unwrap();
    assert!(
        ways.iter()
            .all(|w| w.contains(&logic::Req::Holds("Hole-Laden Necklace"))),
        "{ways:?}"
    );
    let never = b
        .location_terms
        .iter()
        .filter(|t| t.unexplained.is_some())
        .count();
    assert!(never <= 15, "{never} shop rows never counted");
}

#[test]
fn gifts_wait_for_what_their_talk_asks() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let ways = |item: &str| {
        let (i, _) = m
            .locations
            .iter()
            .enumerate()
            .find(|(_, l)| {
                l.kind == gamedata::model::Kind::Gift
                    && l.items.iter().any(|it| it.name.as_deref() == Some(item))
            })
            .unwrap_or_else(|| panic!("no gift of {item}"));
        b.location_terms[i].ways.clone()
    };
    // Melina gives the Rold Medallion once Morgott is beaten: every way
    // asks for him. (The area map's place entry speaks for it in the walk.)
    let morgott = logic::find_boss(m, "Morgott, the Omen King").unwrap();
    let rold = m
        .locations
        .iter()
        .find(|l| {
            l.items
                .iter()
                .any(|it| it.name.as_deref() == Some("Rold Medallion"))
        })
        .and_then(|l| l.given_if.as_ref())
        .unwrap();
    assert!(!rold.is_empty());
    assert!(
        rold.iter()
            .all(|w| w.contains(&gamedata::model::Cond::Boss(m.bosses[morgott].area)))
    );
    // Rya gives the Volcano Manor Invitation for her necklace.
    let invitation = ways("Volcano Manor Invitation").unwrap();
    assert!(
        invitation
            .iter()
            .all(|w| w.contains(&logic::Req::Holds("Rya's Necklace")))
    );
}

#[test]
fn quest_gifts_open_on_the_stages_events_set() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let w = b.walk(&b.vanilla());
    // Gifts that wait on an NPC's quest stage: the stage flags are explained
    // by the events that set them, and such a gift may be missed.
    use gamedata::model::Kind;
    // Shop rows may be missable too, by their on-sale flag; nothing else is.
    assert!(
        m.locations
            .iter()
            .all(|l| !l.missable || matches!(l.kind, Kind::Gift | Kind::Shop))
    );
    let missable: Vec<usize> = (0..m.locations.len())
        .filter(|&i| m.locations[i].missable && m.locations[i].kind == Kind::Gift)
        .collect();
    assert!(missable.len() > 60, "{} missable gifts", missable.len());
    let got = missable.iter().filter(|&&i| w.taken[i]).count();
    assert!(got >= 25, "only {got} quest gifts got in the vanilla walk");
    let is_missable = |item: &str| {
        m.locations
            .iter()
            .filter(|l| l.items.iter().any(|it| it.name.as_deref() == Some(item)))
            .any(|l| l.missable)
    };
    // Melina's whistle waits on nothing that moves on.
    assert!(!is_missable("Spectral Steed Whistle"));
    // Lost once an event turns on a flag they need off: the Rold Medallion's slot
    // needs the Erdtree unburnt (110), which a key item placed elsewhere allows.
    for item in [
        "Rold Medallion",
        "Grace Mimic",
        "Perfume Bottle",
        "Hole-Laden Necklace",
        "Watchful Spirit",
        "Gourmet Scorpion Stew",
    ] {
        assert!(is_missable(item), "{item}");
    }
}

/// The gifts opened by reading one condition at a time, and what they ask.
#[test]
fn gifts_read_one_at_a_time() {
    let m = model_or_skip!();
    let b = Bound::new(graph(), m);
    let w = b.walk(&b.vanilla());
    let by_lot = |id: i32| {
        (0..m.locations.len())
            .find(|&i| {
                m.locations[i].kind == gamedata::model::Kind::Gift
                    && m.locations[i]
                        .lot
                        .as_ref()
                        .is_some_and(|l| l.ids.first() == Some(&id))
            })
            .unwrap()
    };
    // The Great Jar's reward takes a key item, and asks only for its map.
    let jar = by_lot(104700);
    assert!(!m.locations[jar].missable);
    assert!(w.taken[jar], "the Jar's reward is got");
    let asks = format!("{:?}", m.locations[jar].given_if);
    assert!(asks.contains("Map(") && !asks.contains("Flag("), "{asks}");
    // Ymir's maps: open, and either-or, so none of them takes a key item.
    for lot in [106610, 106620, 106630] {
        let l = by_lot(lot);
        assert!(m.locations[l].missable, "lot {lot}");
        assert!(w.taken[l], "lot {lot} is got");
    }
    for lot in [106620, 106630] {
        let asks = format!("{:?}", m.locations[by_lot(lot)].given_if);
        assert!(asks.contains("Hole-Laden Necklace"), "{asks}");
    }
    // Nor does the gift that starts it: events can turn on a flag it needs off.
    assert!(m.locations[by_lot(106600)].missable);
}

/// The capital's gate asks for two Great Runes held (Nova, from play, 2026-11-03): they
/// are key items, and with every Great Rune taken out the capital stays shut.
#[test]
fn the_capitals_gate_asks_for_two_great_runes() {
    let Some(model) = model() else { return };
    let graph = graph();
    let edge = graph
        .edges
        .iter()
        .find(|e| e.from == "altus" && e.to == "leyndell")
        .unwrap();
    let any = edge.need.any.as_ref().unwrap();
    assert_eq!(any.n, 2);
    assert!(any.bosses.is_empty());
    assert_eq!(any.items.len(), 5);
    let bound = Bound::new(graph, model);
    assert!(bound.key_items().iter().any(|k| k.contains("Great Rune")));
    assert!(bound.walk(&bound.vanilla()).goal);
    let without: Vec<Vec<&str>> = bound
        .vanilla()
        .into_iter()
        .map(|g| g.into_iter().filter(|i| !i.ends_with("Great Rune")).collect())
        .collect();
    let w = bound.walk(&without);
    let capital = graph.areas.iter().position(|a| a.id == "leyndell").unwrap();
    assert!(!w.reached[capital], "the capital opens with no Great Rune");
}

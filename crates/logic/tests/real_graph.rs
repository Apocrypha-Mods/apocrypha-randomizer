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

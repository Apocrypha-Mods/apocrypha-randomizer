//! The area graph checked against the game: every name it uses must be one
//! the model has, every location is put in an area, and the vanilla game,
//! walked from the start, must reach every area and the goal.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fs_bytes::Result;
use gamedata::model::Kind;
use gamedata::{Sources, elden_ring};
use logic::{Bound, find_boss};

pub fn run(game: PathBuf) -> Result<()> {
    let sources = Sources::open(&game)?;
    let model = elden_ring::extract(&sources)?;
    let graph = logic::elden_ring();
    let b = Bound::new(&graph, &model);
    let unsure_areas = graph.areas.iter().filter(|a| !a.sure).count();
    let unsure = graph.edges.iter().filter(|e| !e.sure).count()
        + graph.places.iter().filter(|p| !p.sure).count();
    println!(
        "{} areas ({unsure_areas} to check), {} edges and {} places ({unsure} to check)",
        graph.areas.len(),
        graph.edges.len(),
        graph.places.len()
    );

    println!(
        "unused locations (never given in play): {}",
        b.location_unused.iter().filter(|u| **u).count()
    );
    println!("\nproblems: {}", b.problems.len());
    for p in &b.problems {
        println!("  {p}");
    }

    // Locations per area.
    let mut per: BTreeMap<Option<usize>, BTreeMap<Kind, usize>> = BTreeMap::new();
    for (l, a) in model.locations.iter().zip(&b.location_area) {
        *per.entry(*a).or_default().entry(l.kind).or_default() += 1;
    }
    println!("\nlocations by area:");
    for (a, kinds) in &per {
        let name = a.map_or("(anywhere)", |i| graph.areas[i].id.as_str());
        let all: usize = kinds.values().sum();
        let detail: Vec<String> = kinds
            .iter()
            .map(|(k, n)| format!("{} {n}", k.label()))
            .collect();
        println!("  {name:<30} {all:>5}  {}", detail.join(", "));
    }

    let w = b.walk(&b.vanilla());
    println!("\nthe vanilla game, walked from {}:", graph.start);
    let order: Vec<&str> = w
        .order
        .iter()
        .map(|&i| graph.areas[i].id.as_str())
        .collect();
    println!("  opens, in order: {}", order.join(" → "));
    let missed: Vec<&str> = graph
        .areas
        .iter()
        .zip(&w.reached)
        .filter(|(_, r)| !**r)
        .map(|(a, _)| a.id.as_str())
        .collect();
    println!(
        "  never reached: {}",
        if missed.is_empty() {
            "none".into()
        } else {
            missed.join(", ")
        }
    );
    println!("  goal reached: {}", w.goal);

    // Where the items the graph asks for are, in the vanilla game.
    println!("\nkey items, where the vanilla game has them:");
    let mut keys: Vec<&str> = graph
        .edges
        .iter()
        .map(|e| &e.need)
        .chain(graph.places.iter().map(|p| &p.need))
        .flat_map(|n| n.items.iter().chain(n.any.iter().flat_map(|a| &a.items)))
        .map(String::as_str)
        .collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let at: Vec<String> = model
            .locations
            .iter()
            .zip(&b.location_area)
            .zip(&b.location_unused)
            .filter(|((l, _), unused)| {
                !**unused && l.items.iter().any(|i| i.name.as_deref() == Some(k))
            })
            .map(|((l, a), _)| {
                format!(
                    "{} {}",
                    a.map_or("(anywhere)", |i| graph.areas[i].id.as_str()),
                    l.kind.label()
                )
            })
            .collect();
        println!("  {k}: {}", at.join("; "));
    }
    println!("\nbosses the graph asks for:");
    let mut bosses: Vec<&str> = graph
        .edges
        .iter()
        .map(|e| &e.need)
        .chain([&graph.goal])
        .chain(graph.places.iter().map(|p| &p.need))
        .flat_map(|n| n.bosses.iter().map(String::as_str))
        .collect();
    bosses.sort();
    bosses.dedup();
    for name in bosses {
        let area = find_boss(&model, name)
            .ok()
            .and_then(|i| b.boss_area[i])
            .map_or("(nowhere)", |a| graph.areas[a].id.as_str());
        println!("  {name}: {area}");
    }
    Ok(())
}

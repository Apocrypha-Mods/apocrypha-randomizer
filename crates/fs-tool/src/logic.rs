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
    // Shop rows on sale behind a flag nothing explains, by flag.
    let mut unexplained: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for (l, t) in model.locations.iter().zip(&b.location_terms) {
        if let Some(f) = t.unexplained {
            let name = l.items.first().and_then(|i| i.name.as_deref());
            unexplained.entry(f).or_default().push(name.unwrap_or("?"));
        }
    }
    println!(
        "shop rows never counted, their on-sale flag unexplained: {} ({} flags)",
        unexplained.values().map(Vec::len).sum::<usize>(),
        unexplained.len()
    );
    for (f, names) in &unexplained {
        println!("  {f}: {}", names.join(", "));
    }
    // Gifts whose every way waits on a flag nothing explains.
    let mut blocked: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    let mut gifts = 0;
    for (l, t) in model.locations.iter().zip(&b.location_terms) {
        let Some(ways) = &t.ways else { continue };
        gifts += 1;
        let never = |w: &Vec<logic::Req>| {
            w.iter().find_map(|r| match r {
                logic::Req::Never(f) => Some(*f),
                _ => None,
            })
        };
        if let Some(f) = ways.iter().map(never).collect::<Option<Vec<u32>>>() {
            let name = l.items.first().and_then(|i| i.name.as_deref());
            let first = f.first().copied().unwrap_or(0);
            blocked.entry(first).or_default().push(name.unwrap_or("?"));
        }
    }
    println!(
        "gifts whose talk asks something: {gifts}; never counted, every way waiting on a flag nothing explains: {}",
        blocked.values().map(Vec::len).sum::<usize>()
    );
    println!(
        "gifts that wait on a quest stage an event can move past, so no key item goes there: {}",
        model.locations.iter().filter(|l| l.missable).count()
    );
    for (f, names) in &blocked {
        println!("  {f}: {}", names.join(", "));
    }
    println!("\nproblems: {}", b.problems.len());
    for p in &b.problems {
        println!("  {p}");
    }

    // Locations per area.
    // Some(None) is anywhere, None unused.
    let mut per: BTreeMap<Option<Option<usize>>, BTreeMap<Kind, usize>> = BTreeMap::new();
    for ((l, a), unused) in model
        .locations
        .iter()
        .zip(&b.location_area)
        .zip(&b.location_unused)
    {
        let key = (!unused).then_some(*a);
        *per.entry(key).or_default().entry(l.kind).or_default() += 1;
    }
    println!("\nlocations by area:");
    for (a, kinds) in &per {
        let name = match a {
            None => "(unused)",
            Some(None) => "(anywhere)",
            Some(Some(i)) => graph.areas[*i].id.as_str(),
        };
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

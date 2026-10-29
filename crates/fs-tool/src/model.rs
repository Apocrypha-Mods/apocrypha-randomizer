//! The game model's coverage report (the Phase 3 gate, PLAN.md §5): how many
//! item locations of each kind every area has, named by its graces, and what
//! the extraction does not reach yet.

use std::path::PathBuf;
use std::time::Instant;

use fs_bytes::{Result, format_err};
use gamedata::model::Kind;
use gamedata::{Sources, elden_ring};

pub fn run(game: PathBuf, dump: Option<PathBuf>) -> Result<()> {
    let t = Instant::now();
    let sources = Sources::open(&game)?;
    let model = elden_ring::extract(&sources)?;
    println!(
        "{} maps, {} scripts read; model built in {:.1?}",
        sources.maps.len(),
        sources.scripts.len(),
        t.elapsed()
    );

    println!("\nlocations by kind (with a map):");
    for (kind, (all, mapped)) in elden_ring::totals(&model) {
        println!("  {:<12} {all:>6} ({mapped})", kind.label());
    }
    let named = |n: usize, of: usize| format!("{n}/{of}");
    println!(
        "enemies {}, bosses {} (placed {}, named {}), graces {} (placed {}, named {})",
        model.enemies.len(),
        model.bosses.len(),
        named(
            model.bosses.iter().filter(|b| b.map.is_some()).count(),
            model.bosses.len()
        ),
        named(
            model.bosses.iter().filter(|b| b.name.is_some()).count(),
            model.bosses.len()
        ),
        model.graces.len(),
        named(
            model.graces.iter().filter(|g| g.map.is_some()).count(),
            model.graces.len()
        ),
        named(
            model.graces.iter().filter(|g| g.name.is_some()).count(),
            model.graces.len()
        ),
    );
    let unnamed = model
        .locations
        .iter()
        .flat_map(|l| &l.items)
        .filter(|i| i.name.is_none())
        .count();
    let items: usize = model.locations.iter().map(|l| l.items.len()).sum();
    println!("items {items}, without a name {unnamed}");
    println!("\ngaps:");
    println!(
        "  treasure whose lot is not a row: {}",
        model.gaps.treasure_without_lot
    );
    for (t, ids) in &model.gaps.unreached_lots {
        println!("  {t}: {} lots with items that nothing gives", ids.len());
    }

    println!("\nby area:");
    print!("  {:<8}", "area");
    for k in Kind::ALL {
        print!(" {:>8}", short(k));
    }
    println!("  graces");
    for (area, counts) in model.coverage() {
        print!("  {area:<8}");
        for k in Kind::ALL {
            match counts.get(&k) {
                Some(n) => print!(" {n:>8}"),
                None => print!(" {:>8}", "."),
            }
        }
        let names = model.area_names(&area);
        let more = names.len().saturating_sub(3);
        let mut label = names.iter().take(3).copied().collect::<Vec<_>>().join(", ");
        if more > 0 {
            label += &format!(" (+{more})");
        }
        println!("  {label}");
    }

    if let Some(path) = dump {
        let text = toml::to_string(&model).map_err(|e| format_err(e.to_string()))?;
        std::fs::write(&path, text)?;
        println!("\nmodel written to {}", path.display());
    }
    Ok(())
}

fn short(k: Kind) -> &'static str {
    match k {
        Kind::Treasure => "treasure",
        Kind::EnemyDrop => "drop",
        Kind::RepeatDrop => "repeat",
        Kind::Event => "event",
        Kind::BossReward => "boss",
        Kind::Shop => "shop",
        Kind::Starting => "start",
    }
}

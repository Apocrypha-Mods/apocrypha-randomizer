//! One body on every spot ("Oops all") on the player's own game (`APOC_ER_STOCK`;
//! skipped when unset).

use std::path::PathBuf;

use enemies::{Oops, Options};

fn pool() -> Option<(gamedata::Sources, enemies::Pool)> {
    let game = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from)?;
    let s = gamedata::Sources::open(&game).unwrap();
    let model = gamedata::elden_ring::extract(&s).unwrap();
    let pool = enemies::pool(&s, &model).unwrap();
    Some((s, pool))
}

fn oops(name: &str) -> Options {
    Options {
        oops: Some(Oops::Named(name.to_string())),
        ..Default::default()
    }
}

/// The names the page offers can each be chosen, and stand on every spot of the base
/// game; Malenia and those who bring companions cannot be chosen.
#[test]
fn one_body_stands_everywhere() {
    let Some((s, pool)) = pool() else { return };
    let choices = enemies::oops_choices(&pool);
    if std::env::var_os("APOC_OOPS_LIST").is_some() {
        for (name, i) in &choices {
            let b = &pool.spots[*i];
            eprintln!(
                "{name}\t{}\t{}\t{:?}",
                b.body.model,
                b.boss.is_some(),
                b.size
            );
        }
    }
    assert!(choices.len() > 100, "{}", choices.len());
    for refused in ["Malenia, Blade of Miquella", "Godrick the Grafted"] {
        assert!(
            !choices.iter().any(|(n, _)| n == refused),
            "{refused} is offered"
        );
    }
    // A Cemetery Shade may stand everywhere, but not the one with skeletons.
    assert!(choices.iter().all(|(_, i)| {
        !pool
            .companions
            .contains_key(&pool.spots[*i].entity.unwrap_or(0))
    }));
    let margit = oops("Margit, the Fell Omen");
    let plan = enemies::shuffle(&pool, 1, &margit);
    let b = enemies::oops_body(&pool, 1, &margit).unwrap().unwrap();
    let base = pool.spots.iter().filter(|s| !s.dlc).count();
    assert_eq!(plan.from.iter().filter(|&&f| f == b).count(), base);
    for (i, s) in pool.spots.iter().enumerate() {
        assert_eq!(plan.from[i] == b, !s.dlc, "spot {i}");
    }
    // Every copy is written, and Malenia's stage event no longer starts in her arena.
    let mut reg = s.reg.clone();
    let applied = enemies::apply(&mut reg, &s, &pool, &plan, &margit).unwrap();
    assert!(applied.new_rows > 0);
    let (_, haligtree) = applied
        .scripts
        .iter()
        .find(|(m, _)| *m == gamedata::MapId([15, 0, 0, 0]))
        .expect("her arena's script is written");
    let starts_her_stage = haligtree.events.iter().filter(|e| e.id == 0).any(|e| {
        e.instructions.iter().any(|i| {
            i.args
                .chunks_exact(4)
                .any(|w| u32::from_le_bytes(w.try_into().unwrap()) == 15_002_811)
        })
    });
    assert!(!starts_her_stage);
}

#[test]
fn names_are_matched_whole_or_by_their_start() {
    let Some((_, pool)) = pool() else { return };
    let short = enemies::oops_body(&pool, 1, &oops("margit")).unwrap();
    let whole = enemies::oops_body(&pool, 1, &oops("Margit, the Fell Omen")).unwrap();
    assert_eq!(short, whole);
    assert!(enemies::oops_body(&pool, 1, &oops("Nobody At All")).is_err());
    let random = Options {
        oops: Some(Oops::Random),
        ..Default::default()
    };
    let a = enemies::oops_body(&pool, 1, &random).unwrap();
    assert_eq!(a, enemies::oops_body(&pool, 1, &random).unwrap());
    let drawn: std::collections::BTreeSet<_> = (0..20)
        .map(|seed| enemies::oops_body(&pool, seed, &random).unwrap())
        .collect();
    assert!(drawn.len() > 5);
}

/// Every name the page offers (`ui/logic.js`, `OOPS`) is one the module takes.
#[test]
fn the_pages_names_are_known() {
    let Some((_, pool)) = pool() else { return };
    let page =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/logic.js")).unwrap();
    let list = &page[page.find("export const OOPS").unwrap()..];
    let list = &list[..list.find("];").unwrap()];
    let names: Vec<&str> = list
        .lines()
        .filter_map(|l| l.trim().strip_prefix("[\"")?.split('"').next())
        .filter(|n| !n.is_empty() && *n != "random")
        .collect();
    assert!(names.len() >= 10, "{names:?}");
    let choices = enemies::oops_choices(&pool);
    for n in names {
        assert!(choices.iter().any(|(c, _)| c == n), "the page offers {n}");
    }
}

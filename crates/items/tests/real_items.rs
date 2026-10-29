//! Placement on the player's own game (`APOC_ER_STOCK`; skipped when unset).

use std::sync::OnceLock;

use gamedata::{Model, Sources, elden_ring};
use logic::{Bound, Graph};

fn sources() -> Option<&'static (Sources, Model)> {
    static S: OnceLock<Option<(Sources, Model)>> = OnceLock::new();
    S.get_or_init(|| {
        let dir = std::env::var_os("APOC_ER_STOCK")?;
        let s = Sources::open(std::path::Path::new(&dir)).unwrap();
        let m = elden_ring::extract(&s).unwrap();
        Some((s, m))
    })
    .as_ref()
}

fn graph() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(logic::elden_ring)
}

macro_rules! or_skip {
    () => {
        match sources() {
            Some(s) => s,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

#[test]
fn seeds_place_every_item_and_can_be_finished() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    assert!(units.len() > 4000, "{} slots", units.len());
    for seed in 0..20 {
        let plan = items::shuffle(&b, &units, seed, &items::Options::default()).unwrap();
        items::check(&b, &units, &plan).unwrap();
    }
}

#[test]
fn a_plan_is_written_into_the_item_lots() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let plan = items::shuffle(&b, &units, 3, &items::Options::default()).unwrap();
    let mut reg = fs_param::Regulation::open(std::path::Path::new(
        &std::env::var_os("APOC_ER_STOCK").unwrap(),
    ))
    .unwrap();
    items::apply(&mut reg, &units, &plan).unwrap();
    let layout = fs_param::layout::for_table("ItemLotParam_map").unwrap();
    let table = reg.table("ItemLotParam_map").unwrap();
    let mut moved = 0;
    for (u, unit) in units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.table == "ItemLotParam_map")
    {
        let row = table.rows.iter().find(|r| r.id == unit.row).unwrap();
        let now = items::item_at(&units, &plan, u);
        assert_eq!(
            layout.field("item_id").get(&row.data, unit.slot),
            i64::from(now.id)
        );
        moved += usize::from(now.id != unit.item.id);
    }
    assert!(moved > 1000, "only {moved} slots changed");
    // The changed tables pack back into a whole regulation file.
    let bytes = reg.write().unwrap();
    assert!(bytes.len() > 1_000_000);
}

#[test]
fn key_bias_puts_key_items_deeper() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    let depth = items::depth::depths(&b);
    let mean_key_depth = |bias: &str| {
        let mut o = items::Options::default();
        o.set(true, bias).unwrap();
        let (mut sum, mut n) = (0.0, 0);
        for seed in 0..10 {
            let plan = items::shuffle(&b, &units, seed, &o).unwrap();
            for (u, unit) in units.iter().enumerate() {
                if units[plan.placed[u]].category == items::Category::Key {
                    sum += depth[unit.location];
                    n += 1;
                }
            }
        }
        sum / f64::from(n)
    };
    let (flat, deep) = (mean_key_depth("keys=0"), mean_key_depth("keys=100"));
    eprintln!("mean key-item depth: bias 0 {flat:.3}, bias 100 {deep:.3}");
    assert!(deep > flat + 0.1);
}

#[test]
fn every_category_has_items() {
    let (s, m) = or_skip!();
    let b = Bound::new(graph(), m);
    let units = items::units(m, &b, &s.reg).unwrap();
    for c in items::Category::ALL {
        let n = units.iter().filter(|u| u.category == c).count();
        eprintln!("{:<13} {n}", c.name());
        assert!(n > 0, "no {} in the shuffled slots", c.name());
    }
}

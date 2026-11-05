//! Night-only bosses at any hour, on the player's own game (`APOC_ER_STOCK`; skipped
//! when unset): effect 11800 is on Night's Cavalry and their horses alone, and the
//! option clears it from them and from nothing else.

use std::path::PathBuf;

use fs_param::layout::for_table;
use gamedata::Sources;
use rando::night::{self, NIGHT_EFFECT};

#[test]
fn only_night_cavalry_and_their_horses_lose_the_effect() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        eprintln!("APOC_ER_STOCK is not set; skipping");
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let f = for_table("NpcParam").unwrap().field("effect_c");
    let carrying: Vec<i32> = s
        .reg
        .table("NpcParam")
        .unwrap()
        .rows
        .iter()
        .filter(|r| f.get(&r.data, 0) == NIGHT_EFFECT)
        .map(|r| r.id)
        .collect();
    assert_eq!(carrying.len(), 23, "{carrying:?}");
    assert!(
        carrying.iter().all(
            |id| (31_500_000..31_510_000).contains(id) || (31_600_000..31_610_000).contains(id)
        ),
        "{carrying:?}"
    );
    let mut reg = s.reg.clone();
    assert_eq!(night::apply(&mut reg).unwrap(), 23);
    let (before, after) = (
        s.reg.table("NpcParam").unwrap(),
        reg.table("NpcParam").unwrap(),
    );
    for (b, a) in before.rows.iter().zip(&after.rows) {
        if carrying.contains(&b.id) {
            assert_eq!(f.get(&a.data, 0), 0);
            let mut b2 = b.data.clone();
            f.set(&mut b2, 0, 0).unwrap();
            assert_eq!(a.data, b2, "row {} changes only there", b.id);
        } else {
            assert_eq!(a.data, b.data, "row {}", b.id);
        }
    }
}

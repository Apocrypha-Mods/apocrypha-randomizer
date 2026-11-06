//! Gestures shuffled on the player's own game (`APOC_ER_STOCK`; skipped when unset).

use std::collections::BTreeSet;
use std::path::PathBuf;

use rando::gestures::{self, KEPT};

#[test]
fn every_site_gives_a_gesture_and_the_seed_shuffles_them_among_the_sites() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = gamedata::Sources::open(&dir).unwrap();
    let rows: BTreeSet<u32> = s
        .reg
        .table("GestureParam")
        .unwrap()
        .rows
        .iter()
        .map(|r| r.id as u32)
        .collect();
    let sites = gestures::sites(&s);
    assert!(sites.len() >= 15, "{}", sites.len());
    assert!(sites.iter().all(|(_, g)| rows.contains(g)), "{sites:?}");
    assert!(
        sites.iter().any(|(_, g)| KEPT.contains(g)),
        "O Mother is a site"
    );
    let plan = gestures::plan(&s, 6);
    let mut was: Vec<u32> = plan.sites.iter().map(|&(_, w, _)| w).collect();
    let mut now: Vec<u32> = plan.sites.iter().map(|&(_, _, n)| n).collect();
    was.sort();
    now.sort();
    assert_eq!(was, now, "each gesture given as often as before");
    assert!(
        plan.sites
            .iter()
            .all(|(_, w, n)| !KEPT.contains(w) && !KEPT.contains(n))
    );
    assert_ne!(plan, gestures::plan(&s, 7));
    for (m, script) in &s.scripts {
        let mut edited = script.clone();
        let changed = plan.edit(*m, &mut edited).unwrap();
        assert_eq!(changed, plan.maps().contains_key(m));
        let again = fs_emevd::Emevd::read(&edited.write().unwrap()).unwrap();
        assert_eq!(again, edited);
    }
}

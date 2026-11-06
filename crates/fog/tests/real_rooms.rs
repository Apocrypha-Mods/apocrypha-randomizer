//! The room map and the doors' shuffle on the player's own game (`APOC_ER_STOCK`;
//! skipped when unset).

use std::path::PathBuf;

use fog::{doors, rooms};
use gamedata::{Sources, elden_ring};

#[test]
fn doors_shuffle_and_the_game_can_be_finished() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let model = elden_ring::extract(&s).unwrap();
    let base = logic::elden_ring();
    let mut all = doors::boss_fogs(&s);
    all.extend(doors::mouths(&s));
    let n = all.len();
    let eligible = rooms::eligible(all, &base, &model);
    let (bosses, mouths) = (
        eligible
            .iter()
            .filter(|d| d.kind != doors::Kind::Mouth)
            .count(),
        eligible
            .iter()
            .filter(|d| d.kind == doors::Kind::Mouth)
            .count(),
    );
    eprintln!(
        "{} of {n} doors shuffled: {bosses} boss fogs, {mouths} mouths",
        eligible.len()
    );
    assert!(bosses > 60 && mouths > 40, "{bosses} {mouths}");

    // As the game has them, the rooms change nothing: the game can be finished.
    let same: Vec<usize> = (0..eligible.len()).collect();
    let (g, outside) = rooms::graph(&base, &model, &eligible, &same).unwrap();
    let bound = logic::Bound::new(&g, &model);
    assert!(bound.problems.is_empty(), "{:?}", bound.problems);
    let walk = bound.walk(&bound.vanilla());
    let base_bound = logic::Bound::new(&base, &model);
    let base_walk = base_bound.walk(&base_bound.vanilla());
    for (i, a) in base.areas.iter().enumerate() {
        if base_walk.reached[i] && !walk.reached[i] {
            eprintln!("LOST {}", a.id);
        }
    }
    for (i, a) in g.areas.iter().enumerate().skip(base.areas.len()) {
        if !walk.reached[i] {
            eprintln!("ROOM UNREACHED {}", a.id);
        }
    }
    assert!(walk.goal);
    assert!(outside.iter().all(|o| !o.is_empty()));

    let mut tries = Vec::new();
    for seed in 0..10 {
        let plan = rooms::plan(&base, &model, eligible.clone(), seed).unwrap();
        tries.push(plan.tries);
    }
    eprintln!("draws per seed: {tries:?}");
}

/// Every map and script a seed's doors change writes and reads back as edited, each
/// door's events are in its map's script and started from event 0, and the fight's copy
/// of 9005800 waits on a flag where 9005800 waits on the prompt.
#[test]
fn a_seeds_doors_are_written_and_read_back() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let model = elden_ring::extract(&s).unwrap();
    let base = logic::elden_ring();
    let mut all = doors::boss_fogs(&s);
    all.extend(doors::mouths(&s));
    let eligible = rooms::eligible(all, &base, &model);
    let plan = rooms::plan(&base, &model, eligible, 5).unwrap();
    let ids = fog::write::ids(&plan.doors);
    let mut maps = 0;
    for (m, msb) in &s.maps {
        let mut edited = msb.clone();
        if fog::write::edit_map(&plan, &ids, *m, &mut edited).unwrap() {
            let back = fs_msb::Msb::read(&edited.write().unwrap()).unwrap();
            assert_eq!(back, edited, "{m}");
            maps += 1;
        }
    }
    let mut scripts = 0;
    for (m, script) in &s.scripts {
        let mut edited = script.clone();
        if fog::write::edit_script(&plan, &ids, *m, &mut edited).unwrap() {
            let back = fs_emevd::Emevd::read(&edited.write().unwrap()).unwrap();
            assert_eq!(back, edited, "{m}");
            let zero = edited.events.iter().find(|e| e.id == 0).unwrap();
            for (d, door) in plan.doors.iter().enumerate().filter(|(_, d)| d.map == *m) {
                for ev in ids[d].events {
                    assert!(
                        edited.events.iter().any(|e| e.id == i64::from(ev)),
                        "{}",
                        door.name
                    );
                    assert!(
                        zero.instructions
                            .iter()
                            .any(|i| i.args.get(4..8) == Some(&ev.to_le_bytes()[..]))
                    );
                }
            }
            scripts += 1;
        }
    }
    assert_eq!(maps, fog::write::maps(&plan).len());
    assert_eq!(scripts, maps);
    let func = fog::write::common_func(&s.common_func).unwrap();
    let fight = func
        .events
        .iter()
        .find(|e| e.id == fog::write::FIGHT_EVENT)
        .unwrap();
    assert!(!fight.instructions.iter().any(|i| (i.bank, i.id) == (3, 24)));
    assert_eq!(fs_emevd::Emevd::read(&func.write().unwrap()).unwrap(), func);
}

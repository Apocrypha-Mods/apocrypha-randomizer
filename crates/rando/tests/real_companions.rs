//! A boss's companions on the player's own game (`APOC_ER_STOCK`; skipped when unset).

use std::path::PathBuf;

use fs_msb::layout::field;

const PARTS: &str = "PARTS_PARAM_ST";
const SHADE: u32 = 30_050_800;

#[test]
fn cemetery_shades_skeletons_go_with_it() {
    let Some(game) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        eprintln!("APOC_ER_STOCK is not set; skipping");
        return;
    };
    let s = gamedata::Sources::open(&game).unwrap();
    let model = gamedata::elden_ring::extract(&s).unwrap();
    let pool = enemies::pool(&s, &model).unwrap();
    assert_eq!(pool.companions[&SHADE].len(), 3);
    let home = pool
        .spots
        .iter()
        .position(|p| p.entity == Some(SHADE))
        .unwrap();
    let options = enemies::Options {
        bosses: 100,
        ordinary: 100,
        ..Default::default()
    };
    let (mut plan, mut at) = (None, 0);
    for seed in 0..200 {
        let p = enemies::shuffle(&pool, seed, &options);
        at = p.from.iter().position(|&f| f == home).unwrap();
        if at != home && pool.spots[at].boss.is_some() {
            plan = Some(p);
            break;
        }
    }
    let plan = plan.expect("the Shade moved to an arena in 200 seeds");
    let mut reg = s.reg.clone();
    let applied = enemies::apply(&mut reg, &s, &pool, &plan, &options).unwrap();

    let spot = &pool.spots[at];
    let (_, msb) = applied.maps.iter().find(|(m, _)| *m == spot.map).unwrap();
    let before = &s.maps[spot.map_index].1;
    assert_eq!(
        msb.table(PARTS).entries.len(),
        before.table(PARTS).entries.len(),
        "no table grows"
    );
    let parts = &msb.table(PARTS).entries;
    let pos = |e: &[u8]| {
        let f = field(PARTS, "position");
        [
            f.get_f32(e, 0).unwrap(),
            f.get_f32(e, 1).unwrap(),
            f.get_f32(e, 2).unwrap(),
        ]
    };
    let boss = pos(&parts[spot.part]);
    let index = msb
        .table("MODEL_PARAM_ST")
        .entries
        .iter()
        .position(|e| fs_msb::layout::entry_name(e).as_deref() == Some("c3510"))
        .unwrap() as i64;
    let beside: Vec<_> = parts
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            let was = &before.table(PARTS).entries[*i];
            let p = pos(e);
            *e != was
                && field(PARTS, "model").get(e, 0) == Some(index)
                && field(PARTS, "entity_id").get(e, 0).unwrap_or(0) > 0
                && (p[0] - boss[0]).hypot(p[2] - boss[2]) <= 6.01
                && p[1] == boss[1]
        })
        .collect();
    assert_eq!(beside.len(), 3);
    // The map still writes and reads back, with names unique among the three.
    let again = fs_msb::Msb::read(&msb.write().unwrap()).unwrap();
    assert_eq!(&again, msb);
    let names: std::collections::BTreeSet<_> = beside
        .iter()
        .map(|(_, e)| fs_msb::layout::entry_name(e))
        .collect();
    assert_eq!(names.len(), 3);

    // An event at each place: one waits on the new boss, one puts the old skeletons away.
    let script = |m| &applied.scripts.iter().find(|(id, _)| *id == m).unwrap().1;
    let names_word = |e: &fs_emevd::Event, v: u32| {
        e.instructions.iter().any(|i| {
            i.args
                .chunks_exact(4)
                .any(|w| u32::from_le_bytes(w.try_into().unwrap()) == v)
        })
    };
    let new_entity = field(PARTS, "entity_id").get(beside[0].1, 0).unwrap() as u32;
    let boss_script = spot.boss.as_ref().unwrap().script;
    assert!(
        script(boss_script)
            .events
            .iter()
            .any(|e| names_word(e, new_entity))
    );
    let home_script = pool.spots[home].boss.as_ref().unwrap().script;
    assert!(
        script(home_script)
            .events
            .iter()
            .any(|e| e.id != 0 && names_word(e, 30_050_815) && e.instructions.len() == 3)
    );
}

//! DLC Start on the player's own game (`APOC_ER_STOCK`; skipped when unset): our flag,
//! event and lots are free, the warp is the arm's own, and a seed writes them.

use std::path::PathBuf;

use fs_emevd::Instruction;
use gamedata::Sources;
use rando::dlc_start::{self, ARM_GRACE, EVENT, LANDING, LOT, MAP, STARTED};

fn words(i: &Instruction) -> Vec<u32> {
    i.args
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn game() -> Option<Sources> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    Some(Sources::open(&dir).unwrap())
}

#[test]
fn the_warp_is_the_arms_and_ours_are_free() {
    let Some(s) = game() else { return };
    let (_, arm) = s
        .scripts
        .iter()
        .find(|(m, _)| *m == gamedata::MapId([12, 5, 0, 0]))
        .unwrap();
    let ev = arm.events.iter().find(|e| e.id == 12_052_500).unwrap();
    let warp = ev
        .instructions
        .iter()
        .find(|i| (i.bank, i.id) == (2003, 14))
        .unwrap();
    assert_eq!(words(warp), [u32::from_le_bytes(MAP), LANDING, ARM_GRACE]);
    let respawn = ev
        .instructions
        .iter()
        .find(|i| (i.bank, i.id) == (2003, 23))
        .unwrap();
    assert_eq!(words(respawn), [LANDING]);

    for e in s
        .scripts
        .iter()
        .map(|(_, e)| e)
        .chain([&s.common, &s.common_func])
    {
        for ev in &e.events {
            assert_ne!(ev.id, EVENT, "event {EVENT} is the game's");
            for i in &ev.instructions {
                assert!(
                    !words(i).contains(&STARTED),
                    "event {} holds {STARTED}",
                    ev.id
                );
            }
        }
    }
    for (_, name, esd) in &s.talk {
        assert!(
            !esd.expressions
                .windows(4)
                .any(|w| w == STARTED.to_le_bytes()),
            "{name}"
        );
    }
    for layout in fs_param::layout::elden_ring() {
        for field in layout.fields.iter().filter(|f| f.name.contains("flag")) {
            for table in &layout.tables {
                let Ok(t) = s.reg.table(table) else { continue };
                for r in &t.rows {
                    for k in 0..field.count {
                        assert_ne!(
                            field.get(&r.data, k),
                            i64::from(STARTED),
                            "{table} {}",
                            r.id
                        );
                    }
                }
            }
        }
    }
    let mut reg = s.reg.clone();
    dlc_start::lots(&mut reg).unwrap();
    let lots = reg.table("ItemLotParam_map").unwrap();
    let ours: Vec<i32> = lots
        .rows
        .iter()
        .map(|r| r.id)
        .filter(|&id| (LOT..LOT + 10).contains(&id))
        .collect();
    assert_eq!(ours, [LOT, LOT + 1]);
    let after = dlc_start::apply(&s.common).unwrap();
    let zero = after.events.iter().find(|e| e.id == 0).unwrap();
    assert_eq!(words(&zero.instructions[0])[1], EVENT as u32);
    assert_eq!(
        fs_emevd::Emevd::read(&after.write().unwrap()).unwrap(),
        after
    );
}

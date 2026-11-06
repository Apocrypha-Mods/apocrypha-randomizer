//! Onslaught on the player's own game (`APOC_ER_STOCK`; skipped when unset): each
//! ordinary enemy no script names stands three times over, the copies with names of
//! their own, no entity id, the original's groups, and a place beside it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs_msb::layout::{entry_name, field};

const PARTS: &str = "PARTS_PARAM_ST";

#[test]
fn three_of_every_enemy_no_script_names() {
    let Some(game) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = gamedata::Sources::open(&game).unwrap();
    let model = gamedata::elden_ring::extract(&s).unwrap();
    let pool = enemies::pool(&s, &model).unwrap();
    let mut maps = BTreeMap::new();
    let added = enemies::onslaught::apply(&s.maps, &pool, 3, &mut maps).unwrap();
    let eligible = pool
        .spots
        .iter()
        .filter(|p| p.boss.is_none() && p.entity.is_none())
        .count();
    assert_eq!(added.copies, 2 * eligible);
    assert!(enemies::onslaught::apply(&s.maps, &pool, 11, &mut BTreeMap::new()).is_err());

    let (ty, ent, pos) = (
        field(PARTS, "type"),
        field(PARTS, "entity_id"),
        field(PARTS, "position"),
    );
    let groups = field(PARTS, "entity_groups");
    for (&m, msb) in &maps {
        let before = &s.maps[m].1.table(PARTS).entries;
        let after = &msb.table(PARTS).entries;
        let names: Vec<String> = after.iter().map(|e| entry_name(e).unwrap()).collect();
        assert_eq!(
            names.iter().collect::<BTreeSet<_>>().len(),
            names.len(),
            "{}: names repeat",
            s.maps[m].0
        );
        // The enemies come first and keep their places; the copies follow them.
        let enemies_before = before.iter().filter(|e| ty.get(e, 0) == Some(2)).count();
        let enemies_after = after.iter().filter(|e| ty.get(e, 0) == Some(2)).count();
        let here = pool
            .spots
            .iter()
            .filter(|p| p.map_index == m && p.boss.is_none() && p.entity.is_none())
            .collect::<Vec<_>>();
        assert_eq!(enemies_after - enemies_before, 2 * here.len());
        // An original keeps its place and name; only indices into parts after the
        // enemies (its collision) move along.
        for spot in &here {
            assert_eq!(
                entry_name(&after[spot.part]),
                entry_name(&before[spot.part])
            );
            assert_eq!(
                pos.get(&after[spot.part], 0),
                pos.get(&before[spot.part], 0)
            );
        }
        let old_names: BTreeSet<String> = before.iter().map(|e| entry_name(e).unwrap()).collect();
        let copies: Vec<&Vec<u8>> = after
            .iter()
            .filter(|e| !old_names.contains(&entry_name(e).unwrap()))
            .collect();
        assert_eq!(copies.len(), 2 * here.len());
        for c in copies {
            let name = entry_name(c).unwrap();
            let n: u32 = name.rsplit_once('_').unwrap().1.parse().unwrap();
            assert_eq!(
                u32::from_le_bytes(c[8..12].try_into().unwrap()),
                n,
                "{name}"
            );
            assert_eq!(ent.get(c, 0), Some(0));
            // Beside one of the originals, with its groups.
            let near = here.iter().map(|p| &after[p.part]).find(|o| {
                let d: f32 = [0, 2]
                    .iter()
                    .map(|&k| pos.get_f32(o, k).unwrap() - pos.get_f32(c, k).unwrap())
                    .map(|x| x * x)
                    .sum::<f32>()
                    .sqrt();
                (d - 1.5).abs() < 0.01 && pos.get_f32(o, 1) == pos.get_f32(c, 1)
            });
            let o = near.unwrap_or_else(|| panic!("{name} stands by no original"));
            assert_eq!(
                (0..groups.count)
                    .map(|k| groups.get(o, k))
                    .collect::<Vec<_>>(),
                (0..groups.count)
                    .map(|k| groups.get(c, k))
                    .collect::<Vec<_>>()
            );
        }
        // The map writes and reads back.
        let again = fs_msb::Msb::read(&msb.write().unwrap()).unwrap();
        assert_eq!(&again, msb);
    }
}

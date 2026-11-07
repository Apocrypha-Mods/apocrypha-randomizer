//! Torrent in the woods, music and enemy colours on the player's own game
//! (`APOC_ER_STOCK`; skipped when unset).

use std::collections::BTreeSet;
use std::path::PathBuf;

use fs_param::layout::for_table;
use gamedata::{Sources, elden_ring};
use rando::{colours, music, tweaks};

fn game() -> Option<Sources> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    Some(Sources::open(&dir).unwrap())
}

/// 433 is on the woods' effect and 190 alone, and only the woods' effect changes.
#[test]
fn torrent_is_let_into_the_woods_by_one_number() {
    let Some(s) = game() else { return };
    let f = for_table("SpEffectParam").unwrap().field("behaviour");
    let before = s.reg.table("SpEffectParam").unwrap();
    let holding: Vec<i32> = before
        .rows
        .iter()
        .filter(|r| f.get(&r.data, 0) == tweaks::NO_TORRENT)
        .map(|r| r.id)
        .collect();
    assert_eq!(holding, [190, tweaks::WOODS_EFFECT]);
    // Common event 1070 gives it, from event 0.
    let woods = s.common.events.iter().find(|e| e.id == 1070).unwrap();
    assert!(woods.instructions.iter().any(|i| {
        i.args
            .windows(4)
            .any(|w| w == tweaks::WOODS_EFFECT.to_le_bytes())
    }));
    let mut reg = s.reg.clone();
    tweaks::torrent_in_woods(&mut reg).unwrap();
    let after = reg.table("SpEffectParam").unwrap();
    for (b, a) in before.rows.iter().zip(&after.rows) {
        if b.id == tweaks::WOODS_EFFECT {
            assert_eq!(f.get(&a.data, 0), 0);
        } else {
            assert_eq!(a.data, b.data, "{}", b.id);
        }
    }
}

/// The themes the scripts play are rows of the table, Margit's among them, and the
/// shuffle keeps the same names; area music keeps the same numbers, the Roundtable
/// Hold's among them.
#[test]
fn music_changes_places_and_nothing_is_lost() {
    let Some(s) = game() else { return };
    let mut scripts = vec![&s.common, &s.common_func];
    scripts.extend(s.scripts.iter().map(|(_, e)| e));
    let played = music::played(&scripts);
    assert!(played.contains(&213_000), "Margit's c2130_A");
    let names = |reg: &fs_param::Regulation| -> Vec<Vec<u8>> {
        let mut v: Vec<Vec<u8>> = reg
            .table(music::NAMES)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.data[4..36].to_vec())
            .collect();
        v.sort();
        v
    };
    let mut reg = s.reg.clone();
    let n = music::shuffle_boss(&mut reg, &played, 7).unwrap();
    assert!((40..=80).contains(&n), "{n} themes");
    assert_eq!(names(&reg), names(&s.reg));
    assert_ne!(
        reg.table(music::NAMES).unwrap().rows,
        s.reg.table(music::NAMES).unwrap().rows
    );

    let maps: Vec<&fs_msb::Msb> = s.maps.iter().map(|(_, m)| m).collect();
    let map = music::area_swap(&reg, &maps, 7).unwrap();
    assert!(map.contains_key(&111), "the Hold's Bgm_111_Hub");
    let from: BTreeSet<_> = map.keys().collect();
    let to: BTreeSet<_> = map.values().collect();
    assert_eq!(from, to);
    assert!(!map.contains_key(&999));
    music::apply_table(&mut reg, &map).unwrap();
}

/// Area music: the swap covers the map table's numbers and the sound areas' (Stormveil's
/// 100, the DLC's 50 to 563); in the maps only the sound areas' two numbers change, and
/// the numbers are the same set after, in the table and in the maps alike.
#[test]
fn area_music_reaches_the_sound_areas_and_only_their_numbers() {
    let Some(s) = game() else { return };
    let maps: Vec<&fs_msb::Msb> = s.maps.iter().map(|(_, m)| m).collect();
    let swap = music::area_swap(&s.reg, &maps, 7).unwrap();
    let table: BTreeSet<i64> = {
        let f = for_table("MapDefaultInfoParam").unwrap().field("bgm_place");
        let t = s.reg.table("MapDefaultInfoParam").unwrap();
        t.rows.iter().map(|r| f.get(&r.data, 0)).collect()
    };
    let points: BTreeSet<i64> = maps
        .iter()
        .flat_map(|m| music::map_places(m))
        .filter(|&p| p != -1)
        .collect();
    assert_eq!(
        swap.keys().copied().collect::<BTreeSet<_>>(),
        table
            .union(&points)
            .copied()
            .filter(|&p| p != 999)
            .collect(),
    );
    assert!(points.contains(&100) && points.contains(&561), "{points:?}");
    assert!(!points.contains(&999));
    eprintln!(
        "{} numbers: {} in the table, {} in sound areas, {} only there",
        swap.len(),
        table.len() - 1,
        points.len(),
        points.difference(&table).count()
    );
    // The fields the swap may touch: the type and the two numbers of type-30 points.
    let (ty, place, also) = (
        fs_msb::layout::field("POINT_PARAM_ST", "type"),
        fs_msb::layout::field("POINT_PARAM_ST", "bgm_place"),
        fs_msb::layout::field("POINT_PARAM_ST", "bgm_place_also"),
    );
    let (mut changed, mut before_all, mut after_all) = (0, Vec::new(), Vec::new());
    for (m, msb) in &s.maps {
        let mut edited = msb.clone();
        let did = music::edit_map(&swap, &mut edited).unwrap();
        let (b, a) = (
            &msb.table("POINT_PARAM_ST").entries,
            &edited.table("POINT_PARAM_ST").entries,
        );
        for (t, name) in fs_msb::TABLES.iter().enumerate() {
            if *name != "POINT_PARAM_ST" {
                assert_eq!(msb.tables[t], edited.tables[t], "{m} {name}");
            }
        }
        let mut any = false;
        for (x, y) in b.iter().zip(a) {
            if ty.get(x, 0) != Some(30) {
                assert_eq!(x, y, "{m}: not a sound area");
                continue;
            }
            for f in [place, also] {
                let (vx, vy) = (f.get(x, 0).unwrap(), f.get(y, 0).unwrap());
                before_all.push(vx);
                after_all.push(vy);
                assert_eq!(vy, swap.get(&vx).copied().unwrap_or(vx), "{m}");
                any |= vx != vy;
            }
            let mut z = y.clone();
            place.set(&mut z, 0, place.get(x, 0).unwrap()).unwrap();
            also.set(&mut z, 0, also.get(x, 0).unwrap()).unwrap();
            assert_eq!(&z, x, "{m}: only the two numbers change");
        }
        assert_eq!(did, any, "{m}");
        changed += usize::from(did);
        // The map reads back as it was edited.
        assert_eq!(fs_msb::Msb::read(&edited.write().unwrap()).unwrap(), edited);
    }
    assert!(changed > 40, "{changed} maps");
    let set = |v: &[i64]| v.iter().copied().collect::<BTreeSet<_>>();
    // A number every point used still names a used place (the set may shrink to the
    // numbers that were drawn onto these maps, but each is one of the swap's).
    let after = set(&after_all);
    assert!(
        after
            .iter()
            .all(|p| *p == -1 || swap.values().any(|q| q == p))
    );
    assert_eq!(before_all.len(), after_all.len());
    let mut reg = s.reg.clone();
    music::apply_table(&mut reg, &swap).unwrap();
    // Over table and maps together, the numbers are the same set as before.
    let f = for_table("MapDefaultInfoParam").unwrap().field("bgm_place");
    let t = reg.table("MapDefaultInfoParam").unwrap();
    let mut all_after: BTreeSet<i64> = t.rows.iter().map(|r| f.get(&r.data, 0)).collect();
    all_after.extend(after);
    let mut all_before = table.clone();
    all_before.extend(set(&before_all));
    assert_eq!(all_after, all_before);
    eprintln!("{changed} maps' sound areas change");
}

/// Ordinary enemies' rows take a colour, no boss's does, and our rows are free.
#[test]
fn ordinary_enemies_take_a_colour() {
    let Some(s) = game() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let rows = colours::rows_to_colour(&model);
    let mut reg = s.reg.clone();
    let coloured = colours::apply(&mut reg, &rows, 3).unwrap();
    assert!(coloured.len() > 300, "{}", coloured.len());
    // Margit's row is a boss's.
    let margit = model
        .enemies
        .iter()
        .find(|e| e.entity == Some(10_000_850))
        .unwrap()
        .npc_param;
    assert!(!coloured.contains_key(&margit));
    let last = for_table("NpcParam").unwrap().field("effect_last");
    let npc = reg.table("NpcParam").unwrap();
    for r in &npc.rows {
        if let Some(k) = coloured.get(&r.id) {
            assert_eq!(
                last.get(&r.data, 0),
                i64::from(colours::EFFECT_FIRST + *k as i32)
            );
        }
    }
    let phantom = reg.table("PhantomParam").unwrap();
    assert!(phantom.rows.iter().any(|r| r.id == colours::PHANTOM_FIRST));
}

#[derive(Default)]
struct Mem(std::sync::Mutex<Vec<(String, Vec<u8>)>>);

impl rando::Sink for Mem {
    fn put(&self, path: &str, bytes: Vec<u8>) -> fs_bytes::Result<()> {
        self.0.lock().unwrap().push((path.to_string(), bytes));
        Ok(())
    }
}

/// A seed with area music, with the enemies left alone and moved: every map it writes
/// reads back, each tile and later version with a changed number is written once, and
/// its sound areas hold the swap of the game's numbers (taken once, not twice where
/// the enemies wrote the map too).
#[test]
fn a_seed_with_area_music_writes_its_maps() {
    let Some(dir) = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from) else {
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let files = gamedata::GameFiles::open(&dir, &[]).unwrap();
    let model = elden_ring::extract(&s).unwrap();
    let mut options = rando::ItemOptions::default();
    options.area_music = true;
    let maps: Vec<&fs_msb::Msb> = s.maps.iter().map(|(_, m)| m).collect();
    let swap = music::area_swap(&s.reg, &maps, 5).unwrap();
    let game_map = |m: gamedata::MapId| -> fs_msb::Msb {
        let p = rando::far::map_path(m);
        fs_msb::Msb::read(&fs_dcx::decompress(&files.read(&p).unwrap()).unwrap().1).unwrap()
    };
    let numbers = |msb: &fs_msb::Msb| -> Vec<i64> { music::map_places(msb).collect() };
    for foes in ["", "bosses=100,ordinary=100"] {
        let mut enemies = rando::EnemyOptions::default();
        if !foes.is_empty() {
            enemies.set(foes).unwrap();
        }
        let sink = Mem::default();
        let built = rando::build(
            &files,
            &s,
            &model,
            rando::Job {
                seed: 5,
                options: &options,
                enemies: &enemies,
            },
            &sink,
            &|_| {},
        )
        .unwrap();
        assert!(built.spoiler.contains("Area music:"));
        let written = sink.0.into_inner().unwrap();
        let mut seen = BTreeSet::new();
        let mut twins = 0;
        for (p, bytes) in &written {
            let Some(m) = gamedata::MapId::find_in(p).filter(|_| p.ends_with(".msb.dcx")) else {
                continue;
            };
            assert!(seen.insert(m), "{p} written twice");
            let msb = fs_msb::Msb::read(&fs_dcx::decompress(bytes).unwrap().1).unwrap();
            let want: Vec<i64> = numbers(&game_map(m))
                .into_iter()
                .map(|v| swap.get(&v).copied().unwrap_or(v))
                .collect();
            assert_eq!(numbers(&msb), want, "{foes}: {m}");
            twins += usize::from(rando::far::twin(m).is_none() && m.is_open_world());
        }
        // Every tile and later version whose numbers change is among them.
        let mut wanted: Vec<gamedata::MapId> = s.maps.iter().map(|(m, _)| *m).collect();
        wanted.extend(s.maps.iter().filter_map(|(m, _)| rando::far::twin(*m)));
        for m in wanted {
            if !files.exists(&rando::far::map_path(m)) {
                continue;
            }
            let map = game_map(m);
            if music::map_places(&map).any(|p| swap.get(&p).is_some_and(|q| *q != p)) {
                assert!(seen.contains(&m), "{foes}: {m} not written");
            }
        }
        assert!(twins >= 5, "{foes}: {twins} later versions");
        eprintln!(
            "{foes:?}: {} maps written, {twins} later versions",
            seen.len()
        );
    }
}

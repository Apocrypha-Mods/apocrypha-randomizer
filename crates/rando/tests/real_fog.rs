//! Fog gates on the player's own game (`APOC_ER_STOCK`; skipped when unset): the doors
//! a seed may shuffle, and a seed's doors written into the later versions of their
//! tiles as into the tiles.

use std::path::PathBuf;
use std::sync::Mutex;

use fs_emevd::Emevd;
use fs_msb::Msb;
use gamedata::{GameFiles, MapId, Sources};

fn game() -> Option<(PathBuf, GameFiles, Sources)> {
    let dir = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from)?;
    let files = GameFiles::open(&dir, &[]).unwrap();
    let s = Sources::open_with(&dir, &files).unwrap();
    Some((dir, files, s))
}

#[derive(Default)]
struct Mem(Mutex<Vec<(String, Vec<u8>)>>);

impl rando::Sink for Mem {
    fn put(&self, path: &str, bytes: Vec<u8>) -> fs_bytes::Result<()> {
        self.0.lock().unwrap().push((path.to_string(), bytes));
        Ok(())
    }
}

/// Mad Pumpkin Head's fog (m60_44_36) and the Red Bear's (m61_46_45) stand on tiles
/// whose later versions are their copies, so they are doors; Romina's (m61_44_45) is
/// not, as her tile's later version has neither her fog nor her arena.
#[test]
fn doors_on_tiles_with_later_versions() {
    let Some((_, files, s)) = game() else { return };
    let model = gamedata::elden_ring::extract(&s).unwrap();
    let graph = logic::elden_ring();
    let doors = rando::fog_doors(&files, &s, &model, &graph).unwrap();
    let names: Vec<&str> = doors.iter().map(|d| d.name.as_str()).collect();
    eprintln!(
        "{} doors: {} boss fogs, {} mouths, {} on open-world tiles",
        doors.len(),
        doors.iter().filter(|d| d.fog.is_some()).count(),
        doors.iter().filter(|d| d.fog.is_none()).count(),
        doors.iter().filter(|d| d.map.is_open_world()).count()
    );
    for name in [
        "boss:1044361800",
        "boss:2046451800",
        "boss:1043301800",
        "mouth:m31_18_00_00",
    ] {
        assert!(names.contains(&name), "{name}");
    }
    assert!(!names.contains(&"boss:2044451800"));
}

/// A seed with items and fog gates, then one with enemies too: each door's map and
/// script are written, and the later version of a door's tile is written as the tile.
#[test]
fn a_seeds_doors_follow_into_later_versions() {
    let Some((_, files, s)) = game() else { return };
    let model = gamedata::elden_ring::extract(&s).unwrap();
    let mut options = rando::ItemOptions::default();
    options.fog_gates = true;
    let plain = |b: &[u8]| fs_dcx::decompress(b).unwrap().1;
    let off = rando::EnemyOptions::default();
    let moving = rando::EnemyOptions {
        bosses: 100,
        ordinary: 100,
        ..Default::default()
    };
    for (seed, foes) in [(3, &off), (7, &moving)] {
        let sink = Mem::default();
        rando::build(
            &files,
            &s,
            &model,
            rando::Job {
                seed,
                options: &options,
                enemies: foes,
            },
            &sink,
            &|_| {},
        )
        .unwrap();
        let written = sink.0.into_inner().unwrap();
        let get = |p: &str| {
            written
                .iter()
                .find(|(q, _)| q == p)
                .map(|(_, b)| plain(b))
                .unwrap_or_else(|| panic!("{p} not written"))
        };
        let graph = rando::graph_for(&options).unwrap();
        let plan = rando::fog_plan(&files, &s, &model, &graph, seed).unwrap();
        let ids = fog::write::ids(&plan.doors);
        let mut twins = 0;
        for m in fog::write::maps(&plan) {
            let msb = Msb::read(&get(&format!("map/mapstudio/{m}.msb.dcx"))).unwrap();
            let script = Emevd::read(&get(&format!("event/{m}.emevd.dcx"))).unwrap();
            for (d, door) in plan.doors.iter().enumerate().filter(|(_, d)| d.map == m) {
                assert!(
                    fog::doors::locate(&msb, ids[d].outside).is_some(),
                    "{}",
                    door.name
                );
                for ev in ids[d].events {
                    assert!(script.events.iter().any(|e| e.id == i64::from(ev)));
                }
            }
            let [a, b, c, v] = m.0;
            let t = MapId([a, b, c, v + 10]);
            if m.is_open_world() && files.exists(&format!("/map/mapstudio/{t}.msb.dcx")) {
                assert_eq!(
                    get(&format!("map/mapstudio/{t}.msb.dcx")),
                    get(&format!("map/mapstudio/{m}.msb.dcx")),
                    "{t}"
                );
                assert_eq!(
                    get(&format!("event/{t}.emevd.dcx")),
                    get(&format!("event/{m}.emevd.dcx")),
                    "{t}"
                );
                twins += 1;
            }
        }
        eprintln!(
            "seed {seed}: {} doors, {twins} later versions",
            plan.doors.len()
        );
        assert_eq!(twins, 2, "seed {seed}");
    }
}

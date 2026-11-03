//! Leyndell's gate counts Great Runes held, on the player's own game (`APOC_ER_STOCK`;
//! skipped when unset): the runes are the game's own, both forms; our flags and events
//! are free; the edited script reads back as built and nothing else in it changes;
//! nothing else turns 182 on; and a seed writes it, on another mod's script too.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use fs_emevd::catalogue::{Entry, runs};
use fs_emevd::{Emevd, Instruction};
use gamedata::{GameFiles, Model, Sources, elden_ring};
use rando::capital::{self, COUNT_EVENT, GATE, HELD, RUNE_EVENTS, RUNES};

struct Game {
    dir: PathBuf,
    sources: Sources,
    model: Model,
}

fn game() -> Option<&'static Game> {
    static GAME: OnceLock<Option<Game>> = OnceLock::new();
    GAME.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
        let sources = Sources::open(&dir).unwrap();
        let model = elden_ring::extract(&sources).unwrap();
        Some(Game {
            dir,
            sources,
            model,
        })
    })
    .as_ref()
}

macro_rules! game_or_skip {
    () => {
        match game() {
            Some(g) => g,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

fn words(i: &Instruction) -> Vec<u32> {
    i.args
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn scripts(s: &Sources) -> impl Iterator<Item = &Emevd> {
    s.scripts
        .iter()
        .map(|(_, e)| e)
        .chain([&s.common, &s.common_func])
}

/// The Divine Towers' calls of event 90005110 name the restored rune first and the
/// unrestored one fifth; ours are those, Morgott's left out, and the area map names them.
#[test]
fn the_runes_are_the_towers_own_and_the_maps() {
    let g = game_or_skip!();
    let mut towers = BTreeSet::new();
    for e in scripts(&g.sources) {
        for ev in &e.events {
            for i in &ev.instructions {
                let w = words(i);
                if (i.bank, i.id) == (2000, 6) && w.get(1) == Some(&90_005_110) {
                    towers.insert((w[2], w[6]));
                }
            }
        }
    }
    assert_eq!(towers.len(), 6, "{towers:?}");
    let morgott = (193, 8150);
    assert!(towers.remove(&morgott));
    let ours: BTreeSet<(u32, u32)> = RUNES.iter().map(|r| (r.restored, r.unrestored)).collect();
    assert_eq!(towers, ours);
    for r in RUNES {
        for id in [r.restored, r.unrestored] {
            assert_eq!(g.sources.text("GoodsName", id as i32), Some(r.name));
        }
    }
    let graph = logic::elden_ring();
    let edge = graph
        .edges
        .iter()
        .find(|e| e.from == "altus" && e.to == "leyndell")
        .unwrap();
    let names: Vec<&str> = RUNES.iter().map(|r| r.name).collect();
    assert_eq!(edge.need.any.as_ref().unwrap().items, names);
}

/// No script, talk script or flag field of the tables names our flags, and no script
/// has our events' ids.
#[test]
fn our_flags_and_events_are_free_in_the_game() {
    let g = game_or_skip!();
    let ids: Vec<i64> = RUNE_EVENTS.iter().copied().chain([COUNT_EVENT]).collect();
    for e in scripts(&g.sources) {
        for ev in &e.events {
            assert!(!ids.contains(&ev.id), "event {} is the game's", ev.id);
            for i in &ev.instructions {
                for f in HELD {
                    assert!(!words(i).contains(&f), "event {} holds {f}", ev.id);
                }
            }
        }
    }
    for (_, name, esd) in &g.sources.talk {
        for f in HELD {
            let bytes = f.to_le_bytes();
            assert!(
                !esd.expressions.windows(4).any(|w| w == bytes),
                "{name} holds {f}"
            );
        }
    }
    let mut fields = 0;
    for layout in fs_param::layout::elden_ring() {
        for field in layout.fields.iter().filter(|f| f.name.contains("flag")) {
            for table in &layout.tables {
                let Ok(t) = g.sources.reg.table(table) else {
                    continue;
                };
                fields += 1;
                for r in &t.rows {
                    for k in 0..field.count {
                        let v = field.get(&r.data, k);
                        assert!(
                            !HELD.iter().any(|&f| i64::from(f) == v),
                            "{table} row {} {} is {v}",
                            r.id,
                            field.name
                        );
                    }
                }
            }
        }
    }
    assert!(fields >= 5, "{fields} flag fields read");
}

/// For every number of runes: event 0 differs by the one start, six events are added
/// and read back as built, and every other event is the game's.
#[test]
fn the_script_changes_only_where_meant_and_reads_back() {
    let g = game_or_skip!();
    let before = &g.sources.common;
    for n in 0..=4u8 {
        let after = capital::apply(before, n).unwrap();
        let again = Emevd::read(&after.write().unwrap()).unwrap();
        assert_eq!(again, after, "{n}: reads back as written");
        assert_eq!(after.events.len(), before.events.len() + 6);
        for (old, new) in before.events.iter().zip(&after.events) {
            assert_eq!(old.id, new.id);
            if old.id != 0 {
                assert_eq!(old, new, "event {}", old.id);
            }
        }
        let (old0, new0) = (&before.events[0], &after.events[0]);
        assert_eq!(new0.parameters, old0.parameters);
        assert_eq!(new0.instructions.len(), old0.instructions.len() + 5);
        let changed: Vec<usize> = old0
            .instructions
            .iter()
            .zip(&new0.instructions)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(k, _)| k)
            .collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(words(&old0.instructions[changed[0]]), [2, 730, 182, 2]);
        assert_eq!(words(&new0.instructions[changed[0]]), [0, 731, 0]);
        let added: Vec<u32> = new0.instructions[old0.instructions.len()..]
            .iter()
            .map(|i| words(i)[1])
            .collect();
        assert_eq!(added, [732, 733, 734, 735, 736]);
        let game_730 = before.events.iter().find(|e| e.id == 730).unwrap();
        let count = after.events.iter().find(|e| e.id == COUNT_EVENT).unwrap();
        assert_eq!(count, &capital::count_event(game_730.rest, n));
        assert_eq!(
            words(&count.instructions[2])[1..],
            [400, 404, 4, u32::from(n)]
        );
        // 730's own shape, counting ours in place of 170 to 179.
        let shape = |e: &fs_emevd::Event| -> Vec<(u32, u32)> {
            e.instructions.iter().map(|i| (i.bank, i.id)).collect()
        };
        assert_eq!(shape(count), shape(game_730));
        for ((&id, rune), flag) in RUNE_EVENTS.iter().zip(RUNES).zip(HELD) {
            let e = after.events.iter().find(|e| e.id == id).unwrap();
            assert_eq!(e, &capital::rune_event(id, game_730.rest, rune, flag));
            let held: Vec<(u32, u32)> = e
                .instructions
                .iter()
                .filter(|i| (i.bank, i.id) == (3, 4))
                .map(|i| (Entry::word(i, 1).unwrap(), Entry::word(i, 2).unwrap()))
                .collect();
            assert_eq!(held, [(rune.unrestored, 1), (rune.restored, 1)]);
        }
    }
}

/// In the game, only event 730's start for 182 turns it on; with ours, only our event
/// does, and nothing counts flags 170 to 179 into it.
#[test]
fn nothing_else_turns_182_on() {
    let g = game_or_skip!();
    let setters = |common: &Emevd| -> Vec<i64> {
        let mut out = Vec::new();
        for e in g.sources.scripts.iter().map(|(_, e)| e).chain([common]) {
            for run in runs(e, Some(&g.sources.common_func)) {
                for i in &run.instructions {
                    let w = words(i);
                    let sets = match (i.bank, i.id) {
                        (2003, 66) | (2003, 69) => w.get(1) == Some(&GATE),
                        // A range of flags, set or cleared.
                        (2003, 63) => w.len() > 1 && w[0] <= GATE && GATE <= w[1],
                        _ => false,
                    };
                    if sets {
                        out.push(run.event.id);
                    }
                }
            }
        }
        out
    };
    assert_eq!(setters(&g.sources.common), [730]);
    assert_eq!(
        setters(&capital::apply(&g.sources.common, 2).unwrap()),
        [COUNT_EVENT]
    );
}

#[derive(Default)]
struct Mem(Mutex<Vec<(String, Vec<u8>)>>);

impl rando::Sink for Mem {
    fn put(&self, path: &str, bytes: Vec<u8>) -> fs_bytes::Result<()> {
        self.0.lock().unwrap().push((path.to_string(), bytes));
        Ok(())
    }
}

fn written_common(files: &GameFiles, sources: &Sources, model: &Model, n: u8) -> Emevd {
    let mut options = rando::ItemOptions::default();
    options.set_leyndell_runes(n).unwrap();
    let sink = Mem::default();
    rando::build(
        files,
        sources,
        model,
        rando::Job {
            seed: 5,
            options: &options,
            enemies: &rando::EnemyOptions::default(),
        },
        &sink,
        &|_| {},
    )
    .unwrap();
    let written = sink.0.into_inner().unwrap();
    let (_, bytes) = written
        .iter()
        .find(|(p, _)| p == "event/common.emevd.dcx")
        .expect("the seed writes common.emevd");
    Emevd::read(&fs_dcx::decompress(bytes).unwrap().1).unwrap()
}

/// A seed writes the script for its number, and on another mod's script builds on it.
#[test]
fn a_seed_writes_the_gate_on_the_script_it_stands_on() {
    let g = game_or_skip!();
    let files = GameFiles::open(&g.dir, &[]).unwrap();
    for n in [0, 3] {
        assert_eq!(
            written_common(&files, &g.sources, &g.model, n),
            capital::apply(&g.sources.common, n).unwrap()
        );
    }
    // A mod whose common.emevd has one event more.
    let dir = std::env::temp_dir().join(format!("apoc-capital-{}", std::process::id()));
    let mut theirs = g.sources.common.clone();
    let mut extra = theirs.events.iter().find(|e| e.id == 730).unwrap().clone();
    extra.id = 999_999;
    theirs.events.push(extra);
    let path = "/event/common.emevd.dcx";
    let (header, _) = fs_dcx::decompress(&files.read(path).unwrap()).unwrap();
    let packed = fs_dcx::compress(
        &header.with_algorithm(fs_dcx::Algorithm::Zstd),
        &theirs.write().unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("event")).unwrap();
    std::fs::write(dir.join("event/common.emevd.dcx"), packed).unwrap();
    let over = GameFiles::open(&g.dir, std::slice::from_ref(&dir)).unwrap();
    let sources = Sources::open_with(&g.dir, &over).unwrap();
    assert_eq!(sources.common, theirs);
    let got = written_common(&over, &sources, &g.model, 2);
    std::fs::remove_dir_all(&dir).ok();
    assert!(got.events.iter().any(|e| e.id == 999_999));
    assert_eq!(got, capital::apply(&theirs, 2).unwrap());
}

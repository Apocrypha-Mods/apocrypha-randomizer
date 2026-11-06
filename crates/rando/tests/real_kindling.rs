//! Messmer's Kindling in shards on the player's own game (`APOC_ER_STOCK`; skipped when
//! unset): our flags and events are free, the Sealing Tree's event changes only where it
//! asks for and takes the Kindling, and a seed writes the shards' rows, names and script.

use std::path::PathBuf;
use std::sync::Mutex;

use fs_emevd::{Emevd, Instruction};
use gamedata::{GameFiles, Sources};
use rando::kindling::{self, COUNT_EVENT, FIRST_SHARD_EVENT, HELD, KINDLED, TREE_EVENT};

fn words(i: &Instruction) -> Vec<u32> {
    i.args
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn game() -> Option<(PathBuf, Sources)> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    let s = Sources::open(&dir).unwrap();
    Some((dir, s))
}

fn ours() -> Vec<u32> {
    HELD.chain([KINDLED]).collect()
}

#[test]
fn our_flags_and_events_are_free_in_the_game() {
    let Some((_, s)) = game() else { return };
    let events: Vec<i64> = (FIRST_SHARD_EVENT..=COUNT_EVENT).collect();
    let flags = ours();
    for e in s
        .scripts
        .iter()
        .map(|(_, e)| e)
        .chain([&s.common, &s.common_func])
    {
        for ev in &e.events {
            assert!(!events.contains(&ev.id), "event {} is the game's", ev.id);
            for i in &ev.instructions {
                for f in &flags {
                    assert!(!words(i).contains(f), "event {} holds {f}", ev.id);
                }
            }
        }
    }
    for (_, name, esd) in &s.talk {
        for f in &flags {
            assert!(
                !esd.expressions.windows(4).any(|w| w == f.to_le_bytes()),
                "{name} holds {f}"
            );
        }
    }
    let mut fields = 0;
    for layout in fs_param::layout::elden_ring() {
        for field in layout.fields.iter().filter(|f| f.name.contains("flag")) {
            for table in &layout.tables {
                let Ok(t) = s.reg.table(table) else { continue };
                fields += 1;
                for r in &t.rows {
                    for k in 0..field.count {
                        let v = field.get(&r.data, k);
                        assert!(
                            !flags.iter().any(|&f| i64::from(f) == v),
                            "{table} {} {}",
                            r.id,
                            field.name
                        );
                    }
                }
            }
        }
    }
    assert!(fields >= 5);
    let goods = s.reg.table("EquipParamGoods").unwrap();
    for k in 1..=items::kindling::MOST {
        assert!(!goods.rows.iter().any(|r| r.id == items::kindling::id(k)));
    }
}

#[test]
fn the_tree_asks_for_enough_shards_and_nothing_else_changes() {
    let Some((_, s)) = game() else { return };
    let before = &s.common;
    let after = kindling::apply(before, 5, 3).unwrap();
    let tree = |e: &Emevd| {
        e.events
            .iter()
            .find(|x| x.id == TREE_EVENT)
            .unwrap()
            .clone()
    };
    let (old, new) = (tree(before), tree(&after));
    assert_eq!(old.instructions.len(), new.instructions.len());
    let mut asks = 0;
    for (a, b) in old.instructions.iter().zip(&new.instructions) {
        let kindling = words(a).get(1) == Some(&2_008_021);
        match (a.bank, a.id) {
            (3, 4) if kindling => {
                asks += 1;
                assert_eq!((b.bank, b.id), (3, 0));
                let (wa, wb) = (words(a), words(b));
                assert_eq!(wb[1], KINDLED);
                assert_eq!(wa[0].to_le_bytes()[0], wb[0].to_le_bytes()[0], "same group");
                assert_eq!(u32::from(wb[0].to_le_bytes()[1]), u32::from(wa[2] != 0));
            }
            (2003, 24) if kindling => assert_eq!(*b, enemies::malenia::nothing()),
            _ => assert_eq!(a, b),
        }
    }
    assert_eq!(asks, 3);
    // Event 0 starts ours first; every other event is the game's; ours read back.
    let zero = after.events.iter().find(|e| e.id == 0).unwrap();
    let started: Vec<u32> = zero.instructions[..6].iter().map(|i| words(i)[1]).collect();
    assert_eq!(started, [7900, 7901, 7902, 7903, 7904, 7920]);
    for e in &before.events {
        if e.id != 0 && e.id != TREE_EVENT {
            assert_eq!(after.events.iter().find(|x| x.id == e.id), Some(e));
        }
    }
    assert_eq!(after.events.len(), before.events.len() + 6);
    assert_eq!(Emevd::read(&after.write().unwrap()).unwrap(), after);
    assert!(kindling::apply(before, 5, 6).is_err());
    assert_eq!(kindling::apply(before, 1, 1).unwrap(), *before);
}

#[derive(Default)]
struct Mem(Mutex<Vec<(String, Vec<u8>)>>);

impl rando::Sink for Mem {
    fn put(&self, path: &str, bytes: Vec<u8>) -> fs_bytes::Result<()> {
        self.0.lock().unwrap().push((path.to_string(), bytes));
        Ok(())
    }
}

#[test]
fn a_seed_writes_the_shards_rows_and_names() {
    let Some((dir, s)) = game() else { return };
    let files = GameFiles::open(&dir, &[]).unwrap();
    let model = gamedata::elden_ring::extract(&s).unwrap();
    let mut options = rando::ItemOptions::default();
    options.kindling_shards = 5;
    options.kindling_needed = 3;
    let sink = Mem::default();
    rando::build(
        &files,
        &s,
        &model,
        rando::Job {
            seed: 4,
            options: &options,
            enemies: &rando::EnemyOptions::default(),
        },
        &sink,
        &|_| {},
    )
    .unwrap();
    let written = sink.0.into_inner().unwrap();
    let get = |p: &str| &written.iter().find(|(q, _)| q == p).unwrap().1;
    let key = fs_param::crypto::find_key(
        &std::fs::read(dir.join("eldenring.exe")).unwrap(),
        &std::fs::read(dir.join("regulation.bin")).unwrap(),
    )
    .unwrap();
    let reg = fs_param::Regulation::read(&key, get("regulation.bin")).unwrap();
    let goods = reg.table("EquipParamGoods").unwrap();
    let kindling = goods.rows.iter().find(|r| r.id == 2_008_021).unwrap();
    for k in 1..=5 {
        let row = goods
            .rows
            .iter()
            .find(|r| r.id == items::kindling::id(k))
            .unwrap();
        assert_eq!(row.data, kindling.data);
    }
    let text = gamedata::sources::text_of(get("msg/engus/item_dlc02.msgbnd.dcx")).unwrap();
    assert_eq!(
        text["GoodsName"]
            .get(&items::kindling::id(2))
            .map(String::as_str),
        Some("Messmer's Kindling (2/5)")
    );
    assert!(text["GoodsCaption"].contains_key(&items::kindling::id(5)));
    let common =
        Emevd::read(&fs_dcx::decompress(get("event/common.emevd.dcx")).unwrap().1).unwrap();
    assert!(common.events.iter().any(|e| e.id == COUNT_EVENT));
}

//! The whole map from the start, on the player's own game (`APOC_ER_STOCK`; skipped
//! when unset): every piece's flag is read, our event is free, the script reads back as
//! built with nothing else changed, and a seed asking for it writes it.

use std::path::PathBuf;

use fs_emevd::Emevd;
use gamedata::Sources;
use rando::whole_map::{self, EVENT};

fn stock() -> Option<PathBuf> {
    std::env::var_os("APOC_ER_STOCK").map(PathBuf::from)
}

#[test]
fn every_piece_is_drawn_from_the_start_and_nothing_else_changes() {
    let Some(dir) = stock() else {
        eprintln!("APOC_ER_STOCK is not set; skipping");
        return;
    };
    let s = Sources::open(&dir).unwrap();
    let flags = whole_map::piece_flags(&s.reg).unwrap();
    assert_eq!(flags.len(), 34, "{flags:?}");
    assert!(
        flags.iter().all(|f| (62000..62100).contains(f)),
        "{flags:?}"
    );
    assert!(flags.contains(&62010) && flags.contains(&62084));
    assert!(
        !s.common.events.iter().any(|e| e.id == EVENT),
        "event {EVENT} is free"
    );

    let out = whole_map::apply(&s.common, &s.reg).unwrap();
    let back = Emevd::read(&out.write().unwrap()).unwrap();
    assert_eq!(back, out, "reads back as built");
    // Only event 0 gains a start, and only ours is new.
    assert_eq!(out.events.len(), s.common.events.len() + 1);
    for (before, after) in s.common.events.iter().zip(&out.events) {
        if before.id == 0 {
            // Ours first, as event 0 can end early on flag 2052.
            assert_eq!(after.instructions[1..], before.instructions[..]);
            assert_eq!(
                (after.instructions[0].bank, after.instructions[0].id),
                (2000, 0)
            );
            assert_eq!(after.instructions.len(), before.instructions.len() + 1);
        } else {
            assert_eq!(after, before);
        }
    }
    let ours = out.events.last().unwrap();
    assert_eq!((ours.id, ours.instructions.len()), (EVENT, flags.len()));
}

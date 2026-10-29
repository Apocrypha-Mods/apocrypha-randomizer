//! The instruction catalogue's claims, re-checked on the player's own event
//! scripts (`APOC_ER_STOCK`; skipped when unset).
//!
//! Scripts are found by name (`/event/mAA_BB_CC_DD.emevd.dcx` and the common
//! ones): 544 of the game's 589, enough for every claim here.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use fs_dvdbnd::Dvd;
use fs_emevd::Emevd;
use fs_emevd::catalogue::{self, ArgKind};

struct Game {
    /// `common_func` first, then the rest.
    scripts: Vec<Emevd>,
    reg: fs_param::Regulation,
    text: HashMap<String, HashSet<u32>>,
}

fn game() -> Option<Game> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    let dvd = Dvd::open_elden_ring(&dir).unwrap();
    let mut names = vec![
        "/event/common_func.emevd.dcx".to_string(),
        "/event/common.emevd.dcx".to_string(),
    ];
    for a in 10..62 {
        for b in 0..100 {
            for c in 0..100 {
                for d in 0..4 {
                    names.push(format!("/event/m{a:02}_{b:02}_{c:02}_{d:02}.emevd.dcx"));
                }
            }
        }
    }
    assert!(
        dvd.find(&names[0]).is_some(),
        "common_func comes first, and is the shared script"
    );
    let scripts: Vec<Emevd> = names
        .iter()
        .filter(|p| dvd.find(p).is_some())
        .map(|p| Emevd::read(&fs_dcx::decompress(&dvd.read_path(p).unwrap()).unwrap().1).unwrap())
        .collect();
    let stored = dvd.read_path("/msg/engus/item_dlc02.msgbnd.dcx").unwrap();
    let bnd = fs_bnd4::Bnd4::read(&fs_dcx::decompress(&stored).unwrap().1).unwrap();
    let mut text: HashMap<String, HashSet<u32>> = HashMap::new();
    for f in &bnd.files {
        let Some(stem) = f.short_name().and_then(|n| n.strip_suffix(".fmg")) else {
            continue;
        };
        let base = stem
            .trim_end_matches("_dlc01")
            .trim_end_matches("_dlc02")
            .to_string();
        for (id, t) in fs_fmg::Fmg::read(&f.data).unwrap().entries {
            if t.is_some_and(|t| !t.is_empty()) {
                text.entry(base.clone()).or_default().insert(id as u32);
            }
        }
    }
    Some(Game {
        scripts,
        reg: fs_param::Regulation::open(&dir).unwrap(),
        text,
    })
}

#[test]
fn every_catalogue_claim_holds() {
    let Some(g) = game() else {
        eprintln!("APOC_ER_STOCK is not set; skipping");
        return;
    };
    let refs: Vec<&Emevd> = g.scripts.iter().collect();
    let common = Some(&g.scripts[0]);
    let event_ids: HashSet<u32> = g
        .scripts
        .iter()
        .flat_map(|e| e.events.iter().map(|ev| ev.id as u32))
        .collect();
    for e in catalogue::elden_ring() {
        let uses: Vec<_> = g
            .scripts
            .iter()
            .flat_map(|s| &s.events)
            .flat_map(|ev| &ev.instructions)
            .filter(|i| (i.bank, i.id) == (e.bank, e.id))
            .collect();
        assert!(!uses.is_empty(), "{} is never used", e.name);
        if let Some(len) = e.length {
            assert!(
                uses.iter().all(|i| i.args.len() == len),
                "{}: not every use is {len} bytes",
                e.name
            );
        }
        for a in &e.args {
            let vals: HashSet<u32> = catalogue::values(&refs, common, e.bank, e.id, a.word)
                .into_iter()
                .collect();
            let nonzero: HashSet<u32> = vals
                .iter()
                .copied()
                .filter(|&v| v != 0 && v != u32::MAX)
                .collect();
            let share = |set: &HashSet<u32>| {
                100.0 * nonzero.iter().filter(|v| set.contains(v)).count() as f64
                    / nonzero.len().max(1) as f64
            };
            match &a.kind {
                ArgKind::State => assert!(
                    vals.iter().all(|&v| v <= 1),
                    "{}.{}: {vals:?}",
                    e.name,
                    a.name
                ),
                ArgKind::EventId => assert!(
                    share(&event_ids) >= 95.0,
                    "{}.{}: {:.0}%",
                    e.name,
                    a.name,
                    share(&event_ids)
                ),
                ArgKind::Param(t) => {
                    let rows: HashSet<u32> = g
                        .reg
                        .table(t)
                        .unwrap()
                        .rows
                        .iter()
                        .map(|r| r.id as u32)
                        .collect();
                    eprintln!(
                        "{}.{}: {:.0}% of {} values are {t} rows",
                        e.name,
                        a.name,
                        share(&rows),
                        nonzero.len()
                    );
                    assert!(
                        share(&rows) >= 90.0,
                        "{}.{}: {:.0}% {t}",
                        e.name,
                        a.name,
                        share(&rows)
                    );
                }
                ArgKind::Text(t) => {
                    eprintln!(
                        "{}.{}: {:.0}% of {} values are {t} ids",
                        e.name,
                        a.name,
                        share(&g.text[t]),
                        nonzero.len()
                    );
                    assert!(
                        share(&g.text[t]) >= 95.0,
                        "{}.{}: {:.0}% {t}",
                        e.name,
                        a.name,
                        share(&g.text[t])
                    );
                }
                _ => {}
            }
        }
    }
}

/// What each byte of a packed word is said to hold, on every use the scripts
/// write whole (placeholders are filled only when an event starts).
#[test]
fn every_packed_byte_claim_holds() {
    let Some(g) = game() else {
        eprintln!("APOC_ER_STOCK is not set; skipping");
        return;
    };
    let events = || g.scripts.iter().flat_map(|s| &s.events);
    for e in catalogue::elden_ring() {
        for a in e.args.iter().filter(|a| !a.bytes.is_empty()) {
            for (k, name) in a.bytes.iter().enumerate() {
                let at = a.word * 4 + k;
                let (mut n, mut good) = (0, 0);
                for ev in events() {
                    for (j, ins) in ev.instructions.iter().enumerate() {
                        let filled = ev.parameters.iter().any(|p| {
                            p.instruction == j as i64
                                && (p.target..p.target + p.size as i64).contains(&(at as i64))
                        });
                        if (ins.bank, ins.id) != (e.bank, e.id) || filled {
                            continue;
                        }
                        let b = ins.args[at];
                        n += 1;
                        good += match name.as_str() {
                            "lines" => j + 1 + b as usize <= ev.instructions.len(),
                            "label" => ev.instructions[j + 1..]
                                .iter()
                                .any(|l| l.bank == 1014 && l.id == b as u32),
                            "group" | "source" => (-15..=15).contains(&(b as i8)),
                            "state" | "flag_kind" => b <= 3,
                            "end" => b <= 1,
                            other => panic!("{}: no such byte as {other}", e.name),
                        } as usize;
                    }
                }
                let share = 100.0 * good as f64 / n.max(1) as f64;
                eprintln!("{}.{name}: {share:.1}% of {n}", e.name);
                let floor = if name == "label" { 98.0 } else { 100.0 };
                assert!(share >= floor, "{}.{name}: {share:.1}% of {n}", e.name);
            }
        }
    }
    // Every condition keeps its group in byte 0.
    for ins in events().flat_map(|ev| &ev.instructions) {
        if ins.bank < 1000 {
            assert!((-15..=15).contains(&(ins.args[0] as i8)), "{ins:?}");
        }
    }
}

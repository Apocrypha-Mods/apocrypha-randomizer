//! The audit of gifts that may take a key item. The model reads what a talk
//! script asks on the way to an award from the conditions that pass. This reads
//! the conditions that had to fail too: a gift behind "flag X off" is lost for
//! good if X comes on first. It prints what needs reading by hand.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_emevd::catalogue;
use fs_esd::calls::resolve_strictly;
use fs_esd::commands::{AWARD_ITEM_LOT, SET_EVENT_FLAG};
use fs_esd::cond::Atom;
use gamedata::model::Kind;

pub fn run(game: PathBuf) -> Result<()> {
    let s = gamedata::Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    // Every way to each lot, and who in talk turns each flag on.
    let mut ways: BTreeMap<i32, Vec<(String, BTreeSet<Atom>)>> = BTreeMap::new();
    let mut talk_on: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    let mut talk_asks: BTreeMap<u32, BTreeSet<BTreeSet<Atom>>> = BTreeMap::new();
    for (_, name, esd) in &s.talk {
        for r in resolve_strictly(esd) {
            match ((r.bank, r.id), &r.args[..]) {
                (AWARD_ITEM_LOT, [Some(lot), ..]) if *lot > 0 => {
                    ways.entry(*lot)
                        .or_default()
                        .push((name.clone(), r.needs.clone()));
                }
                (SET_EVENT_FLAG, [Some(flag), Some(1)]) => {
                    if let Ok(f) = u32::try_from(*flag) {
                        talk_on.entry(f).or_default().insert(name.clone());
                        talk_asks.entry(f).or_default().insert(r.needs.clone());
                    }
                }
                _ => {}
            }
        }
    }
    // Which flags events turn on, in each run with its arguments copied in.
    let mut event_on: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    let setters = [catalogue::named("set_flag"), catalogue::named("set_flag_b")];
    let mut scripts: Vec<(String, &fs_emevd::Emevd)> =
        s.scripts.iter().map(|(m, e)| (m.to_string(), e)).collect();
    scripts.push(("common".to_string(), &s.common));
    for (map, script) in scripts {
        for run in catalogue::runs(script, Some(&s.common_func)) {
            for i in &run.instructions {
                let Some(e) = setters.iter().find(|e| (e.bank, e.id) == (i.bank, i.id)) else {
                    continue;
                };
                let word = |name| catalogue::Entry::word(i, e.arg(name).word);
                if word("state") == Some(1)
                    && let Some(f) = word("flag")
                {
                    event_on.entry(f).or_default().insert(map.clone());
                }
            }
        }
    }
    let got: BTreeMap<u32, String> = model
        .locations
        .iter()
        .filter_map(|l| Some((l.flag?, l.source.clone())))
        .collect();
    let mut counted = 0;
    for l in model.locations.iter().filter(|l| l.kind == Kind::Gift) {
        let Some(lot) = l.lot.as_ref().and_then(|lot| lot.ids.first()) else {
            continue;
        };
        if l.missable || l.given_if.as_ref().is_some_and(Vec::is_empty) {
            continue;
        }
        counted += 1;
        let names: Vec<_> = l.items.iter().filter_map(|i| i.name.clone()).collect();
        println!(
            "\nlot {lot} {} {:?} flag {:?} {:?}",
            l.source, names, l.flag, l.given_if
        );
        let mut distinct = BTreeSet::new();
        for (_, needs) in ways.get(lot).into_iter().flatten() {
            distinct.insert(needs.clone());
        }
        // A flag every way needs off is one that can shut the gift.
        let mut all_off: Option<BTreeSet<Atom>> = None;
        for needs in &distinct {
            let here: BTreeSet<Atom> = needs
                .iter()
                .filter(|a| matches!(a, Atom::Flag(_, false) | Atom::HoldsNone(..)))
                .copied()
                .collect();
            all_off = Some(match all_off {
                None => here,
                Some(a) => a.intersection(&here).copied().collect(),
            });
        }
        println!("  {} ways", distinct.len());
        for a in all_off.unwrap_or_default() {
            match a {
                Atom::Flag(f, _) if Some(f) == l.flag => {}
                Atom::Flag(f, _) => println!(
                    "  off {f}: talk {:?} events {:?} got-by {:?}",
                    talk_on.get(&f),
                    event_on.get(&f),
                    got.get(&f)
                ),
                other => println!("  {other:?}"),
            }
            if let Atom::Flag(f, _) = a {
                for asks in talk_asks.get(&f).into_iter().flatten().take(4) {
                    println!("    set when {asks:?}");
                }
            }
        }
    }
    println!("\n{counted} gifts counted");
    Ok(())
}

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

/// Every part whose talk script opens the remembrance list and what stands nearest
/// it, and each model whose animations hold `value`: the two ways the scripts name
/// what carries a shop and what applies a grab.
pub fn name_models(game: PathBuf, value: u32) -> Result<()> {
    use fs_msb::layout::{entry_name, field};
    const PARTS: &str = "PARTS_PARAM_ST";
    const MODELS: &str = "MODEL_PARAM_ST";
    let s = gamedata::Sources::open(&game)?;
    let files = gamedata::GameFiles::open(&game, &[])?;
    let mut talks = BTreeSet::new();
    for (_, name, esd) in &s.talk {
        if fs_esd::calls::resolve(esd)
            .iter()
            .any(|r| (r.bank, r.id) == fs_esd::commands::OPEN_EXCHANGE)
        {
            talks.insert(name.clone());
        }
    }
    println!("talks {talks:?}");
    let get = |e: &[u8], name: &str| field(PARTS, name).get(e, 0).unwrap_or(0);
    let mut seen = BTreeSet::new();
    for (m, msb) in &s.maps {
        let models = &msb.table(MODELS).entries;
        let model_of = |e: &[u8]| {
            usize::try_from(get(e, "model"))
                .ok()
                .and_then(|i| models.get(i))
                .and_then(|me| entry_name(me))
                .unwrap_or_default()
        };
        let pos = |e: &[u8]| {
            let f = field(PARTS, "position");
            [0, 1, 2].map(|i| f.get_f32(e, i).unwrap_or(f32::NAN))
        };
        let parts = &msb.table(PARTS).entries;
        for e in parts {
            let name = model_of(e);
            if name.starts_with('c') {
                seen.insert(name.clone());
            }
            let talk = format!("t{:09}", get(e, "talk_id"));
            if !talks.contains(&talk) {
                continue;
            }
            let here = pos(e);
            let mut near: Vec<(f32, String)> = parts
                .iter()
                .filter(|o| model_of(o).starts_with('c'))
                .map(|o| {
                    let p = pos(o);
                    let d = (0..3).map(|i| (p[i] - here[i]).powi(2)).sum::<f32>().sqrt();
                    (d, model_of(o))
                })
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            near.truncate(5);
            println!("{m} {talk} {name} at {here:?} near {near:?}");
        }
    }
    let bytes = value.to_le_bytes();
    for model in seen {
        let path = format!("/chr/{model}.anibnd.dcx");
        if !files.exists(&path) {
            continue;
        }
        let Ok(raw) = files.read(&path) else { continue };
        let Ok((_, plain)) = fs_dcx::decompress(&raw) else {
            continue;
        };
        let uses = plain.windows(4).filter(|w| *w == bytes).count();
        if uses > 0 {
            println!("{model}: {uses} uses of {value} in its animations");
        }
    }
    Ok(())
}

/// Where each of `flags` appears in any form, and what stands on each way an event
/// turns it on: what keeps a gift shut.
pub fn blockers(game: PathBuf, flags: &[u32]) -> Result<()> {
    let s = gamedata::Sources::open(&game)?;
    let setters = [catalogue::named("set_flag"), catalogue::named("set_flag_b")];
    let range = catalogue::named("set_flag_range");
    let mut scripts: Vec<(String, &fs_emevd::Emevd)> =
        s.scripts.iter().map(|(m, e)| (m.to_string(), e)).collect();
    scripts.push(("common".to_string(), &s.common));
    for &f in flags {
        println!("\n== {f}");
        let bytes = f.to_le_bytes();
        let has = |b: &[u8]| b.windows(4).any(|w| w == bytes);
        for name in s.reg.table_names().map(String::from).collect::<Vec<_>>() {
            if let Ok(t) = s.reg.table(&name) {
                for r in t.rows.iter().filter(|r| has(&r.data)) {
                    println!("  param {name} row {}", r.id);
                }
            }
        }
        for (_, name, esd) in &s.talk {
            let uses = esd.expressions.windows(4).filter(|w| *w == bytes).count();
            if uses > 0 {
                println!("  talk {name}: {uses} uses in expressions");
                for c in &esd.commands {
                    let args = esd.command_args(c);
                    let values: Vec<_> = args
                        .iter()
                        .map(|a| a.and_then(fs_esd::expr::constant))
                        .collect();
                    if values.contains(&Some(f as i32)) {
                        println!("    command {}:{:#x} {values:?}", c.bank, c.id);
                    }
                }
                for r in fs_esd::calls::resolve(esd) {
                    if r.args.contains(&Some(f as i32)) {
                        println!(
                            "    call {}:{} {:?} needs {:?}",
                            r.bank, r.id, r.args, r.needs
                        );
                    }
                }
            }
        }
        for (map, script) in &scripts {
            for run in catalogue::runs(script, Some(&s.common_func)) {
                for i in &run.instructions {
                    if (i.bank, i.id) == (range.bank, range.id) {
                        let word = |name| catalogue::Entry::word(i, range.arg(name).word);
                        if let (Some(a), Some(b), Some(1)) =
                            (word("first"), word("last"), word("state"))
                            && (a..=b).contains(&f)
                        {
                            println!("  {map}: range {a}..{b} set on");
                        }
                    }
                }
                let sets = |i: &fs_emevd::Instruction| {
                    setters.iter().any(|e| {
                        (e.bank, e.id) == (i.bank, i.id)
                            && catalogue::Entry::word(i, e.arg("flag").word) == Some(f)
                            && catalogue::Entry::word(i, e.arg("state").word) == Some(1)
                    })
                };
                for (_, ways) in fs_emevd::flow::ways_to(&run.instructions, sets) {
                    for way in ways.iter().take(3) {
                        let tests: Vec<String> = way
                            .iter()
                            .map(|l| format!("{}{:?}", if l.holds { "" } else { "!" }, l.test))
                            .collect();
                        println!("  {map}: set after {}", tests.join(" & "));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Every gift no way can reach, with what each of its ways stands on: the gifts
/// still shut, and why.
pub fn shut(game: PathBuf) -> Result<()> {
    use gamedata::model::Cond;
    let s = gamedata::Sources::open(&game)?;
    let m = gamedata::elden_ring::extract(&s)?;
    let mut count = 0;
    let (mut missable, mut gifts) = (0, 0);
    for l in m.locations.iter().filter(|l| l.kind == Kind::Gift) {
        gifts += 1;
        missable += usize::from(l.missable);
        let Some(ways) = &l.given_if else { continue };
        let open = |w: &Vec<Cond>| !w.iter().any(|c| matches!(c, Cond::Flag(_)));
        if ways.iter().any(open) {
            continue;
        }
        count += 1;
        let lot = l.lot.as_ref().and_then(|t| t.ids.first().copied());
        println!("lot {lot:?} missable {} ways {ways:?}", l.missable);
    }
    println!("shut {count} of {gifts} gifts, {missable} missable");
    Ok(())
}

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

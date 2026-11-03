//! Statistics for every instruction in the event scripts: how often it is
//! used, its argument lengths, and for each 4-byte word what its values look
//! like and which known value sets they fall in. The raw material for the
//! instruction catalogue, the same way `columns` is for param layouts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_emevd::Emevd;
use fs_param::{Regulation, layout};

/// A named set of known values a word's values can be checked against.
pub struct Known {
    pub name: String,
    pub values: HashSet<u32>,
}

pub fn known_sets(
    reg: &Regulation,
    scripts: &[(String, Emevd)],
    text: &HashMap<String, HashMap<i32, String>>,
) -> Result<Vec<Known>> {
    let mut sets = Vec::new();
    sets.push(Known {
        name: "event id".into(),
        values: scripts
            .iter()
            .flat_map(|(_, e)| e.events.iter().map(|ev| ev.id as u32))
            .collect(),
    });
    let mut flags = HashSet::new();
    for l in layout::elden_ring() {
        for f in l.fields.iter().filter(|f| f.name.contains("flag")) {
            for t in &l.tables {
                for r in &reg.table(t)?.rows {
                    for i in 0..f.count {
                        flags.insert(f.get(&r.data, i) as u32);
                    }
                }
            }
        }
    }
    flags.retain(|&v| v != 0 && v != u32::MAX);
    sets.push(Known {
        name: "param flag".into(),
        values: flags,
    });
    for (name, t) in text {
        if t.len() >= 20 {
            sets.push(Known {
                name: format!("text {name}"),
                values: t.keys().map(|&k| k as u32).collect(),
            });
        }
    }
    for t in reg.table_names() {
        let p = reg.table(t)?;
        if p.rows.len() >= 20 {
            sets.push(Known {
                name: t.to_string(),
                values: p.rows.iter().map(|r| r.id as u32).collect(),
            });
        }
    }
    Ok(sets)
}

#[derive(Default)]
struct Stats {
    count: usize,
    lengths: BTreeMap<usize, usize>,
    words: BTreeMap<usize, HashMap<u32, usize>>,
}

/// The maps' entity ids, as sets a word's values can fall in: enemies (part
/// types 2 and 10), every other part, regions, and the entity groups parts
/// belong to. Regions and groups are read from raw words of the entries, not
/// from `fs_msb`'s layout: a region's id is word 1 of the section whose offset
/// it holds at +0x50 (7,464 set over 46,420 regions, ids shaped like enemies'),
/// and a part's groups are the eight words from +0x1C of its entity section
/// (+0x60), where enemies hold ids such as 1042375810 shared by many parts. A map
/// event's id is word 2 of the section whose offset it holds at +0x18, or for an
/// object action (type 7, which 5[2] names) word 0 of its data at +0x20; these
/// count with the parts.
pub fn entity_sets(maps: &[(String, fs_msb::Msb)]) -> Vec<Known> {
    use fs_msb::layout::field;
    let section = |e: &[u8], at: usize| -> Option<usize> {
        let b = i64::from_le_bytes(e.get(at..at + 8)?.try_into().unwrap());
        usize::try_from(b).ok().filter(|&b| b > 0)
    };
    let word = |e: &[u8], at: usize| -> Option<u32> {
        e.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let (mut enemies, mut parts, mut regions, mut groups) = (
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
    );
    for (_, m) in maps {
        for e in &m.table("PARTS_PARAM_ST").entries {
            let ty = field("PARTS_PARAM_ST", "type").get(e, 0).unwrap_or(-1);
            if let Some(id) = field("PARTS_PARAM_ST", "entity_id").get(e, 0) {
                if [2, 10].contains(&ty) {
                    enemies.insert(id as u32);
                } else {
                    parts.insert(id as u32);
                }
            }
            if let Some(s) = section(e, 0x60) {
                groups.extend((0..8).filter_map(|k| word(e, s + 0x1C + k * 4)));
            }
        }
        for e in &m.table("POINT_PARAM_ST").entries {
            if let Some(s) = section(e, 0x50) {
                regions.extend(word(e, s + 4));
            }
        }
        for e in &m.table("EVENT_PARAM_ST").entries {
            if let Some(s) = section(e, 0x18) {
                parts.extend(word(e, s + 8));
            }
            // An object action (type 7) holds its own id first in its data (+0x20).
            if word(e, 0x0C) == Some(7)
                && let Some(s) = section(e, 0x20)
            {
                parts.extend(word(e, s));
            }
        }
    }
    [
        ("map enemy", enemies),
        ("map part", parts),
        ("map region", regions),
        ("map group", groups),
    ]
    .into_iter()
    .map(|(name, mut values)| {
        values.retain(|&v| v != 0 && v != u32::MAX);
        Known {
            name: name.into(),
            values,
        }
    })
    .collect()
}

pub fn run(game: PathBuf, cache: PathBuf, only: Option<String>, maps: bool) -> Result<()> {
    let reg = Regulation::open(&game)?;
    let scripts = crate::scripts::load(&game, &cache)?;
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(&game)?;
    let text = gamedata::sources::text(&dvd)?;
    let mut sets = known_sets(&reg, &scripts, &text)?;
    if maps {
        sets.extend(entity_sets(&crate::msb_columns::load_maps(&dvd)));
    }

    let mut stats: BTreeMap<(u32, u32), Stats> = BTreeMap::new();
    let common_hash = format!(
        "{:016x}",
        fs_dvdbnd::hash_path("/event/common_func.emevd.dcx")
    );
    let common = scripts
        .iter()
        .find(|(l, _)| l.ends_with(&common_hash))
        .map(|(_, e)| e);
    let resolved = fs_emevd::catalogue::resolve_calls(scripts.iter().map(|(_, e)| e), common);
    for (_, e) in &scripts {
        for ev in &e.events {
            // Words that parameters fill in are placeholders; their real values
            // come from the calls that start the event (below).
            let filled: HashSet<(i64, i64)> = ev
                .parameters
                .iter()
                .flat_map(|p| {
                    (0..i64::from(p.size) / 4).map(move |k| (p.instruction, p.target / 4 + k))
                })
                .collect();
            for (i, ins) in ev.instructions.iter().enumerate() {
                let s = stats.entry((ins.bank, ins.id)).or_default();
                s.count += 1;
                *s.lengths.entry(ins.args.len()).or_default() += 1;
                for (w, word) in ins.args.chunks_exact(4).enumerate() {
                    if filled.contains(&(i as i64, w as i64)) {
                        continue;
                    }
                    *s.words
                        .entry(w)
                        .or_default()
                        .entry(u32::from_le_bytes(word.try_into().unwrap()))
                        .or_default() += 1;
                }
            }
        }
    }
    for f in &resolved {
        if let Some(s) = stats.get_mut(&(f.bank, f.id)) {
            *s.words
                .entry(f.word)
                .or_default()
                .entry(f.value)
                .or_default() += 1;
        }
    }
    println!(
        "{} placeholder words filled in from the calls that start their events",
        resolved.len()
    );
    let only: Option<HashSet<(u32, u32)>> = only.map(|o| {
        o.split(',')
            .filter_map(|s| s.split_once(':'))
            .map(|(b, i)| (b.parse().unwrap(), i.parse().unwrap()))
            .collect()
    });
    for ((bank, id), s) in &stats {
        if only.as_ref().is_some_and(|o| !o.contains(&(*bank, *id))) {
            continue;
        }
        let name = fs_emevd::catalogue::lookup(*bank, *id)
            .map(|e| format!(" ({})", e.name))
            .unwrap_or_default();
        println!(
            "{bank}[{id}]{name} used {} times, argument lengths {:?}",
            s.count, s.lengths
        );
        for (w, vals) in &s.words {
            println!("    word {w}: {}", describe(vals, &sets));
        }
    }
    Ok(())
}

/// One word's values: spread, shape, and the known sets they fall in.
pub fn describe(vals: &HashMap<u32, usize>, sets: &[Known]) -> String {
    let total: usize = vals.values().sum();
    let zeros = vals.get(&0).copied().unwrap_or(0);
    let mut top: Vec<(u32, usize)> = vals.iter().map(|(&v, &c)| (v, c)).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let shown: Vec<String> = top
        .iter()
        .take(5)
        .map(|(v, c)| format!("{}×{c}", *v as i32))
        .collect();
    let mut tags = Vec::new();
    let nonzero: Vec<u32> = vals
        .keys()
        .copied()
        .filter(|&v| v != 0 && v != u32::MAX)
        .collect();
    let floats = nonzero.iter().filter(|&&v| {
        let f = f32::from_bits(v);
        f.is_normal() && (1e-3..1e6).contains(&f.abs())
    });
    if !nonzero.is_empty() && floats.count() * 10 >= nonzero.len() * 9 {
        tags.push("float".to_string());
    }
    if nonzero.len() >= 3 {
        let mut best: Vec<(usize, &str)> = sets
            .iter()
            .map(|k| {
                (
                    nonzero.iter().filter(|v| k.values.contains(v)).count(),
                    k.name.as_str(),
                )
            })
            .filter(|(n, _)| n * 100 >= nonzero.len() * 70)
            .collect();
        best.sort_by_key(|b| std::cmp::Reverse(b.0));
        for (n, name) in best.iter().take(3) {
            tags.push(format!("{name} {}%", n * 100 / nonzero.len()));
        }
    }
    if vals.keys().all(|&v| v < 0x0100_0000)
        && vals.keys().any(|&v| v > 0xFF)
        && vals.len() <= 64
        && vals.keys().all(|v| v & 0xFF <= 8)
    {
        tags.push("packed bytes?".to_string());
    }
    format!(
        "{} values, {} distinct, {} zero; [{}]{}",
        total,
        vals.len(),
        zeros,
        shown.join(" "),
        if tags.is_empty() {
            String::new()
        } else {
            format!("  {}", tags.join(", "))
        }
    )
}

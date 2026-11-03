//! What the model does not reach, sorted: treasures whose lot is missing or
//! empty, and the item lots nothing gives, each with what the files say of it
//! (its flag found in a script, a talk script or another table; its items got
//! elsewhere in the model or nowhere).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_emevd::catalogue;
use fs_msb::layout::{entry_name, field};
use fs_param::layout::for_table;
use gamedata::{Sources, elden_ring};

/// Every 4-byte word, aligned, in every row of every table but the lot tables.
fn param_words(s: &Sources) -> HashMap<u32, BTreeSet<String>> {
    let mut out: HashMap<u32, BTreeSet<String>> = HashMap::new();
    let names: Vec<String> = s.reg.table_names().map(String::from).collect();
    for name in names {
        if name.starts_with("ItemLotParam") {
            continue;
        }
        let Ok(t) = s.reg.table(&name) else { continue };
        for r in &t.rows {
            for w in r.data.chunks_exact(4) {
                let v = u32::from_le_bytes(w.try_into().unwrap());
                if v > 1000 {
                    let e = out.entry(v).or_default();
                    if e.len() < 3 {
                        e.insert(format!("{name} {}", r.id));
                    }
                }
            }
        }
    }
    out
}

pub fn run(game: PathBuf, all: bool) -> Result<()> {
    let s = Sources::open(&game)?;
    let model = elden_ring::extract(&s)?;

    // Every word any event script holds, and every 4-byte window of talk.
    let mut script_words: HashSet<u32> = HashSet::new();
    let mut scripts: Vec<&fs_emevd::Emevd> = s.scripts.iter().map(|(_, e)| e).collect();
    scripts.push(&s.common);
    scripts.push(&s.common_func);
    for e in &scripts {
        for run in catalogue::runs(e, Some(&s.common_func)) {
            for i in &run.instructions {
                for w in i.args.chunks_exact(4) {
                    script_words.insert(u32::from_le_bytes(w.try_into().unwrap()));
                }
            }
        }
    }
    let mut talk_words: HashMap<u32, String> = HashMap::new();
    for (_, name, esd) in &s.talk {
        for w in esd.expressions.windows(4) {
            talk_words
                .entry(u32::from_le_bytes(w.try_into().unwrap()))
                .or_insert_with(|| name.clone());
        }
    }
    let params = param_words(&s);

    // Treasures whose lot is not a row, or holds nothing.
    let lot_layout = for_table("ItemLotParam_map").unwrap();
    let rows: HashMap<&str, HashMap<i32, Vec<u8>>> = ["ItemLotParam_map", "ItemLotParam_enemy"]
        .into_iter()
        .map(|t| {
            (
                t,
                s.reg
                    .table(t)
                    .unwrap()
                    .rows
                    .into_iter()
                    .map(|r| (r.id, r.data))
                    .collect(),
            )
        })
        .collect();
    let items_of = |data: &[u8]| -> Vec<(i64, i64)> {
        let (id, cat) = (
            lot_layout.field("item_id"),
            lot_layout.field("item_category"),
        );
        (0..id.count)
            .map(|i| (cat.get(data, i), id.get(data, i)))
            .filter(|&(c, i)| c > 0 && i > 0)
            .collect()
    };
    let flag_of = |data: &[u8]| lot_layout.field("get_item_flag").get(data, 0) as u32;
    let ev = |name| field("EVENT_PARAM_ST", name);
    let mut missing: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut empty: Vec<String> = Vec::new();
    for (map, msb) in &s.maps {
        for e in &msb.table("EVENT_PARAM_ST").entries {
            if ev("type").get(e, 0) != Some(4) {
                continue;
            }
            let lot = ev("item_lot").get(e, 0).unwrap_or(-1);
            let name = format!("{map} {}", entry_name(e).unwrap_or_default());
            match rows["ItemLotParam_map"].get(&(lot as i32)) {
                None => missing.entry(lot).or_default().push(name),
                Some(d) if items_of(d).is_empty() => {
                    empty.push(format!("{name} lot {lot} flag {}", flag_of(d)))
                }
                Some(_) => {}
            }
        }
    }
    let n: usize = missing.values().map(Vec::len).sum();
    println!("treasures whose lot is not a row: {n}");
    for (lot, at) in &missing {
        let enemy = rows["ItemLotParam_enemy"].contains_key(&(*lot as i32));
        println!(
            "  lot {lot}{}: {} ({})",
            if enemy { " (an enemy lot's id)" } else { "" },
            at.len(),
            at.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
        );
    }
    println!("\ntreasures whose lot holds no item: {}", empty.len());
    for e in &empty {
        println!("  {e}");
    }

    // Items the model gives somewhere, by (lot category, id).
    let given: HashSet<(String, i32)> = model
        .locations
        .iter()
        .flat_map(|l| &l.items)
        .map(|i| (format!("{:?}", i.kind), i.id))
        .collect();
    let kind_name = |c: i64| match c {
        1 => "Goods",
        2 => "Weapon",
        3 => "Protector",
        4 => "Accessory",
        5 => "Gem",
        6 => "CustomWeapon",
        _ => "?",
    };
    let text_table = |c: i64| match c {
        1 => "GoodsName",
        2 => "WeaponName",
        3 => "ProtectorName",
        4 => "AccessoryName",
        5 => "GemName",
        _ => "",
    };
    let name_of = |c: i64, id: i64| {
        let id = id as i32;
        let base = if c == 2 { id - id % 10_000 } else { id };
        s.text(text_table(c), id)
            .or_else(|| s.text(text_table(c), base))
            .map(String::from)
    };
    let goods_type: HashMap<i32, i64> = {
        let l = for_table("EquipParamGoods").unwrap();
        s.reg
            .table("EquipParamGoods")?
            .rows
            .iter()
            .map(|r| (r.id, l.field("goods_type").get(&r.data, 0)))
            .collect()
    };

    // NPCs' own map lots (the 100000s): which NpcParam rows name one, and
    // whether a map places that row.
    let npc = for_table("NpcParam").unwrap();
    let placed: HashSet<i32> = model.enemies.iter().map(|e| e.npc_param).collect();
    let mut npc_lots: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    for r in &s.reg.table("NpcParam")?.rows {
        let lot = npc.field("item_lot_map_id").get(&r.data, 0) as i32;
        if (100_000..120_000).contains(&lot) {
            npc_lots.entry(lot).or_default().push(format!(
                "{}{}",
                r.id,
                if placed.contains(&r.id) {
                    ""
                } else {
                    " (unplaced)"
                }
            ));
        }
    }
    println!(
        "\nNpcParam rows naming a map lot in the 100000s: {}",
        npc_lots.len()
    );
    let unreached: HashSet<i32> = model.gaps.unreached_lots["ItemLotParam_map"]
        .iter()
        .copied()
        .collect();
    for (lot, rows) in &npc_lots {
        println!(
            "  {lot}{}: {}",
            if unreached.contains(lot) {
                " (unreached)"
            } else {
                ""
            },
            rows.join(", ")
        );
    }

    let maps: HashSet<(u8, u8, u8)> = s
        .maps
        .iter()
        .map(|(m, _)| (m.0[0], m.0[1], m.0[2]))
        .collect();
    for (table, ids) in &model.gaps.unreached_lots {
        println!(
            "\n{table}: {} lots with items that nothing gives",
            ids.len()
        );
        // (flagged, named) -> group -> lines
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in ids {
            let d = &rows[table.as_str()][id];
            let items = items_of(d);
            let flag = flag_of(d);
            let named = items.iter().any(|&(c, i)| name_of(c, i).is_some());
            let tutorial = items
                .iter()
                .all(|&(c, i)| c == 1 && goods_type.get(&(i as i32)) == Some(&7));
            let elsewhere = items
                .iter()
                .all(|&(c, i)| given.contains(&(kind_name(c).to_string(), i as i32)));
            let mut seen_in = Vec::new();
            if flag != 0 {
                if script_words.contains(&flag) {
                    seen_in.push("script".to_string());
                }
                if let Some(t) = talk_words.get(&flag) {
                    seen_in.push(format!("talk {t}"));
                }
                if let Some(p) = params.get(&flag) {
                    seen_in.extend(p.iter().cloned());
                }
            }
            let lot_seen = script_words.contains(&(*id as u32))
                || talk_words.contains_key(&(*id as u32))
                || params.contains_key(&(*id as u32));
            let group = format!(
                "{}: {} {} {}{}{}",
                lot_class(*id, &maps),
                if flag != 0 { "flagged" } else { "unflagged" },
                if named { "named" } else { "unnamed" },
                if tutorial {
                    "tutorial"
                } else if elsewhere {
                    "got elsewhere"
                } else {
                    "found nowhere else"
                },
                if seen_in.is_empty() {
                    ""
                } else {
                    ", flag seen"
                },
                if lot_seen { ", lot id seen" } else { "" },
            );
            let names: Vec<String> = items
                .iter()
                .map(|&(c, i)| name_of(c, i).unwrap_or_else(|| format!("{}:{i}", kind_name(c))))
                .collect();
            let mut lot_in: Vec<String> = Vec::new();
            if script_words.contains(&(*id as u32)) {
                lot_in.push("script".into());
            }
            if let Some(t) = talk_words.get(&(*id as u32)) {
                lot_in.push(format!("talk {t}"));
            }
            lot_in.extend(params.get(&(*id as u32)).into_iter().flatten().cloned());
            groups.entry(group).or_default().push(format!(
                "{id} flag {flag} {names:?}{}{}",
                if seen_in.is_empty() {
                    String::new()
                } else {
                    format!(" flag in {seen_in:?}")
                },
                if lot_in.is_empty() {
                    String::new()
                } else {
                    format!(" lot in {lot_in:?}")
                }
            ));
        }
        for (g, lines) in &groups {
            println!("  {g}: {}", lines.len());
            for l in lines.iter().take(if all { usize::MAX } else { 6 }) {
                println!("    {l}");
            }
        }
    }
    Ok(())
}

/// What a lot's id says of it ([`elden_ring::lot_map`]), and whether the map
/// it is numbered after was read.
fn lot_class(id: i32, maps: &HashSet<(u8, u8, u8)>) -> String {
    match elden_ring::lot_map(id) {
        Ok([a, b, c]) if maps.contains(&(a, b, c)) => "map read".into(),
        Ok(_) => "map not read".into(),
        Err(sort) => format!("{sort:?}"),
    }
}

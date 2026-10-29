//! Elden Ring's model, extracted from its own files with the field layouts
//! and instruction catalogue of the format crates.
//!
//! How each kind of location is found:
//! - **Treasure:** MSB treasure events (type 4): the part they sit on and the
//!   ItemLotParam_map row they give.
//! - **Enemy drops:** each placed enemy's NpcParam names an ItemLotParam_enemy
//!   row and an ItemLotParam_map row. A lot with a flag drops once; one
//!   without drops on every roll.
//! - **Gathering:** an asset's model is a row of
//!   AssetEnvironmentGeometryParam (AEG099_683 is row 99683), whose pick-up
//!   and break lots every asset of that model gives.
//! - **Events:** the lots the scripts award (`award_item_lot`, `_b`) and
//!   award on a flag (`item_lot_for_flag`, mostly a boss's defeat flag), with
//!   each event's arguments filled
//!   in from the call that starts it.
//! - **Gifts:** the lots the talk scripts award (`AWARD_ITEM_LOT`), with
//!   every call between a script's state machines followed.
//! - **Shops:** every ShopLineupParam row, placed where the characters stand
//!   whose talk scripts open a range holding it (`SHOP_RANGES`: shops,
//!   dragon communion, puppets and exchanges). A row the Twin Maiden Husks
//!   resell (bell-bearing stock) is placed with its merchant, and at the
//!   Roundtable only when nobody else sells it.
//! - **Starting:** the ten classes' CharaInitParam rows (3000–3009).
//!
//! A lot may run on into the rows after it: rows that follow a lot directly
//! and that nothing references themselves are taken as part of it. That is
//! the probable reading (treasure lots followed by such a row share its flag
//! in 50 of 63 cases) and waits on play to be confirmed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use fs_bytes::Result;
use fs_emevd::catalogue;
use fs_esd::commands::{AWARD_ITEM_LOT, SHOP_RANGES};
use fs_msb::layout::{entry_name, field};
use fs_param::layout::{Layout, for_table};
use fs_param::param::Row;

use crate::model::*;
use crate::sources::EVERYWHERE;
use crate::{MapId, Sources};

const LOT_MAP: &str = "ItemLotParam_map";
const LOT_ENEMY: &str = "ItemLotParam_enemy";
const ENEMY_TYPES: [i64; 2] = [2, 10];
const TREASURE: i64 = 4;
/// The Roundtable Hold, where the Twin Maiden Husks resell merchants' stock.
const ROUNDTABLE: MapId = MapId([11, 10, 0, 0]);
/// Assets: the part type of every map object (573,966 of them).
const ASSET: i64 = 13;
/// The ten classes' starting equipment in CharaInitParam.
const CLASSES: std::ops::RangeInclusive<i32> = 3000..=3009;

fn layout(table: &str) -> &'static Layout {
    for_table(table).unwrap_or_else(|| panic!("no layout for {table}"))
}

/// A part of some map, found by entity id.
struct Placed<'a> {
    map: MapId,
    entry: &'a [u8],
}

/// An item lot a script awards.
struct Award {
    map: Option<MapId>,
    event: i64,
    kind: Kind,
    lot: i32,
    /// The flag it is awarded on, for `item_lot_for_flag`.
    on: Option<u32>,
}

struct Lots {
    rows: HashMap<&'static str, HashMap<i32, Row>>,
    /// Every lot something references directly, per table.
    referenced: HashMap<&'static str, HashSet<i32>>,
}

impl Lots {
    fn row(&self, table: &str, id: i32) -> Option<&Row> {
        self.rows[table].get(&id)
    }

    /// The lot and the rows it runs on into.
    fn chain(&self, table: &'static str, id: i32) -> Vec<i32> {
        let mut ids = vec![id];
        let mut next = id + 1;
        while self.rows[table].contains_key(&next) && !self.referenced[table].contains(&next) {
            ids.push(next);
            next += 1;
        }
        ids
    }

    fn flag(&self, table: &str, id: i32) -> Option<u32> {
        let f = layout(LOT_MAP).field("get_item_flag");
        self.row(table, id)
            .map(|r| f.get(&r.data, 0) as u32)
            .filter(|&f| f != 0)
    }
}

pub fn extract(s: &Sources) -> Result<Model> {
    let mut model = Model::default();
    let part = |name| field("PARTS_PARAM_ST", name);

    // Every part with an entity id, and the graces that name the maps.
    let mut placed: HashMap<u32, Placed> = HashMap::new();
    for (map, msb) in &s.maps {
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if let Some(id) = part("entity_id").get(e, 0).filter(|&v| v != 0) {
                placed.entry(id as u32).or_insert(Placed {
                    map: *map,
                    entry: e,
                });
            }
        }
    }
    let warp = layout("BonfireWarpParam");
    for r in &s.reg.table("BonfireWarpParam")?.rows {
        let entity = warp.field("entity_id").get(&r.data, 0) as u32;
        let grace = Grace {
            id: r.id,
            name: s
                .text("PlaceName", warp.field("text_id").get(&r.data, 0) as i32)
                .map(String::from),
            map: placed.get(&entity).map(|p| p.map),
            entity,
            flag: warp.field("event_flag").get(&r.data, 0) as u32,
            position: placed.get(&entity).map(|p| position(p.entry)),
        };
        if let (Some(m), Some(n)) = (grace.map, &grace.name) {
            model.maps.entry(m).or_default().push(n.clone());
        }
        model.graces.push(grace);
    }
    for (map, _) in &s.maps {
        model.maps.entry(*map).or_default();
    }

    // Enemies.
    let npc = layout("NpcParam");
    let npcs: HashMap<i32, Row> = s
        .reg
        .table("NpcParam")?
        .rows
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let npc_name = |id: i32| {
        npcs.get(&id)
            .and_then(|r| s.text("NpcName", npc.field("name_id").get(&r.data, 0) as i32))
            .map(String::from)
    };
    for (map, msb) in &s.maps {
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if !part("type")
                .get(e, 0)
                .is_some_and(|t| ENEMY_TYPES.contains(&t))
            {
                continue;
            }
            let npc_param = part("npc_param").get(e, 0).unwrap_or(-1) as i32;
            model.enemies.push(Enemy {
                map: *map,
                part: entry_name(e).unwrap_or_default(),
                entity: part("entity_id")
                    .get(e, 0)
                    .filter(|&v| v != 0)
                    .map(|v| v as u32),
                npc_param,
                name: npc_name(npc_param),
                think: part("think").get(e, 0).unwrap_or(-1) as i32,
                chara_init: part("chara_init")
                    .get(e, 0)
                    .filter(|&v| v > 0)
                    .map(|v| v as i32),
                position: position(e),
            });
        }
    }

    // What the scripts award, and the bosses' names from their health bars.
    let mut scripts: Vec<(Option<MapId>, &fs_emevd::Emevd, Option<&fs_emevd::Emevd>)> = s
        .scripts
        .iter()
        .map(|(m, e)| (Some(*m), e, Some(&s.common_func)))
        .collect();
    scripts.push((None, &s.common, Some(&s.common_func)));
    scripts.push((None, &s.common_func, None));
    let mut awards: Vec<Award> = Vec::new();
    let mut bar_names: HashMap<u32, String> = HashMap::new();
    // The maps whose scripts turn each flag on.
    let mut set_in: HashMap<u32, HashSet<MapId>> = HashMap::new();
    for &(map, script, common) in &scripts {
        for (name, kind) in [
            ("award_item_lot", Kind::Event),
            ("award_item_lot_b", Kind::Event),
            ("item_lot_for_flag", Kind::BossReward),
        ] {
            let ins = catalogue::named(name);
            let lot_word = ins.arg("item_lot").word;
            for u in catalogue::uses(script, common, ins.bank, ins.id) {
                let Some(lot) = u.word(lot_word).map(|v| v as i32).filter(|&v| v > 0) else {
                    continue;
                };
                let on = (kind == Kind::BossReward)
                    .then(|| u.word(ins.arg("flag").word))
                    .flatten();
                awards.push(Award {
                    map,
                    event: u.event,
                    kind,
                    lot,
                    on,
                });
            }
        }
        let set = catalogue::named("set_flag");
        for u in catalogue::uses(script, common, set.bank, set.id) {
            if let (Some(m), Some(f), Some(1)) = (
                map,
                u.word(set.arg("flag").word),
                u.word(set.arg("state").word),
            ) {
                set_in.entry(f).or_default().insert(m);
            }
        }
        let bar = catalogue::named("boss_health_bar");
        for u in catalogue::uses(script, common, bar.bank, bar.id) {
            let (Some(boss), Some(text)) =
                (u.word(bar.arg("boss").word), u.word(bar.arg("name").word))
            else {
                continue;
            };
            if let Some(n) = s.text("NpcName", text as i32) {
                bar_names.entry(boss).or_insert_with(|| n.to_string());
            }
        }
    }

    // What the talk scripts give, and which shop rows they open.
    let mut gifts: Vec<(Option<MapId>, &str, i32)> = Vec::new();
    let mut sold_by: HashMap<i32, BTreeSet<(MapId, &str)>> = HashMap::new();
    for (map, name, esd) in &s.talk {
        for r in fs_esd::calls::resolve(esd) {
            let command = (r.bank, r.id);
            if let (AWARD_ITEM_LOT, [Some(lot), ..]) = (command, &r.args[..])
                && *lot > 0
            {
                gifts.push(((*map != EVERYWHERE).then_some(*map), name, *lot));
            }
            for (_, a, b) in SHOP_RANGES.iter().filter(|s| s.0 == command) {
                let (Some(Some(first)), Some(Some(last))) = (r.args.get(*a), r.args.get(*b)) else {
                    continue;
                };
                if (0..1000).contains(&(last - first)) {
                    for id in *first..=*last {
                        sold_by.entry(id).or_default().insert((*map, name));
                    }
                }
            }
        }
    }
    // Where each talk script's characters stand.
    let mut speakers: HashMap<i32, Vec<Spot>> = HashMap::new();
    for (map, msb) in &s.maps {
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if !part("type")
                .get(e, 0)
                .is_some_and(|t| ENEMY_TYPES.contains(&t))
            {
                continue;
            }
            if let Some(talk) = part("talk_id").get(e, 0).filter(|&v| v > 0) {
                speakers.entry(talk as i32).or_default().push(Spot {
                    map: *map,
                    position: Some(position(e)),
                });
            }
        }
    }
    // A script's places: where its characters stand, or else its binder's
    // map when that is a real one.
    let spots_of = |map: MapId, name: &str| -> Vec<Spot> {
        let placed = name
            .trim_start_matches('t')
            .parse::<i32>()
            .ok()
            .and_then(|t| speakers.get(&t).cloned());
        match placed {
            Some(p) => p,
            None if map != EVERYWHERE && !(map.is_open_world() && map.0[1..] == [0, 0, 0]) => {
                vec![Spot {
                    map,
                    position: None,
                }]
            }
            None => Vec::new(),
        }
    };

    // Bosses.
    let area = layout("GameAreaParam");
    for r in &s.reg.table("GameAreaParam")?.rows {
        let at = placed.get(&(r.id as u32)).filter(|p| {
            part("type")
                .get(p.entry, 0)
                .is_some_and(|t| ENEMY_TYPES.contains(&t))
        });
        model.bosses.push(Boss {
            area: r.id,
            map: at.map(|p| p.map),
            entity: at.map(|_| r.id as u32),
            name: bar_names.get(&(r.id as u32)).cloned().or_else(|| {
                at.and_then(|p| npc_name(part("npc_param").get(p.entry, 0).unwrap_or(-1) as i32))
            }),
            defeat_flag: area.field("defeat_flag").get(&r.data, 0) as u32,
        });
    }

    // What each placed asset gives when picked or broken, by its model.
    let geometry = layout("AssetEnvironmentGeometryParam");
    let asset_lots: HashMap<i32, [i32; 2]> = s
        .reg
        .table("AssetEnvironmentGeometryParam")?
        .rows
        .iter()
        .map(|r| {
            let lot = |f| geometry.field(f).get(&r.data, 0) as i32;
            (r.id, [lot("pick_item_lot"), lot("break_item_lot")])
        })
        .filter(|(_, l)| l.iter().any(|&v| v > 0))
        .collect();
    // (map, lot, how) -> (the first such asset, how many)
    let mut gathered: BTreeMap<(MapId, i32, &str), (&[u8], usize)> = BTreeMap::new();
    for (map, msb) in &s.maps {
        let models: Vec<Option<i32>> = msb
            .table("MODEL_PARAM_ST")
            .entries
            .iter()
            .map(|e| entry_name(e).as_deref().and_then(asset_model))
            .collect();
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if part("type").get(e, 0) != Some(ASSET) {
                continue;
            }
            let Some(lots) = part("model")
                .get(e, 0)
                .and_then(|i| models.get(usize::try_from(i).ok()?).copied().flatten())
                .and_then(|m| asset_lots.get(&m))
            else {
                continue;
            };
            for (lot, how) in lots.iter().zip(["picked", "broken"]) {
                if *lot > 0 {
                    gathered.entry((*map, *lot, how)).or_insert((e, 0)).1 += 1;
                }
            }
        }
    }

    // Every lot something references, so that chains stop before them.
    let mut lots = Lots {
        rows: HashMap::new(),
        referenced: HashMap::new(),
    };
    for t in [LOT_MAP, LOT_ENEMY] {
        lots.rows.insert(
            t,
            s.reg
                .table(t)?
                .rows
                .into_iter()
                .map(|r| (r.id, r))
                .collect(),
        );
        lots.referenced.insert(t, HashSet::new());
    }
    let ev = |name| field("EVENT_PARAM_ST", name);
    let treasures: Vec<(MapId, &[u8], &fs_msb::Msb)> = s
        .maps
        .iter()
        .flat_map(|(m, msb)| {
            msb.table("EVENT_PARAM_ST")
                .entries
                .iter()
                .filter(|e| ev("type").get(e, 0) == Some(TREASURE))
                .map(move |e| (*m, e.as_slice(), msb))
        })
        .collect();
    let refs = lots.referenced.get_mut(LOT_MAP).unwrap();
    refs.extend(
        treasures
            .iter()
            .filter_map(|(_, e, _)| ev("item_lot").get(e, 0).map(|v| v as i32)),
    );
    refs.extend(awards.iter().map(|a| a.lot));
    refs.extend(gifts.iter().map(|g| g.2));
    refs.extend(gathered.keys().map(|k| k.1));
    for r in npcs.values() {
        for (t, f) in [
            (LOT_ENEMY, "item_lot_enemy_id"),
            (LOT_MAP, "item_lot_map_id"),
        ] {
            let v = npc.field(f).get(&r.data, 0) as i32;
            if v > 0 {
                lots.referenced.get_mut(t).unwrap().insert(v);
            }
        }
    }

    let names = Names(s);
    let at_lot = |kind, map, source: String, table: &'static str, id: i32| {
        let ids = lots.chain(table, id);
        Location {
            kind,
            map,
            source,
            entity: None,
            position: None,
            items: ids
                .iter()
                .flat_map(|&i| lot_items(lots.row(table, i).unwrap(), &names))
                .collect(),
            flag: lots.flag(table, id),
            also: Vec::new(),
            lot: Some(Lot {
                table: table.into(),
                ids,
            }),
        }
    };

    // Treasure.
    for &(map, e, msb) in &treasures {
        let lot = ev("item_lot").get(e, 0).unwrap_or(-1) as i32;
        if lots.row(LOT_MAP, lot).is_none() {
            model.gaps.treasure_without_lot += 1;
            continue;
        }
        let on = ev("treasure_part")
            .get(e, 0)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| msb.table("PARTS_PARAM_ST").entries.get(i));
        let mut l = at_lot(
            Kind::Treasure,
            Some(map),
            on.and_then(|p| entry_name(p))
                .or_else(|| entry_name(e))
                .unwrap_or_default(),
            LOT_MAP,
            lot,
        );
        l.entity = on
            .and_then(|p| part("entity_id").get(p, 0))
            .filter(|&v| v != 0)
            .map(|v| v as u32);
        l.position = on.map(|p| position(p));
        model.locations.push(l);
    }

    // Enemy drops: a flagged lot once, wherever its enemies stand; an
    // unflagged one once per map.
    let mut seen: HashSet<(Option<MapId>, &str, i32)> = HashSet::new();
    for en in &model.enemies {
        let Some(r) = npcs.get(&en.npc_param) else {
            continue;
        };
        for (t, f) in [
            (LOT_ENEMY, "item_lot_enemy_id"),
            (LOT_MAP, "item_lot_map_id"),
        ] {
            let lot = npc.field(f).get(&r.data, 0) as i32;
            if lots.row(t, lot).is_none() {
                continue;
            }
            let once = lots.flag(t, lot).is_some();
            if !seen.insert((if once { None } else { Some(en.map) }, t, lot)) {
                continue;
            }
            let who = en
                .name
                .clone()
                .unwrap_or_else(|| format!("NpcParam {}", en.npc_param));
            let kind = if once {
                Kind::EnemyDrop
            } else {
                Kind::RepeatDrop
            };
            let mut l = at_lot(kind, Some(en.map), format!("{who} ({})", en.part), t, lot);
            l.entity = en.entity;
            l.position = Some(en.position);
            model.locations.push(l);
        }
    }

    // Gathering, once per map and lot.
    for (&(map, lot, how), &(e, n)) in &gathered {
        if lots.row(LOT_MAP, lot).is_none() {
            continue;
        }
        let name = entry_name(e).unwrap_or_default();
        let mut l = at_lot(
            Kind::Gather,
            Some(map),
            format!("{name} ({how}, ×{n})"),
            LOT_MAP,
            lot,
        );
        l.position = Some(position(e));
        model.locations.push(l);
    }

    // Script awards, each lot once.
    let mut given: HashSet<i32> = HashSet::new();
    awards.sort_by_key(|a| (a.kind != Kind::BossReward, a.map.is_none()));
    for Award {
        map,
        event,
        kind,
        lot,
        on,
    } in awards
    {
        if lots.row(LOT_MAP, lot).is_none() || !given.insert(lot) {
            continue;
        }
        // The boss whose defeat flag the award waits on, if it is one.
        let boss = on.and_then(|f| model.bosses.iter().find(|b| b.defeat_flag == f));
        let source = match (boss.and_then(|b| b.name.as_deref()), on) {
            (Some(n), _) => format!("{n} (event {event})"),
            (None, Some(f)) => format!("flag {f} (event {event})"),
            (None, None) => format!("event {event}"),
        };
        // A shared script's award is placed where its boss stands, or else in
        // the one map whose script sets its flag.
        let map = map.or_else(|| boss.and_then(|b| b.map)).or_else(|| {
            let maps = set_in.get(&on?)?;
            (maps.len() == 1).then(|| *maps.iter().next().unwrap())
        });
        let mut l = at_lot(kind, map, source, LOT_MAP, lot);
        l.entity = boss.and_then(|b| b.entity);
        model.locations.push(l);
    }

    // Gifts, each lot once, and not again if a script awards it too.
    for (map, name, lot) in gifts {
        if lots.row(LOT_MAP, lot).is_none() || !given.insert(lot) {
            continue;
        }
        model.locations.push(at_lot(
            Kind::Gift,
            map,
            format!("talk {name}"),
            LOT_MAP,
            lot,
        ));
    }

    // Shops.
    let shop = layout("ShopLineupParam");
    for r in &s.reg.table("ShopLineupParam")?.rows {
        let id = shop.field("equip_id").get(&r.data, 0) as i32;
        let kind = match shop.field("equip_type").get(&r.data, 0) {
            0 => ItemKind::Weapon,
            1 => ItemKind::Protector,
            2 => ItemKind::Accessory,
            3 => ItemKind::Goods,
            4 => ItemKind::Gem,
            _ => continue,
        };
        if id <= 0 {
            continue;
        }
        let stock = shop.field("sell_quantity").get(&r.data, 0);
        let openers = sold_by.get(&r.id);
        let mut spots: Vec<Spot> = openers
            .into_iter()
            .flatten()
            .flat_map(|&(map, name)| spots_of(map, name))
            .collect();
        // The Husks' copy only when nobody else sells it.
        if spots.iter().any(|p| p.map != ROUNDTABLE) {
            spots.retain(|p| p.map != ROUNDTABLE);
        }
        // Merchants found standing somewhere first, then binders' maps.
        spots.sort_by_key(|p| p.position.is_none());
        spots.dedup_by_key(|p| p.map);
        let mut source = format!("ShopLineupParam {}", r.id);
        if let Some(o) = openers {
            let talk: Vec<&str> = o.iter().map(|o| o.1).collect();
            source += &format!(" (talk {})", talk.join(", "));
        }
        let first = spots.first().copied();
        model.locations.push(Location {
            kind: Kind::Shop,
            map: first.map(|p| p.map),
            source,
            entity: None,
            position: first.and_then(|p| p.position),
            lot: None,
            items: vec![Item {
                kind,
                id,
                count: u32::try_from(stock).unwrap_or(0),
                chance: 1000,
                name: names.of(kind, id),
            }],
            flag: None,
            also: spots.into_iter().skip(1).collect(),
        });
    }

    // Starting equipment.
    let init = layout("CharaInitParam");
    for r in s.reg.table("CharaInitParam")?.rows {
        if !CLASSES.contains(&r.id) {
            continue;
        }
        let mut items = Vec::new();
        for (f, kind) in [
            ("weapon", ItemKind::Weapon),
            ("weapon_extra", ItemKind::Weapon),
            ("protector", ItemKind::Protector),
            ("ammo", ItemKind::Weapon),
            ("accessory", ItemKind::Accessory),
            ("goods_a", ItemKind::Goods),
            ("magic", ItemKind::Goods),
            ("goods", ItemKind::Goods),
            ("goods_b", ItemKind::Goods),
        ] {
            let f = init.field(f);
            for i in 0..f.count {
                let id = f.get(&r.data, i) as i32;
                if id > 0 {
                    items.push(Item {
                        kind,
                        id,
                        count: 1,
                        chance: 1000,
                        name: names.of(kind, id),
                    });
                }
            }
        }
        model.locations.push(Location {
            kind: Kind::Starting,
            map: None,
            source: format!("CharaInitParam {}", r.id),
            entity: None,
            position: None,
            lot: None,
            items,
            flag: None,
            also: Vec::new(),
        });
    }

    // Lots with items that nothing above gives.
    let mut reached: HashMap<&str, HashSet<i32>> = HashMap::new();
    for l in &model.locations {
        if let Some(lot) = &l.lot {
            reached
                .entry(lot.table.as_str())
                .or_default()
                .extend(lot.ids.iter().copied());
        }
    }
    for t in [LOT_MAP, LOT_ENEMY] {
        let mut ids: Vec<i32> = lots.rows[t]
            .values()
            .filter(|r| !lot_items(r, &names).is_empty())
            .map(|r| r.id)
            .filter(|id| !reached.get(t).is_some_and(|s| s.contains(id)))
            .collect();
        ids.sort();
        model.gaps.unreached_lots.insert(t.into(), ids);
    }
    Ok(model)
}

/// An asset model's row in AssetEnvironmentGeometryParam: AEG099_683 is
/// 99683.
fn asset_model(name: &str) -> Option<i32> {
    let (a, b) = name.strip_prefix("AEG")?.split_once('_')?;
    let (a, b) = (a.parse::<i32>().ok()?, b.get(..3)?.parse::<i32>().ok()?);
    Some(a * 1000 + b)
}

/// A part's position (probable, PARTS_PARAM_ST.position).
fn position(e: &[u8]) -> [f32; 3] {
    let f = field("PARTS_PARAM_ST", "position");
    [0, 1, 2].map(|i| f.get_f32(e, i).unwrap_or(f32::NAN))
}

/// Item names from the game's English text.
struct Names<'a>(&'a Sources);

impl Names<'_> {
    fn of(&self, kind: ItemKind, id: i32) -> Option<String> {
        let table = match kind {
            ItemKind::Goods => "GoodsName",
            ItemKind::Weapon => "WeaponName",
            ItemKind::Protector => "ProtectorName",
            ItemKind::Accessory => "AccessoryName",
            ItemKind::Gem => "GemName",
            ItemKind::CustomWeapon => return None,
        };
        // A weapon with an affinity or a level is named by its base id.
        let base = if kind == ItemKind::Weapon {
            id - id % 10_000
        } else {
            id
        };
        self.0
            .text(table, id)
            .or_else(|| self.0.text(table, base))
            .map(String::from)
    }
}

/// The items a lot row holds, in its eight slots.
fn lot_items(r: &Row, names: &Names) -> Vec<Item> {
    let l = layout(LOT_MAP);
    let (id, cat, weight, count) = (
        l.field("item_id"),
        l.field("item_category"),
        l.field("weight"),
        l.field("count"),
    );
    (0..id.count)
        .filter_map(|i| {
            let item = id.get(&r.data, i) as i32;
            let kind = match cat.get(&r.data, i) {
                1 => ItemKind::Goods,
                2 => ItemKind::Weapon,
                3 => ItemKind::Protector,
                4 => ItemKind::Accessory,
                5 => ItemKind::Gem,
                6 => ItemKind::CustomWeapon,
                _ => return None,
            };
            (item > 0).then(|| Item {
                kind,
                id: item,
                count: count.get(&r.data, i) as u32,
                chance: weight.get(&r.data, i) as u32,
                name: names.of(kind, item),
            })
        })
        .collect()
}

/// Counts for a quick read of a model: each kind's total and how many of
/// them have a map.
pub fn totals(model: &Model) -> BTreeMap<Kind, (usize, usize)> {
    let mut out = BTreeMap::new();
    for l in &model.locations {
        let e: &mut (usize, usize) = out.entry(l.kind).or_default();
        e.0 += 1;
        e.1 += usize::from(l.map.is_some());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_asset_models_row_is_read_from_its_name() {
        assert_eq!(asset_model("AEG099_683"), Some(99_683));
        assert_eq!(asset_model("AEG463_670"), Some(463_670));
        assert_eq!(asset_model("c4720"), None);
        assert_eq!(asset_model("AEG099"), None);
    }
}

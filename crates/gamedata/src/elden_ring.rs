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
//!   every call between a script's state machines followed. What holds on
//!   the way to each award (`fs_esd::reach`) becomes the gift's ways
//!   ([`Location::given_if`]): flags explained as a boss beaten, an item
//!   handed over, a location got, a grace rested at, or a talk script setting
//!   them on what it asks in turn; a flag nothing explains stays a [`Cond::Flag`].
//! - **Shops:** every ShopLineupParam row, placed where the characters stand
//!   whose talk scripts open a range holding it (`SHOP_RANGES`: shops,
//!   dragon communion, puppets and exchanges). A row the Twin Maiden Husks
//!   resell (bell-bearing stock) is placed with its merchant, and at the
//!   Roundtable only when nobody else sells it. Each row's [`Terms`] say
//!   what buying it asks: its currency, the items its material set spends,
//!   and the flag that puts it on sale, with the boss whose defeat sets that
//!   flag (an event run that marks one boss defeated and sets it) or the
//!   items whose handing over in talk does (`fs_esd::handover`).
//! - **Starting:** the ten classes' CharaInitParam rows (3000–3009).
//! - **Keepsakes:** the nine gifts' CharaInitParam rows (2401–2409), in the
//!   order character creation lists them.
//!
//! A lot may run on into the rows after it: rows that follow a lot directly
//! and that nothing references themselves are taken as part of it. That is
//! the probable reading (treasure lots followed by such a row share its flag
//! in 50 of 63 cases) and waits on play to be confirmed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use fs_bytes::Result;
use fs_emevd::catalogue;
use fs_esd::commands::{AWARD_ITEM_LOT, SET_EVENT_FLAG, SHOP_RANGES, TAKE_ITEM};
use fs_esd::cond::Atom;
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
/// The keepsakes in CharaInitParam; 2400 is "None".
const KEEPSAKES: std::ops::RangeInclusive<i32> = 2401..=2409;

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
    let mut parts: HashSet<(u32, MapId)> = HashSet::new();
    for (map, msb) in &s.maps {
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if let Some(id) = part("entity_id").get(e, 0).filter(|&v| v != 0) {
                parts.insert((id as u32, *map));
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
    // The flags each run of an event turns on, and the bosses it marks
    // defeated: by script, event and call.
    type Run = (usize, i64, Option<Vec<u8>>);
    let mut set_together: HashMap<Run, BTreeSet<u32>> = HashMap::new();
    let mut defeated_in: HashMap<Run, BTreeSet<i32>> = HashMap::new();
    for (index, &(map, script, common)) in scripts.iter().enumerate() {
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
            let (Some(f), Some(1)) = (u.word(set.arg("flag").word), u.word(set.arg("state").word))
            else {
                continue;
            };
            if let Some(m) = map {
                set_in.entry(f).or_default().insert(m);
            }
            set_together
                .entry((index, u.event, u.call))
                .or_default()
                .insert(f);
        }
        let defeated = catalogue::named("boss_defeated");
        for u in catalogue::uses(script, common, defeated.bank, defeated.id) {
            if let Some(area) = u.word(defeated.arg("boss_area").word) {
                defeated_in
                    .entry((index, u.event, u.call))
                    .or_default()
                    .insert(area as i32);
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

    // The ways each flag is turned on by an event: what the run asks on the
    // way to the instruction that sets it (`fs_emevd::flow`), and the map
    // whose script runs it.
    let mut event_sets: HashMap<u32, Vec<EventWay>> = HashMap::new();
    let setters = [catalogue::named("set_flag"), catalogue::named("set_flag_b")];
    let sets = |i: &fs_emevd::Instruction| -> Option<u32> {
        let e = setters.iter().find(|e| (e.bank, e.id) == (i.bank, i.id))?;
        let word = |name| catalogue::Entry::word(i, e.arg(name).word);
        (word("kind")? == 0 && word("state")? == 1)
            .then(|| word("flag"))
            .flatten()
            .filter(|&f| f != 0)
    };
    // Every flag an event can turn off, on any path: a gift that waits on one
    // may be missed. Talk scripts turn their own flags off and on as a
    // conversation goes, which says nothing of a quest moving on, so they are
    // left out.
    let mut cleared: HashSet<u32> = HashSet::new();
    let range = catalogue::named("set_flag_range");
    for &(_, script, common) in &scripts[..scripts.len() - 1] {
        for run in catalogue::runs(script, common) {
            for i in &run.instructions {
                let word = |e: &catalogue::Entry, name| catalogue::Entry::word(i, e.arg(name).word);
                if let Some(e) = setters.iter().find(|e| (e.bank, e.id) == (i.bank, i.id))
                    && word(e, "state") == Some(0)
                    && let Some(f) = word(e, "flag")
                {
                    cleared.insert(f);
                }
                if (i.bank, i.id) == (range.bank, range.id)
                    && word(range, "state") == Some(0)
                    && let (Some(a), Some(b)) = (word(range, "first"), word(range, "last"))
                    && a <= b
                    && b - a < 1000
                {
                    cleared.extend(a..=b);
                }
            }
        }
    }
    for &(map, script, common) in &scripts[..scripts.len() - 1] {
        for run in catalogue::runs(script, common) {
            for (at, ways) in fs_emevd::flow::ways_in(&run, |i| sets(i).is_some()) {
                let flag = sets(&run.instructions[at]).unwrap();
                let all = event_sets.entry(flag).or_default();
                all.extend(ways.into_iter().map(|way| EventWay { map, way }));
            }
        }
    }

    // The flags each lot needs off on every way to it, failed conditions counted.
    let mut needs_off: HashMap<i32, BTreeSet<u32>> = HashMap::new();
    for (_, _, esd) in &s.talk {
        for r in fs_esd::calls::resolve_strictly(esd) {
            if let (AWARD_ITEM_LOT, [Some(lot), ..]) = ((r.bank, r.id), &r.args[..])
                && *lot > 0
            {
                let off: BTreeSet<u32> = r
                    .needs
                    .iter()
                    .filter_map(|a| match a {
                        Atom::Flag(f, false) => Some(*f),
                        _ => None,
                    })
                    .collect();
                needs_off
                    .entry(*lot)
                    .and_modify(|all| all.retain(|f| off.contains(f)))
                    .or_insert(off);
            }
        }
    }

    // What the talk scripts give, and which shop rows they open.
    let mut gifts: Vec<(Option<MapId>, &str, i32)> = Vec::new();
    let mut sold_by: HashMap<i32, BTreeSet<(MapId, &str)>> = HashMap::new();
    // The goods whose handing over turns each flag on.
    let mut handed: HashMap<u32, BTreeSet<i32>> = HashMap::new();
    // What holds on each way a talk script gives a lot, or turns a flag on.
    let mut gift_ways: HashMap<i32, BTreeSet<BTreeSet<Atom>>> = HashMap::new();
    let mut talk_sets: HashMap<u32, BTreeSet<BTreeSet<Atom>>> = HashMap::new();
    // Flags talk turns off: a shop row on sale behind one can go off sale.
    let mut talk_clears: HashSet<u32> = HashSet::new();
    // What talk takes from the player or asks them to hold: (type, id).
    let mut quest_goods: BTreeSet<(i32, i32)> = BTreeSet::new();
    for (map, name, esd) in &s.talk {
        let chosen = fs_esd::choice::handovers(esd);
        for h in fs_esd::handover::handovers(esd).into_iter().chain(chosen) {
            for f in h.flags {
                handed.entry(f).or_default().insert(h.goods);
            }
        }
        for r in fs_esd::calls::resolve(esd) {
            let command = (r.bank, r.id);
            if let (TAKE_ITEM, [Some(kind), Some(id), ..]) = (command, &r.args[..]) {
                quest_goods.insert((*kind, *id));
            }
            quest_goods.extend(r.needs.iter().filter_map(|a| match a {
                Atom::Holds(kind, id) => Some((*kind, *id)),
                _ => None,
            }));
            if let (AWARD_ITEM_LOT, [Some(lot), ..]) = (command, &r.args[..])
                && *lot > 0
            {
                gifts.push(((*map != EVERYWHERE).then_some(*map), name, *lot));
                gift_ways.entry(*lot).or_default().insert(r.needs.clone());
            }
            // Any value but 0 turns a flag on: ten flags are set with -1, each a
            // first-talk flag its script then tests for off (Corhyn's 1040529255).
            if let (SET_EVENT_FLAG, [Some(flag), Some(v)]) = (command, &r.args[..])
                && *v != 0
                && let Ok(flag) = u32::try_from(*flag)
            {
                talk_sets.entry(flag).or_default().insert(r.needs.clone());
            }
            if let (SET_EVENT_FLAG, [Some(flag), Some(0)]) = (command, &r.args[..])
                && let Ok(flag) = u32::try_from(*flag)
            {
                talk_clears.insert(flag);
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
            terms: None,
            given_if: None,
            missable: false,
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
        model.gaps.treasure_empty += usize::from(l.items.is_empty());
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

    // Flags an event turns on as it marks one boss defeated: the one boss
    // `boss_defeated` names, or else the one whose defeat flag it sets
    // (Morgott's event also sets Margit's). On once that boss is beaten.
    let defeats: HashMap<u32, i32> = model
        .bosses
        .iter()
        .filter(|b| b.defeat_flag != 0)
        .map(|b| (b.defeat_flag, b.area))
        .collect();
    let areas: HashSet<i32> = model.bosses.iter().map(|b| b.area).collect();
    let mut after_boss: HashMap<u32, Option<i32>> =
        defeats.iter().map(|(&f, &b)| (f, Some(b))).collect();
    for (run, flags) in &set_together {
        let named: BTreeSet<i32> = defeated_in
            .get(run)
            .into_iter()
            .flatten()
            .copied()
            .filter(|a| areas.contains(a))
            .collect();
        let bosses = if named.is_empty() {
            flags
                .iter()
                .filter_map(|f| defeats.get(f).copied())
                .collect()
        } else {
            named
        };
        let [boss] = bosses.into_iter().collect::<Vec<_>>()[..] else {
            continue;
        };
        for &f in flags.iter().filter(|f| !defeats.contains_key(f)) {
            // A flag tied to two bosses is tied to neither.
            let e = after_boss.entry(f).or_insert(Some(boss));
            if *e != Some(boss) {
                *e = None;
            }
        }
    }

    // What shop rows cost besides the price.
    let mtrl = layout("EquipMtrlSetParam");
    let materials: HashMap<i32, Vec<Item>> = s
        .reg
        .table("EquipMtrlSetParam")?
        .rows
        .iter()
        .map(|r| {
            let (id, count, category) = (
                mtrl.field("item_id"),
                mtrl.field("count"),
                mtrl.field("category"),
            );
            let items = (0..id.count)
                .filter_map(|i| {
                    let item = id.get(&r.data, i) as i32;
                    let kind = match category.get(&r.data, i) {
                        1 => ItemKind::Protector,
                        4 => ItemKind::Goods,
                        _ => return None,
                    };
                    (item > 0).then(|| Item {
                        kind,
                        id: item,
                        count: u32::try_from(count.get(&r.data, i)).unwrap_or(1),
                        chance: 1000,
                        name: names.of(kind, item),
                    })
                })
                .collect();
            (r.id, items)
        })
        .collect();

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
        let on_sale = Some(shop.field("on_sale_flag").get(&r.data, 0) as u32).filter(|&f| f != 0);
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
            terms: Some(Terms {
                price: shop.field("value").get(&r.data, 0) as i32,
                currency: shop.field("currency").get(&r.data, 0) as u8,
                cost: materials
                    .get(&(shop.field("mtrl_set_id").get(&r.data, 0) as i32))
                    .cloned()
                    .unwrap_or_default(),
                on_sale,
                after_boss: on_sale.and_then(|f| after_boss.get(&f).copied().flatten()),
                handed: on_sale
                    .and_then(|f| handed.get(&f))
                    .into_iter()
                    .flatten()
                    .map(|&id| Item {
                        kind: ItemKind::Goods,
                        id,
                        count: 1,
                        chance: 1000,
                        name: names.of(ItemKind::Goods, id),
                    })
                    .collect(),
                ways: None,
            }),
            given_if: None,
            missable: false,
        });
    }

    // Starting equipment.
    let init = layout("CharaInitParam");
    for r in s.reg.table("CharaInitParam")?.rows {
        if !CLASSES.contains(&r.id) && !KEEPSAKES.contains(&r.id) {
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
            kind: if CLASSES.contains(&r.id) {
                Kind::Starting
            } else {
                Kind::Keepsake
            },
            map: None,
            source: format!("CharaInitParam {}", r.id),
            entity: None,
            position: None,
            lot: None,
            items,
            flag: None,
            also: Vec::new(),
            terms: None,
            given_if: None,
            missable: false,
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
    model.gaps.flagged_named = sort_unreached(s, &model, &lots, &names)?;
    model.bearings = bearings(s, &names);
    model.quest_items = quest_goods
        .into_iter()
        .filter_map(|(t, id)| {
            let kind = shop_item_kind(t)?;
            Some(Item {
                kind,
                id,
                count: 1,
                chance: 1000,
                name: names.of(kind, id),
            })
        })
        .collect();
    // What each gift asks, in the model's terms.
    let explain = Explain {
        after_boss: &after_boss,
        handed: &handed,
        got: model.locations.iter().filter_map(|l| l.flag).collect(),
        graces: model
            .graces
            .iter()
            .filter(|g| g.map.is_some() && g.flag != 0)
            .map(|g| g.flag)
            .collect(),
        parts,
        talk_sets: &talk_sets,
        event_sets: &event_sets,
        cleared: &cleared,
        names: &names,
        memo: Default::default(),
        fleeting: Default::default(),
    };
    for l in model.locations.iter_mut().filter(|l| l.kind == Kind::Gift) {
        let Some(ways) = l
            .lot
            .as_ref()
            .and_then(|lot| gift_ways.get(lot.ids.first()?))
        else {
            continue;
        };
        let mut out: Vec<Way> = Vec::new();
        explain.fleeting.set(false);
        for atoms in ways {
            out.extend(explain.ways(atoms, 0));
        }
        l.missable = explain.fleeting.get();
        keep(&mut out);
        // A way that asks nothing makes the rest moot; no way at all is never.
        l.given_if = (out.first() != Some(&Vec::new())).then_some(out);
    }
    mark_either_or(&mut model.locations);
    // A gift that needs a flag off is lost once an event turns that flag on. A flag
    // talk sets is left out, as talk marks its own gifts given that way.
    for l in model.locations.iter_mut().filter(|l| l.kind == Kind::Gift) {
        let off = l
            .lot
            .as_ref()
            .and_then(|lot| lot.ids.first())
            .and_then(|id| needs_off.get(id));
        l.missable |= off.is_some_and(|off| {
            off.iter()
                .any(|f| Some(*f) != l.flag && !PASSING.contains(f) && event_sets.contains_key(f))
        });
    }
    explain_shops(&mut model.locations, &explain, &talk_sets, &talk_clears);

    Ok(model)
}

/// The bell bearings talk scripts take from a menu, where the flag handing one
/// over turns on is the one an entry offering shop ranges waits on: the Twin
/// Maiden Husks' (`fs_esd::choice::offers`, `fs_esd::menu::ranges_by_flag`).
fn bearings(s: &Sources, names: &Names) -> Vec<Bearing> {
    let mut out = Vec::new();
    for (_, script, esd) in &s.talk {
        let ranges = fs_esd::menu::ranges_by_flag(esd);
        if ranges.is_empty() {
            continue;
        }
        for h in fs_esd::choice::offers(esd) {
            for flag in &h.flags {
                let Some(r) = ranges.get(flag) else { continue };
                if out
                    .iter()
                    .any(|b: &Bearing| b.goods == h.goods && b.script == *script)
                {
                    continue;
                }
                out.push(Bearing {
                    goods: h.goods,
                    name: names.of(ItemKind::Goods, h.goods),
                    script: script.clone(),
                    flag: *flag,
                    ranges: r.iter().copied().collect(),
                });
            }
        }
    }
    out.sort_by_key(|b| (b.goods, b.script.clone()));
    out
}

/// The unreached map lots that carry a flag and a named item, sorted by what
/// their ids and items say.
fn sort_unreached(
    s: &Sources,
    model: &Model,
    lots: &Lots,
    names: &Names,
) -> Result<BTreeMap<LotSort, Vec<i32>>> {
    let goods = layout("EquipParamGoods");
    let tutorial: HashSet<i32> = s
        .reg
        .table("EquipParamGoods")?
        .rows
        .iter()
        .filter(|r| goods.field("goods_type").get(&r.data, 0) == 7)
        .map(|r| r.id)
        .collect();
    let given: HashSet<(ItemKind, i32)> = model
        .locations
        .iter()
        .flat_map(|l| &l.items)
        .map(|i| (i.kind, i.id))
        .collect();
    let maps: HashSet<[u8; 3]> = s
        .maps
        .iter()
        .map(|(m, _)| [m.0[0], m.0[1], m.0[2]])
        .collect();
    let mut out: BTreeMap<LotSort, Vec<i32>> = BTreeMap::new();
    for &id in &model.gaps.unreached_lots[LOT_MAP] {
        let items = lot_items(lots.row(LOT_MAP, id).unwrap(), names);
        // A name the game's text gives as "[ERROR]…" is cut content's.
        let named = |i: &Item| i.name.as_deref().is_some_and(|n| !n.starts_with("[ERROR]"));
        if lots.flag(LOT_MAP, id).is_none() || !items.iter().any(named) {
            continue;
        }
        let sort = match lot_map(id) {
            _ if items
                .iter()
                .all(|i| i.kind == ItemKind::Goods && tutorial.contains(&i.id)) =>
            {
                LotSort::Tutorial
            }
            Err(sort) => sort,
            Ok(map) if !maps.contains(&map) => LotSort::MapNotRead,
            Ok(_) if items.iter().all(|i| given.contains(&(i.kind, i.id))) => LotSort::MapLeftover,
            Ok(_) => LotSort::MapUnknown,
        };
        out.entry(sort).or_default().push(id);
    }
    Ok(out)
}

/// The map a lot is numbered after: `AABBxxxx` for `mAA_BB`, `10XXYYxxxx` and
/// `20XXYYxxxx` for the open worlds' tiles `m60_XX_YY` and `m61_XX_YY`, as the
/// maps' own treasure lots are. Ids of no map are sorted by their range.
pub fn lot_map(id: i32) -> std::result::Result<[u8; 3], LotSort> {
    let tile = |world: u8| {
        [
            world,
            (id / 1_000_000 % 100) as u8,
            (id / 10_000 % 100) as u8,
        ]
    };
    Ok(match id {
        ..100_000 => return Err(LotSort::LowId),
        100_000..200_000 => return Err(LotSort::Npc),
        9_000_000..10_000_000 => return Err(LotSort::Test),
        10_000_000..60_000_000 => [(id / 1_000_000) as u8, (id / 10_000 % 100) as u8, 0],
        1_000_000_000..2_000_000_000 => tile(60),
        2_000_000_000.. => tile(61),
        _ => return Err(LotSort::Other),
    })
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

/// How many ways to keep for one gift or flag: those with fewest flags nothing
/// explains, then the shortest ([`open_first`]).
const MAX_WAYS: usize = 8;

/// How ways are ordered before only [`MAX_WAYS`] are kept: a way waiting on a
/// flag nothing explains is never open, so open ways go first, then short ones.
/// Which ways are dropped only decides which stay shut, never opens one.
fn open_first(w: &Way) -> (usize, usize) {
    let unexplained = w.iter().filter(|c| matches!(c, Cond::Flag(_))).count();
    (unexplained, w.len())
}

/// Orders ways by [`open_first`] and keeps at most [`MAX_WAYS`], leaving out
/// any that asks all another asks and more: the same conditions in another
/// order would otherwise fill the places of ways that differ.
fn keep(ways: &mut Vec<Way>) {
    ways.sort_by_key(open_first);
    let mut out: Vec<Way> = Vec::new();
    for w in ways.drain(..) {
        if out.len() == MAX_WAYS {
            break;
        }
        if !out.iter().any(|o| o.iter().all(|c| w.contains(c))) {
            out.push(w);
        }
    }
    *ways = out;
}

/// How deep a flag is followed back to what sets it. Long quests chain further than
/// eight steps (Hyetta's grapes), and at the limit a way stays shut, so a deeper one
/// only opens gifts whose every step was read.
const MAX_DEPTH: usize = 16;

/// The Knights of the Great Jar (m60_47_41). Talking to the Jar turns on
/// 1047419200; then entering the fight's region summons each knight not yet
/// dead (event 1047412300, which ends by running again), and 1047419201, the
/// Jar's reward, waits for all three dead (1047412301). Nothing clears either
/// flag, so the fight can be taken up again. Other deaths stay
/// unread: an NPC may only be there at one stage of a quest, which the way would
/// not show.
const JAR_KNIGHTS: [u32; 3] = [1047410701, 1047410702, 1047410703];
/// The two hanging bells the Hole-Laden Necklace sounds (m61_50_40, m61_53_46):
/// object actions by their maps' event tables, and each event asks for nothing
/// else than holding the necklace there.
const BELLS: [u32; 2] = [2050403600, 2053463600];
/// The player's entity id in the event scripts.
const PLAYER: u32 = 10000;
/// Flags events turn on and off again at once, which shut nothing for good: 9001,
/// which common events 901 and 902 hold on only while they run.
const PASSING: [u32; 1] = [9001];
/// A lot given in place of one of an either-or pair ([`mark_either_or`]) that
/// shares no flag with them: Ymir's 106620, given with both bells rung only if
/// 106610 was taken (machine 2147483608 of t420006100 branches on 2051459208,
/// set when 106610 is given, and gives this or 106630).
const GIVEN_INSTEAD: [i32; 1] = [106620];

/// Gifts a player gets one or the other of. Two gifts with one pick-up flag are
/// the same reward by two ways, and the way taken shuts the other: Ymir gives
/// 106610 if talked to before both bells have rung, and 106630 if after.
/// Which one depends on the player, so neither can be counted on for a key item.
fn mark_either_or(locations: &mut [Location]) {
    let mut by_flag: HashMap<u32, usize> = HashMap::new();
    for l in locations.iter().filter(|l| l.kind == Kind::Gift) {
        if let Some(f) = l.flag {
            *by_flag.entry(f).or_default() += 1;
        }
    }
    for l in locations.iter_mut().filter(|l| l.kind == Kind::Gift) {
        let shared = l.flag.is_some_and(|f| by_flag[&f] > 1);
        let instead = l
            .lot
            .as_ref()
            .and_then(|lot| lot.ids.first())
            .is_some_and(|id| GIVEN_INSTEAD.contains(id));
        l.missable |= shared || instead;
    }
}

/// Shop rows whose on-sale flag no boss or handover explains take the ways
/// events and talk turn it on ([`Terms::ways`]), read as a gift's are: Corhyn
/// puts the Immutable Shield on sale as he is talked to, and the Twin Maiden
/// Husks resell a gift a quest's end left untaken. A row is missable when a way
/// can be lost: it needs a flag off that an event or a pick-up turns on (the
/// Husks' resale stops once the gift is taken), or the flag can be turned off
/// again, by an event or by talk.
fn explain_shops(
    locations: &mut [Location],
    explain: &Explain,
    talk_sets: &HashMap<u32, BTreeSet<BTreeSet<Atom>>>,
    talk_clears: &HashSet<u32>,
) {
    for l in locations.iter_mut().filter(|l| l.kind == Kind::Shop) {
        let Some(t) = l.terms.as_mut() else { continue };
        let Some(f) = t
            .on_sale
            .filter(|_| t.after_boss.is_none() && t.handed.is_empty())
        else {
            continue;
        };
        explain.fleeting.set(false);
        let mut ways = explain.flag(f, 0);
        if ways == [vec![Cond::Flag(f)]] {
            continue;
        }
        // A way that asks nothing makes the rest moot.
        if ways.iter().any(Vec::is_empty) {
            ways = vec![Vec::new()];
        }
        let lost_in_talk = talk_sets.get(&f).into_iter().flatten().flatten().any(|a| {
            matches!(a, Atom::Flag(g, false)
                if *g != f && !PASSING.contains(g)
                    && (explain.event_sets.contains_key(g) || explain.got.contains(g)))
        });
        l.missable |= explain.fleeting.get() || lost_in_talk || talk_clears.contains(&f);
        t.ways = Some(ways);
    }
}

/// Turns what talk conditions ask ([`Atom`]s) into the model's terms.
struct Explain<'a> {
    after_boss: &'a HashMap<u32, Option<i32>>,
    handed: &'a HashMap<u32, BTreeSet<i32>>,
    /// Flags that picking up a location sets.
    got: HashSet<u32>,
    /// Graces' flags, of those placed in a map.
    graces: HashSet<u32>,
    /// Every part's entity id, with the map it is placed in.
    parts: HashSet<(u32, MapId)>,
    talk_sets: &'a HashMap<u32, BTreeSet<BTreeSet<Atom>>>,
    event_sets: &'a HashMap<u32, Vec<EventWay>>,
    /// Flags an event can turn off again.
    cleared: &'a HashSet<u32>,
    names: &'a Names<'a>,
    /// Each flag's ways at a depth, and whether any flag on the way can be
    /// turned off again.
    memo: std::cell::RefCell<HashMap<(u32, usize), Explained>>,
    /// Whether what was explained since this was reset used a flag that can
    /// be turned off again.
    fleeting: std::cell::Cell<bool>,
}

/// A flag's ways, and whether any flag on the way can be turned off again.
type Explained = (Vec<Way>, bool);

/// One way an event turns a flag on: what its run asks, in the script of
/// `map` (none for the common script, which always runs).
struct EventWay {
    map: Option<MapId>,
    way: fs_emevd::flow::Way,
}

impl Explain<'_> {
    /// The ways to have all of `atoms`: flags that must be on and items that
    /// must be held. Flags that must be off are taken to be off.
    fn ways(&self, atoms: &BTreeSet<Atom>, depth: usize) -> Vec<Way> {
        let mut ways: Vec<Way> = vec![Vec::new()];
        for a in atoms {
            let options = match *a {
                Atom::Flag(f, true) => self.flag(f, depth),
                Atom::Holds(t, id) => match shop_item_kind(t) {
                    Some(kind) => vec![vec![Cond::Holds(self.item(kind, id))]],
                    None => continue,
                },
                _ => continue,
            };
            let mut next: Vec<Way> = Vec::new();
            for w in &ways {
                for o in &options {
                    let mut both = w.clone();
                    both.extend(o.iter().filter(|c| !w.contains(c)).cloned());
                    next.push(both);
                }
            }
            keep(&mut next);
            ways = next;
        }
        ways
    }

    /// The ways a flag comes to be on: a boss beaten, an item handed over, a
    /// location got, a grace rested at, or a talk script or event setting it,
    /// on what that asks.
    fn flag(&self, f: u32, depth: usize) -> Vec<Way> {
        if let Some(Some(boss)) = self.after_boss.get(&f) {
            return vec![vec![Cond::Boss(*boss)]];
        }
        if let Some(goods) = self.handed.get(&f) {
            return goods
                .iter()
                .map(|&id| vec![Cond::Holds(self.item(ItemKind::Goods, id))])
                .collect();
        }
        if self.got.contains(&f) {
            return vec![vec![Cond::Got(f)]];
        }
        // Rested at, so reached: Seluvis's quest waits on any grace of Liurnia.
        if self.graces.contains(&f) {
            return vec![vec![Cond::Grace(f)]];
        }
        let talk = self.talk_sets.get(&f);
        let events = self.event_sets.get(&f);
        if (talk.is_none() && events.is_none()) || depth >= MAX_DEPTH {
            return vec![vec![Cond::Flag(f)]];
        }
        if let Some((known, fleeting)) = self.memo.borrow().get(&(f, depth)) {
            self.fleeting.set(self.fleeting.get() || *fleeting);
            return known.clone();
        }
        let outer = self.fleeting.replace(self.cleared.contains(&f));
        let mut out: Vec<Way> = Vec::new();
        for atoms in talk.into_iter().flatten() {
            // The flag itself is not a reason for itself.
            let rest: BTreeSet<Atom> = atoms
                .iter()
                .filter(|a| **a != Atom::Flag(f, true))
                .copied()
                .collect();
            out.extend(self.ways(&rest, depth + 1));
        }
        for e in events.into_iter().flatten() {
            out.extend(self.event_way(e, f, depth + 1));
        }
        keep(&mut out);
        if out.is_empty() {
            // Named, so that what blocks is reported.
            out.push(vec![Cond::Flag(f)]);
        }
        let fleeting = self.fleeting.get();
        self.memo
            .borrow_mut()
            .insert((f, depth), (out.clone(), fleeting));
        self.fleeting.set(outer || fleeting);
        out
    }

    /// The ways to have what an event run asks before it sets `f`. Flags that
    /// must be off are taken to be off, as in talk.
    fn event_way(&self, e: &EventWay, f: u32, depth: usize) -> Vec<Way> {
        use fs_emevd::flow::{Reading, Test};
        let mut ways: Vec<Way> = vec![e.map.map(Cond::Map).into_iter().collect()];
        let mut lost = false;
        // Each pair a distance is asked of, and whether it must be within or
        // beyond, by byte 1 and whether the test must hold: a way asking both of
        // one pair may be one no place meets.
        let mut sides: HashMap<(u32, u32), BTreeSet<bool>> = HashMap::new();
        for lit in &e.way {
            if let Some(Reading::Distance { from, to, side }) = lit.test.reading() {
                sides
                    .entry((from, to))
                    .or_default()
                    .insert((side != 0) == lit.holds);
            }
        }
        for lit in &e.way {
            let options = match (&lit.test, lit.holds) {
                (Test::Flag(g), true) if *g != f => self.flag(*g, depth),
                (Test::Flag(_), true) => continue,
                // Off, so lost once an event can turn it on, if this way can happen,
                // or once the location whose pick-up sets it is taken.
                (Test::Flag(g), false) => {
                    lost |= *g != f
                        && ((self.event_sets.contains_key(g) && !self.talk_sets.contains_key(g))
                            || self.got.contains(g));
                    continue;
                }
                (Test::Other { .. }, holds) => match (lit.test.reading(), holds) {
                    (Some(Reading::Dead(c)), true) if JAR_KNIGHTS.contains(&c) => continue,
                    (Some(Reading::ObjectAction(o)), true) if BELLS.contains(&o) => continue,
                    // The logic plays alone, always in the player's own world.
                    (Some(Reading::MyWorld { wanted }), holds) if wanted == holds => continue,
                    (Some(Reading::MyWorld { .. }), _) => return Vec::new(),
                    // The player near a part of the map whose script this is, or
                    // away from it: either is met by reaching the map, which the
                    // way already asks.
                    (Some(Reading::Distance { from, to, .. }), _)
                        if sides[&(from, to)].len() == 1
                            && e.map.is_some_and(|m| {
                                (from == PLAYER && self.parts.contains(&(to, m)))
                                    || (to == PLAYER && self.parts.contains(&(from, m)))
                            }) =>
                    {
                        continue;
                    }
                    // Holding a good, however the event asks it.
                    (Some(Reading::Good { id, held }), holds) if held == holds => {
                        vec![vec![Cond::Holds(self.item(ItemKind::Goods, id))]]
                    }
                    // Any other test (a position, a character's state, a
                    // count) shuts the way: it may ask what the logic cannot
                    // see.
                    _ => return Vec::new(),
                },
            };
            let mut next: Vec<Way> = Vec::new();
            for w in &ways {
                for o in &options {
                    let mut both = w.clone();
                    both.extend(o.iter().filter(|c| !w.contains(c)).cloned());
                    next.push(both);
                }
            }
            keep(&mut next);
            ways = next;
        }
        if lost && !ways.is_empty() {
            self.fleeting.set(true);
        }
        ways
    }

    fn item(&self, kind: ItemKind, id: i32) -> Item {
        Item {
            kind,
            id,
            count: 1,
            chance: 1000,
            name: self.names.of(kind, id),
        }
    }
}

/// An item kind by ShopLineupParam's numbering, which talk scripts share.
fn shop_item_kind(t: i32) -> Option<ItemKind> {
    Some(match t {
        0 => ItemKind::Weapon,
        1 => ItemKind::Protector,
        2 => ItemKind::Accessory,
        3 => ItemKind::Goods,
        4 => ItemKind::Gem,
        _ => return None,
    })
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

    #[test]
    fn a_lots_map_is_read_from_its_id() {
        assert_eq!(lot_map(1_036_430_010), Ok([60, 36, 43]));
        assert_eq!(lot_map(2_050_450_510), Ok([61, 50, 45]));
        assert_eq!(lot_map(12_020_700), Ok([12, 2, 0]));
        assert_eq!(lot_map(50_000), Err(LotSort::LowId));
        assert_eq!(lot_map(101_580), Err(LotSort::Npc));
        assert_eq!(lot_map(9_930_001), Err(LotSort::Test));
    }

    #[test]
    fn ways_kept_differ_from_each_other() {
        let (a, b) = (Cond::Flag(1), Cond::Map(MapId([60, 34, 50, 0])));
        // The same two in either order, a way asking more, and all but two of
        // the ways kept, each asking one flag nothing explains: the repeat and the
        // way asking more leave room for one of them only.
        let mut ways: Vec<Way> = vec![
            vec![a.clone(), b.clone()],
            vec![b.clone(), a.clone()],
            vec![b.clone(), a.clone(), Cond::Got(3)],
        ];
        ways.extend((10..10 + MAX_WAYS as u32 - 2).map(|f| vec![Cond::Flag(f)]));
        keep(&mut ways);
        assert_eq!(ways.len(), MAX_WAYS - 1);
        assert_eq!(
            ways.iter()
                .filter(|w| w.contains(&a) && w.contains(&b))
                .count(),
            1
        );
        // An open way goes first.
        ways.push(vec![b.clone()]);
        keep(&mut ways);
        assert_eq!(ways[0], vec![b.clone()]);
        assert!(!ways.iter().any(|w| w.len() > 1 && w.contains(&b)));
    }
}

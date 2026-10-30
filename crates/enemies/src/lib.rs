//! Enemy placement (docs/spec/enemies.md). What stands at a spot changes; the
//! spot keeps its entity id, so the scripts that name it still find whoever
//! stands there, and it keeps its drops, so the item plan stays true.
//!
//! - **Bosses** trade places with bosses, every body once, and **ordinary
//!   enemies** with ordinary enemies of about their size. Each group has a
//!   slider, and a third slider has bosses and ordinary enemies trade places.
//! - **A body** is what moves: the NpcParam row, AI, equipment and model.
//! - **Scaling** gives the body a copy of its NpcParam row with the health and
//!   strength tier of whoever stood there before. The game's own tiers do the
//!   rest: Margit's health is his row's 2521 times his tier's 1.656.

pub mod malenia;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use fs_bytes::{Result, format_err};
use fs_emevd::catalogue;
use fs_msb::Msb;
use fs_msb::layout::{entry_name, field};
use fs_param::layout::for_table;
use fs_param::{Param, Regulation, Row};
use gamedata::model::{Kind, Model};
use gamedata::{MapId, Sources};
use items::rng::Rng;

const PARTS: &str = "PARTS_PARAM_ST";
const MODELS: &str = "MODEL_PARAM_ST";
/// An enemy that takes part in play. Type 10 stands in cutscenes and the like.
const ENEMY: i64 = 2;
/// Mixed into the seed, so enemies draw apart from items.
const SALT: u64 = 0x454E_454D_4945_5321;
/// Where the rows made for a seed start. The game's own stop below this.
const FIRST_NEW_ROW: i32 = 800_000_000;
/// Models with no creature of their own: the player's form, and a stand-in.
const NOT_BODIES: [&str; 1] = ["c1000"];
/// Enemies the area map counts on doing what they do: an Abductor Virgin's
/// grab is a way to the Volcano Manor. A Walking Mausoleum carries a shop.
const STAY: [&str; 3] = ["Abductor Virgin", "Walking Mausoleum", "dummy"];

/// What the player sets (docs/spec/enemies.md §2). All at 0 changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// How much of the bosses moves, 0 to 100.
    pub bosses: u8,
    /// How much of the ordinary enemies moves, 0 to 100.
    pub ordinary: u8,
    /// Share of the bosses that trade places with an ordinary enemy.
    pub mix: u8,
    /// Whether a body takes on the health and tier of the spot it lands on.
    pub scale: bool,
    /// Whether base-game and DLC bodies stay on their own side.
    pub dlc_separate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            bosses: 0,
            ordinary: 0,
            mix: 0,
            scale: true,
            dlc_separate: false,
        }
    }
}

impl Options {
    pub fn is_off(&self) -> bool {
        self.bosses == 0 && self.ordinary == 0 && self.mix == 0
    }

    /// Apply `name=percent` settings: `--enemies bosses=100,ordinary=50,mix=10`.
    pub fn set(&mut self, spec: &str) -> std::result::Result<(), String> {
        for part in spec.split(',').filter(|p| !p.is_empty()) {
            let (name, value) = part
                .split_once('=')
                .ok_or_else(|| format!("'{part}': expected name=percent"))?;
            let percent: u8 = value
                .trim_end_matches('%')
                .parse()
                .ok()
                .filter(|&p| p <= 100)
                .ok_or_else(|| format!("'{part}': the percent must be 0 to 100"))?;
            match name {
                "bosses" => self.bosses = percent,
                "ordinary" => self.ordinary = percent,
                "mix" => self.mix = percent,
                _ => return Err(format!("'{name}': expected bosses, ordinary or mix")),
            }
        }
        Ok(())
    }
}

/// How big a body is, for keeping dragons out of tunnels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    pub fn of(height: f32, radius: f32) -> Size {
        let span = height.max(radius * 2.0);
        if span < 2.7 {
            Size::Small
        } else if span < 6.0 {
            Size::Medium
        } else {
            Size::Large
        }
    }
}

/// What moves: everything that makes an enemy the enemy it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub npc: i32,
    pub think: i32,
    /// Equipment, for a body built like a player.
    pub chara: i32,
    pub model: String,
    pub name: String,
}

/// A spot in a map that a body stands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    /// Index of its map in `Sources::maps`.
    pub map_index: usize,
    /// Index of its part in the map.
    pub part: usize,
    pub map: MapId,
    pub entity: Option<u32>,
    pub body: Body,
    pub size: Size,
    pub dlc: bool,
    /// For a boss: the health bar's text, and the map whose script shows it.
    pub boss: Option<BossSpot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BossSpot {
    pub bar_text: i32,
    pub script: MapId,
}

/// A boss left where it is, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixed {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct Pool {
    pub spots: Vec<Spot>,
    pub fixed: Vec<Fixed>,
}

impl Pool {
    pub fn bosses(&self) -> impl Iterator<Item = &Spot> {
        self.spots.iter().filter(|s| s.boss.is_some())
    }
}

fn is_dlc(m: MapId) -> bool {
    items::in_dlc_map(m)
}

/// Every word of an instruction.
fn words(i: &fs_emevd::Instruction) -> impl Iterator<Item = u32> + '_ {
    i.args
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
}

/// Find everything that can move.
pub fn pool(s: &Sources, model: &Model) -> Result<Pool> {
    let npc_rows: HashMap<i32, Row> = s
        .reg
        .table("NpcParam")?
        .rows
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let npc = for_table("NpcParam").ok_or_else(|| format_err("no layout for NpcParam"))?;
    let get = |e: &[u8], n: &str| field(PARTS, n).get(e, 0).unwrap_or(-1);

    // Each boss as the scripts show it, and whose death they wait on: 4[0]
    // with the character, then state 1.
    let bar = catalogue::named("boss_health_bar");
    let mut bars: BTreeMap<u32, (MapId, i32)> = BTreeMap::new();
    let mut waited: HashSet<u32> = HashSet::new();
    for (m, script) in &s.scripts {
        for u in catalogue::uses(script, Some(&s.common_func), bar.bank, bar.id) {
            if let (Some(b), Some(t)) = (u.word(bar.arg("boss").word), u.word(bar.arg("name").word))
            {
                bars.entry(b).or_insert((*m, t as i32));
            }
        }
        for run in catalogue::runs(script, Some(&s.common_func)) {
            for i in &run.instructions {
                if (i.bank, i.id) == (4, 0) {
                    let w: Vec<u32> = words(i).collect();
                    if w.len() >= 3 && w[2] == 1 {
                        waited.insert(w[1]);
                    }
                }
            }
        }
    }
    let area_bosses: HashSet<u32> = model.bosses.iter().filter_map(|b| b.entity).collect();
    // A talk script that gives something can't lose its speaker.
    let giving: HashSet<&str> = model
        .locations
        .iter()
        .filter(|l| l.kind == Kind::Gift)
        .filter_map(|l| l.source.strip_prefix("talk "))
        .collect();

    let mut out = Pool::default();
    let mut placed: HashMap<u32, usize> = HashMap::new();
    for (map_index, (m, msb)) in s.maps.iter().enumerate() {
        let models = &msb.table(MODELS).entries;
        for (part, e) in msb.table(PARTS).entries.iter().enumerate() {
            if get(e, "type") != ENEMY {
                continue;
            }
            let id = get(e, "npc_param") as i32;
            let Some(row) = npc_rows.get(&id) else {
                continue;
            };
            let Some(model_name) = usize::try_from(get(e, "model"))
                .ok()
                .and_then(|i| models.get(i))
                .and_then(|me| entry_name(me))
            else {
                continue;
            };
            let entity = Some(get(e, "entity_id"))
                .filter(|&v| v > 0)
                .map(|v| v as u32);
            let chara = get(e, "chara_init") as i32;
            let talk = get(e, "talk_id");
            // Most rows have no name: their id is 0, whose text is a leftover.
            let name = Some(npc.field("name_id").get(&row.data, 0) as i32)
                .filter(|&id| id > 0)
                .and_then(|id| s.text("NpcName", id))
                .unwrap_or("")
                .to_string();
            let body = Body {
                npc: id,
                think: get(e, "think") as i32,
                chara,
                model: model_name.clone(),
                name: name.clone(),
            };
            let size = Size::of(
                npc.field("height").get_f32(&row.data, 0),
                npc.field("radius").get_f32(&row.data, 0),
            );
            let spot = |boss| Spot {
                map_index,
                part,
                map: *m,
                entity,
                body: body.clone(),
                size,
                dlc: is_dlc(*m),
                boss,
            };
            if let Some((script, bar_text)) = entity.and_then(|en| bars.get(&en)) {
                let en = entity.unwrap_or(0);
                let shown = s.text("NpcName", *bar_text).unwrap_or(&name).to_string();
                let refuse = if shown.contains("dummy") {
                    Some("a stand-in with no fight of its own")
                } else if placed.contains_key(&en) {
                    Some("placed twice")
                } else if !waited.contains(&en) {
                    Some("no script waits on its death, so the fight might never end")
                } else if talk > 0 && giving.contains(format!("t{talk:09}").as_str()) {
                    Some("its talk script gives an item")
                } else {
                    None
                };
                if let Some(reason) = refuse {
                    out.fixed.push(Fixed {
                        name: shown,
                        reason: reason.to_string(),
                    });
                    continue;
                }
                placed.insert(en, out.spots.len());
                let mut sp = spot(Some(BossSpot {
                    bar_text: *bar_text,
                    script: *script,
                }));
                sp.body.name = shown;
                out.spots.push(sp);
                continue;
            }
            let body_ok = !NOT_BODIES.contains(&model_name.as_str())
                && (model_name != "c0000" || chara > 0)
                && npc.field("hp").get(&row.data, 0) > 1
                && body.think > 1
                && talk <= 0
                && !entity.is_some_and(|en| area_bosses.contains(&en))
                && !STAY.iter().any(|n| name.contains(n))
                // A named character who is not built like a player is someone's
                // quest: Blaidd, Alexander. Named humans here are hostile ones.
                && (name.is_empty() || chara > 0);
            if body_ok {
                out.spots.push(spot(None));
            }
        }
    }
    Ok(out)
}

/// Which spot each body came from: `from[i]` is the spot whose body now stands
/// on spot `i`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub from: Vec<usize>,
}

/// Each of `group` moves with `percent` chance; those that move trade places.
fn trade(from: &mut [usize], group: &[usize], percent: u8, rng: &mut Rng) {
    let moved: Vec<usize> = group
        .iter()
        .copied()
        .filter(|_| rng.below(100) < usize::from(percent))
        .collect();
    let mut bodies: Vec<usize> = moved.iter().map(|&i| from[i]).collect();
    rng.shuffle(&mut bodies);
    for (&i, b) in moved.iter().zip(bodies) {
        from[i] = b;
    }
}

/// Place every body. The same pool, seed and options give the same plan.
pub fn shuffle(pool: &Pool, seed: u64, options: &Options) -> Plan {
    let mut rng = Rng::new(seed ^ SALT);
    let mut from: Vec<usize> = (0..pool.spots.len()).collect();
    let side = |s: &Spot| options.dlc_separate && s.dlc;

    let mut bosses: BTreeMap<bool, Vec<usize>> = BTreeMap::new();
    let mut ordinary: BTreeMap<(bool, Size), Vec<usize>> = BTreeMap::new();
    for (i, s) in pool.spots.iter().enumerate() {
        if s.boss.is_some() {
            bosses.entry(side(s)).or_default().push(i);
        } else {
            ordinary.entry((side(s), s.size)).or_default().push(i);
        }
    }
    for group in bosses.values() {
        trade(&mut from, group, options.bosses, &mut rng);
    }
    for group in ordinary.values() {
        trade(&mut from, group, options.ordinary, &mut rng);
    }

    // Mixing: a boss's body goes out into an open world, where there is room,
    // and the ordinary enemy from there takes the arena. Malenia keeps to
    // arenas, as her second phase needs a script to run in.
    for (&dlc, group) in &bosses {
        let mut fields: Vec<usize> = pool
            .spots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.boss.is_none() && s.map.is_open_world() && side(s) == dlc)
            .map(|(i, _)| i)
            .collect();
        rng.shuffle(&mut fields);
        let mut fields = fields.into_iter();
        for &b in group {
            if rng.below(100) >= usize::from(options.mix) || malenia::is_home(&pool.spots[from[b]])
            {
                continue;
            }
            let Some(f) = fields.next() else { break };
            from.swap(b, f);
        }
    }
    Plan { from }
}

/// What every moved body's new row and map edits came to.
pub struct Applied {
    /// The maps that changed.
    pub maps: Vec<(MapId, Msb)>,
    /// The event scripts that changed.
    pub scripts: Vec<(MapId, fs_emevd::Emevd)>,
    /// Health bar text to write: NpcName id and name.
    pub bar_names: BTreeMap<i32, String>,
    pub new_rows: usize,
}

/// A copy of `body`'s row that keeps what belongs to `spot`: its drops and
/// runes always, its health and tier when scaling.
fn row_for(body: &Row, spot: &Row, scale: bool, npc: &fs_param::layout::Layout) -> Result<Vec<u8>> {
    let mut data = body.data.clone();
    let mut names = vec!["item_lot_enemy_id", "item_lot_map_id", "runes"];
    if scale {
        names.extend(["hp", "scale_a", "scale_b"]);
    }
    for name in names {
        let f = npc.field(name);
        f.set(&mut data, 0, f.get(&spot.data, 0))?;
    }
    Ok(data)
}

/// Point a part at a body: NpcParam row, AI, equipment, and a model the map
/// lists, adding one when it doesn't. Talk stops, as the voice was the old one's.
fn stand_on(msb: &mut Msb, part: usize, body: &Body, row: i32) -> Result<()> {
    let model = model_index(msb, &body.model)?;
    let t = msb
        .tables
        .iter_mut()
        .find(|t| t.name == PARTS)
        .ok_or_else(|| format_err("the map has no parts"))?;
    let e = &mut t.entries[part];
    field(PARTS, "npc_param").set(e, 0, i64::from(row))?;
    field(PARTS, "think").set(e, 0, i64::from(body.think))?;
    field(PARTS, "chara_init").set(e, 0, i64::from(body.chara))?;
    field(PARTS, "talk_id").set(e, 0, 0)?;
    field(PARTS, "model").set(e, 0, model as i64)?;
    Ok(())
}

/// The index of a creature model in the map's list, adding it if missing.
pub fn model_index(msb: &mut Msb, name: &str) -> Result<usize> {
    let t = msb
        .tables
        .iter_mut()
        .find(|t| t.name == MODELS)
        .ok_or_else(|| format_err("the map has no models"))?;
    if let Some(i) = t
        .entries
        .iter()
        .position(|e| entry_name(e).as_deref() == Some(name))
    {
        return Ok(i);
    }
    let i = t.entries.len();
    t.entries.push(new_model(name, i)?);
    Ok(i)
}

/// A creature model entry as the game's maps write them: name offset, type 2,
/// its own index, the offset of its source path, and how many parts use it.
fn new_model(name: &str, index: usize) -> Result<Vec<u8>> {
    let mut e = Vec::new();
    let name16: Vec<u8> = name
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    let sib_at = 0x28 + name16.len();
    let path = format!("N:\\GR\\data\\Model\\chr\\{name}\\sib\\{name}.sib");
    e.extend_from_slice(&0x28i64.to_le_bytes());
    e.extend_from_slice(&2i32.to_le_bytes());
    e.extend_from_slice(
        &i32::try_from(index)
            .map_err(|_| format_err("too many models"))?
            .to_le_bytes(),
    );
    e.extend_from_slice(&(sib_at as i64).to_le_bytes());
    e.extend_from_slice(&0i64.to_le_bytes());
    e.extend_from_slice(&0i64.to_le_bytes());
    e.extend_from_slice(&name16);
    for u in path.encode_utf16().chain([0]) {
        e.extend_from_slice(&u.to_le_bytes());
    }
    e.resize(e.len().next_multiple_of(8), 0);
    Ok(e)
}

/// Set each listed model's count of parts to what the map now holds.
fn recount_models(msb: &mut Msb) -> Result<()> {
    let mut users: HashMap<i64, i64> = HashMap::new();
    for e in &msb.table(PARTS).entries {
        if let Some(m) = field(PARTS, "model").get(e, 0) {
            *users.entry(m).or_default() += 1;
        }
    }
    let t = msb
        .tables
        .iter_mut()
        .find(|t| t.name == MODELS)
        .ok_or_else(|| format_err("the map has no models"))?;
    for (i, e) in t.entries.iter_mut().enumerate() {
        let n = users.get(&(i as i64)).copied().unwrap_or(0);
        if entry_name(e).is_some_and(|n| n.starts_with('c')) && e.len() >= 0x20 {
            e[0x18..0x20].copy_from_slice(&n.to_le_bytes());
        }
    }
    Ok(())
}

/// Write a plan into the regulation, the maps and the scripts.
pub fn apply(
    reg: &mut Regulation,
    s: &Sources,
    pool: &Pool,
    plan: &Plan,
    options: &Options,
) -> Result<Applied> {
    let npc = for_table("NpcParam").ok_or_else(|| format_err("no layout for NpcParam"))?;
    let mut table: Param = reg.table("NpcParam")?;
    if table.rows.iter().any(|r| r.id >= FIRST_NEW_ROW) {
        return Err(format_err(
            "NpcParam already has rows where this tool makes its own",
        ));
    }
    let rows: HashMap<i32, Row> = table.rows.iter().map(|r| (r.id, r.clone())).collect();
    let mut made: BTreeMap<Vec<u8>, i32> = BTreeMap::new();
    let mut new_rows: Vec<Row> = Vec::new();
    let mut maps: BTreeMap<usize, Msb> = BTreeMap::new();
    let mut bar_names: BTreeMap<i32, String> = BTreeMap::new();

    for (i, spot) in pool.spots.iter().enumerate() {
        let body = &pool.spots[plan.from[i]].body;
        if plan.from[i] == i {
            continue;
        }
        let (Some(b), Some(p)) = (rows.get(&body.npc), rows.get(&spot.body.npc)) else {
            return Err(format_err("a body's row has gone missing"));
        };
        let data = row_for(b, p, options.scale, npc)?;
        let next = FIRST_NEW_ROW + made.len() as i32;
        let id = *made.entry(data.clone()).or_insert_with(|| {
            new_rows.push(Row {
                id: next,
                data,
                name: b.name.clone(),
            });
            next
        });
        let msb = maps
            .entry(spot.map_index)
            .or_insert_with(|| s.maps[spot.map_index].1.clone());
        stand_on(msb, spot.part, body, id)?;
        if let Some(b) = &spot.boss {
            bar_names
                .entry(b.bar_text)
                .or_insert_with(|| body.name.clone());
        }
    }
    let new_rows_count = new_rows.len();
    table.rows.extend(new_rows);
    table.rows.sort_by_key(|r| r.id);
    reg.set_table("NpcParam", &table)?;
    for msb in maps.values_mut() {
        recount_models(msb)?;
    }
    let scripts = malenia::scripts(s, pool, plan)?;
    Ok(Applied {
        maps: maps.into_iter().map(|(i, m)| (s.maps[i].0, m)).collect(),
        scripts,
        bar_names,
        new_rows: new_rows_count,
    })
}

/// The seed's options and where every boss went, for the spoiler log; ordinary
/// enemies are counted only.
pub fn spoiler(pool: &Pool, plan: &Plan, options: &Options) -> String {
    let mut out = format!(
        "\nEnemies\n  bosses {}%  ordinary {}%  mix {}%  scale to the place: {}\n",
        options.bosses,
        options.ordinary,
        options.mix,
        if options.scale { "on" } else { "off" }
    );
    out += "\n  Bosses: who now stands where\n";
    for (i, s) in pool.spots.iter().enumerate() {
        if s.boss.is_some() {
            let b = &pool.spots[plan.from[i]].body;
            let who = if b.name.is_empty() { &b.model } else { &b.name };
            out += &format!("  {} ({}): {who}\n", s.body.name, s.map);
        }
    }
    let moved = (0..pool.spots.len()).filter(|&i| plan.from[i] != i).count();
    out += &format!(
        "\n  {moved} of {} spots hold someone new\n",
        pool.spots.len()
    );
    if !pool.fixed.is_empty() {
        out += "\n  Bosses left in place\n";
        let mut seen = BTreeSet::new();
        for f in &pool.fixed {
            if seen.insert((&f.name, &f.reason)) {
                out += &format!("  {}: {}\n", f.name, f.reason);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(i: usize, boss: bool, map: [u8; 4], size: Size) -> Spot {
        Spot {
            map_index: 0,
            part: i,
            map: MapId(map),
            entity: boss.then_some(i as u32 + 1),
            body: Body {
                npc: i as i32,
                think: 2,
                chara: -1,
                model: format!("c{i}"),
                name: format!("n{i}"),
            },
            size,
            dlc: false,
            boss: boss.then_some(BossSpot {
                bar_text: i as i32,
                script: MapId(map),
            }),
        }
    }

    fn sample() -> Pool {
        let mut spots = Vec::new();
        for i in 0..20 {
            spots.push(at(i, true, [10, 0, 0, 0], Size::Large));
        }
        for i in 20..200 {
            let size = [Size::Small, Size::Medium, Size::Large][i % 3];
            spots.push(at(i, false, [60, 40, 40, 0], size));
        }
        Pool {
            spots,
            fixed: vec![],
        }
    }

    fn opts(bosses: u8, ordinary: u8, mix: u8) -> Options {
        Options {
            bosses,
            ordinary,
            mix,
            ..Default::default()
        }
    }

    #[test]
    fn every_body_stands_once_and_a_seed_repeats() {
        let p = sample();
        let plan = shuffle(&p, 7, &opts(100, 100, 30));
        let mut seen = plan.from.clone();
        seen.sort_unstable();
        assert_eq!(seen, (0..p.spots.len()).collect::<Vec<_>>());
        assert_eq!(plan, shuffle(&p, 7, &opts(100, 100, 30)));
        assert_ne!(plan, shuffle(&p, 8, &opts(100, 100, 30)));
    }

    #[test]
    fn at_nought_nothing_moves() {
        let p = sample();
        let plan = shuffle(&p, 7, &Options::default());
        assert!(plan.from.iter().enumerate().all(|(i, &f)| i == f));
    }

    #[test]
    fn with_no_mixing_groups_and_sizes_stay_apart() {
        let p = sample();
        let plan = shuffle(&p, 3, &opts(100, 100, 0));
        for (i, &f) in plan.from.iter().enumerate() {
            let (a, b) = (&p.spots[i], &p.spots[f]);
            assert_eq!(a.boss.is_some(), b.boss.is_some());
            if a.boss.is_none() {
                assert_eq!(a.size, b.size);
            }
        }
    }

    #[test]
    fn mixing_sends_bosses_to_open_ground_only() {
        let p = sample();
        let plan = shuffle(&p, 3, &opts(100, 0, 100));
        let crossed = plan
            .from
            .iter()
            .enumerate()
            .filter(|(i, f)| p.spots[*i].boss.is_some() != p.spots[**f].boss.is_some())
            .count();
        assert_eq!(crossed, 40, "all twenty trade places, both ways");
        for (i, &f) in plan.from.iter().enumerate() {
            if p.spots[f].boss.is_some() && p.spots[i].boss.is_none() {
                assert!(p.spots[i].map.is_open_world());
            }
        }
    }

    #[test]
    fn a_new_model_entry_reads_like_the_games() {
        let e = new_model("c2130", 9).unwrap();
        assert_eq!(entry_name(&e).as_deref(), Some("c2130"));
        assert_eq!(e[8], 2);
        assert_eq!(e[0xC], 9);
        assert_eq!(e.len() % 8, 0);
    }

    #[test]
    fn sliders_are_read_by_name() {
        let mut o = Options::default();
        o.set("bosses=100,mix=5%").unwrap();
        assert_eq!((o.bosses, o.ordinary, o.mix), (100, 0, 5));
        assert!(o.set("dragons=10").is_err());
        assert!(o.set("bosses=101").is_err());
    }
}

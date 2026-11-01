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
pub mod placidusax;

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
const EVENTS: &str = "EVENT_PARAM_ST";
/// The event type that ties a rider's part to a mount's.
const MOUNT_EVENT: i64 = 21;
/// The event type that makes copies of its parts as a fight goes on.
const GENERATOR_EVENT: i64 = 5;
/// An enemy that takes part in play. Type 10 stands in cutscenes and the like.
const ENEMY: i64 = 2;
/// Mixed into the seed, so enemies draw apart from items.
const SALT: u64 = 0x454E_454D_4945_5321;
/// Where the rows made for a seed start, unless a mod's already go higher. The
/// game's own stop below this.
const FIRST_NEW_ROW: i32 = 800_000_000;
/// Models with no creature of their own: the player's form, and a stand-in.
const NOT_BODIES: [&str; 1] = ["c1000"];
/// Enemies the area map counts on doing what they do. No row names these, so
/// they go by model. An Abductor Virgin's grab is a way to the Volcano Manor: c4470,
/// the one tall body placed in the academy, the capital and the Manor, where public
/// guides list Virgins. A Walking Mausoleum carries a shop: c4450, the part all
/// seven of its open-world remembrance talk scripts stand on (`apoc-fs name-models`).
const STAY_MODELS: [&str; 2] = ["c4470", "c4450"];
/// Stand-ins, which do carry the word in their names.
const STAY: [&str; 1] = ["dummy"];
/// NpcParam teams of those who do not fight the player until a script says so:
/// no team (Gideon), friends (Patches, merchants) and summonable allies (Millicent).
const NOT_HOSTILE: [i64; 5] = [0, 1, 2, 26, 28];
/// NpcParam team of invaders, when built like a player: Millicent or Anastasia come to fight.
const INVADER_TEAM: i64 = 27;
/// Events that name a boss's number and others' for reasons not to do with a fight:
/// a start-up list (0, 50, 100), "stay gone once this flag is on" (90005300, whose flag can
/// be the boss's number) and the invasion events shared by a map's invaders.
const NOT_A_FIGHT: [i64; 8] = [0, 50, 100, 90005300, 90005790, 90005791, 90005792, 90005793];

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
    /// The most models ordinary enemies' moves may add to a map, 0 for no limit.
    /// Bosses are not counted. The game's own limit is not known: its largest
    /// map lists 50, and with no limit a seed adds up to 101 to one.
    pub models: u16,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            bosses: 0,
            ordinary: 0,
            mix: 0,
            scale: true,
            // Apart unless asked: mixing them is said to want a helper that raises
            // the game's limits, which we do not have (PLAN.md §6a).
            dlc_separate: true,
            models: 20,
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
            if name == "models" {
                self.models = value
                    .parse()
                    .map_err(|_| format!("'{part}': models takes a count"))?;
                continue;
            }
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
                _ => {
                    return Err(format!(
                        "'{name}': expected bosses, ordinary, mix or models"
                    ));
                }
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

/// Who a spot's body may trade with among ordinary enemies. An invader's spot has events
/// that bring its body in and keep it gone once dead, which a plain spot lacks, so an
/// invader elsewhere would come back at every rest. Humans apart from plain enemies keeps
/// each map to the human-built bodies it has unmodded, whose number the game limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Class {
    #[default]
    Plain,
    Human,
    Invader,
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
    /// The `NpcName` text it goes by, in every language; 0 if none.
    pub name_text: i32,
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
    pub class: Class,
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
    /// The creature models each map lists, by `Spot::map_index`.
    pub listed: Vec<BTreeSet<String>>,
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

/// The parts a map's events tie to what they are. A mount event sets up a ride
/// between two parts and stays when a body changes, so the game rides bodies that
/// have no ride: it crashed around Stormveil, with its 16 such events. A generator
/// goes on making copies of its parts, and nearly all make one model: a large body
/// there would come again and again.
fn tied_parts(msb: &Msb) -> HashSet<usize> {
    let mut out = HashSet::new();
    let mut add = |p: Option<i64>| out.extend(p.and_then(|p| usize::try_from(p).ok()));
    for e in &msb.table(EVENTS).entries {
        match field(EVENTS, "type").get(e, 0) {
            Some(MOUNT_EVENT) => {
                add(field(EVENTS, "rider_part").get(e, 0));
                add(field(EVENTS, "mount_part").get(e, 0));
            }
            Some(GENERATOR_EVENT) => {
                let list = field(EVENTS, "generator_part");
                (0..32).for_each(|i| add(list.get(e, i)));
            }
            _ => {}
        }
    }
    out
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
    // Bodies that fall and get up as a next stage: 4[14] with 5 then 1. The plain
    // fall is 5 then 0. Such a body never falls plainly, so it moves only with
    // its script, as Malenia's does, and only its own body starts that script.
    let mut rises: HashSet<u32> = HashSet::new();
    let defeat = catalogue::named("boss_defeated");
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
                if (i.bank, i.id) == (4, 14) {
                    let w: Vec<u32> = words(i).collect();
                    if w.len() >= 4 && w[2] == 5 && w[3] == 1 && w[1] != malenia::HER_ENTITY {
                        rises.insert(w[1]);
                    }
                    // A fall the same event then calls a defeat ends a fight too.
                    if w.len() >= 4 && w[2] == 5 && w[3] == 0 {
                        let ends = run.instructions.iter().any(|d| {
                            (d.bank, d.id) == (defeat.bank, defeat.id)
                                && words(d).next() == Some(w[1])
                        });
                        if ends {
                            waited.insert(w[1]);
                        }
                    }
                }
            }
        }
    }
    // Those a fight's own events act on: Malenia's copies, Messmer's snakes, the Fire
    // Giant's second body. Away from it they would stand about as strays, and a stranger
    // in their place would be told what they were told.
    let mut helpers: HashSet<u32> = HashSet::new();
    for (_, script) in &s.scripts {
        for run in catalogue::runs(script, Some(&s.common_func)) {
            if NOT_A_FIGHT.contains(&run.event.id) {
                continue;
            }
            // Bank 2000 starts other events, which are read on their own.
            let acts = || {
                run.instructions
                    .iter()
                    .filter(|i| i.bank != 2000)
                    .flat_map(words)
            };
            if acts().any(|w| bars.contains_key(&w)) {
                helpers.extend(acts().filter(|w| !bars.contains_key(w)));
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
    let mut twice: BTreeSet<u32> = BTreeSet::new();
    out.listed = s
        .maps
        .iter()
        .map(|(_, msb)| {
            msb.table(MODELS)
                .entries
                .iter()
                .filter_map(|e| entry_name(e))
                .filter(|n| n.starts_with('c'))
                .collect()
        })
        .collect();
    for (map_index, (m, msb)) in s.maps.iter().enumerate() {
        let models = &msb.table(MODELS).entries;
        let tied = tied_parts(msb);
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
            let name_text = npc.field("name_id").get(&row.data, 0) as i32;
            let name = Some(name_text)
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
                name_text: name_text.max(0),
            };
            let size = Size::of(
                npc.field("height").get_f32(&row.data, 0),
                npc.field("radius").get_f32(&row.data, 0),
            );
            let team = npc.field("team").get(&row.data, 0);
            let hostile = !NOT_HOSTILE.contains(&team);
            // Team 27 also holds plain creatures clustered in a few maps, so the model
            // has to be a human's too.
            let class = match (model_name == "c0000", team == INVADER_TEAM) {
                (true, true) => Class::Invader,
                (true, false) => Class::Human,
                (false, _) => Class::Plain,
            };
            let spot = |boss| Spot {
                map_index,
                part,
                map: *m,
                entity,
                body: body.clone(),
                size,
                class,
                dlc: is_dlc(*m),
                boss,
            };
            if let Some((script, bar_text)) = entity.and_then(|en| bars.get(&en)) {
                let en = entity.unwrap_or(0);
                let shown = s.text("NpcName", *bar_text).unwrap_or(&name).to_string();
                let refuse = if shown.contains("dummy") {
                    Some("a stand-in with no fight of its own")
                } else if tied.contains(&part) {
                    Some("a map event ties it to its rider, mount or generator")
                } else if rises.contains(&en) {
                    Some("it gets up as a next stage, and its script stays here")
                } else if !hostile {
                    Some("friendly until a script turns it, which would not happen elsewhere")
                } else if placed.contains_key(&en) {
                    twice.insert(en);
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
                sp.body.name_text = *bar_text;
                out.spots.push(sp);
                continue;
            }
            let body_ok = !NOT_BODIES.contains(&model_name.as_str())
                && !tied.contains(&part)
                && (model_name != "c0000" || chara > 0)
                && npc.field("hp").get(&row.data, 0) > 1
                && body.think > 1
                && talk <= 0
                && hostile
                && !entity.is_some_and(|en| rises.contains(&en))
                && !entity.is_some_and(|en| area_bosses.contains(&en))
                && !entity.is_some_and(|en| helpers.contains(&en))
                && !STAY_MODELS.contains(&model_name.as_str())
                && !STAY.iter().any(|n| name.contains(n))
                // A named character who is not built like a player is someone's
                // quest: Blaidd, Alexander. Named humans here are hostile ones.
                && (name.is_empty() || chara > 0);
            if body_ok {
                out.spots.push(spot(None));
            }
        }
    }
    // Neither copy of a boss placed twice moves: one moved alone would leave
    // two bodies under one entity.
    out.spots
        .retain(|s| s.boss.is_none() || !s.entity.is_some_and(|en| twice.contains(&en)));
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
    let mut ordinary: BTreeMap<(bool, Size, Class), Vec<usize>> = BTreeMap::new();
    for (i, s) in pool.spots.iter().enumerate() {
        if s.boss.is_some() {
            bosses.entry(side(s)).or_default().push(i);
        } else {
            ordinary
                .entry((side(s), s.size, s.class))
                .or_default()
                .push(i);
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
            .filter(|(_, s)| {
                s.boss.is_none()
                    && s.class == Class::Plain
                    && s.map.is_open_world()
                    && side(s) == dlc
            })
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
    if options.ordinary > 0 && options.models > 0 {
        cap_models(pool, &mut from, usize::from(options.models));
    }
    Plan { from }
}

/// The models each map must add for the ordinary bodies that now stand in it. A
/// boss's model does not count against the limit (`docs/spec/enemies.md` §2).
pub fn new_models<'a>(pool: &'a Pool, from: &[usize]) -> BTreeMap<usize, BTreeMap<&'a str, usize>> {
    let mut out: BTreeMap<usize, BTreeMap<&str, usize>> = BTreeMap::new();
    for (i, s) in pool.spots.iter().enumerate() {
        let body = &pool.spots[from[i]];
        let model = body.body.model.as_str();
        if body.boss.is_none() && !pool.listed[s.map_index].contains(model) {
            *out.entry(s.map_index)
                .or_default()
                .entry(model)
                .or_default() += 1;
        }
    }
    out
}

/// Bring every map down to `cap` added models where swaps can. A map keeps the
/// models it has most of; an ordinary enemy of any other trades with one of its
/// own size elsewhere whose model this map can take, if the other map can take
/// this one. Swaps keep every body landing once. Bosses are not touched.
fn cap_models(pool: &Pool, from: &mut [usize], cap: usize) {
    let mut by_group: BTreeMap<(bool, Size, Class), Vec<usize>> = BTreeMap::new();
    let mut by_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, s) in pool.spots.iter().enumerate() {
        if s.boss.is_none() {
            by_group
                .entry((s.dlc, s.size, s.class))
                .or_default()
                .push(i);
            by_map.entry(s.map_index).or_default().push(i);
        }
    }
    let mut added = new_models(pool, from);
    // A swap can open room for a later one, so go round while any helps.
    for _ in 0..4 {
        let mut swapped = false;
        for (&m, spots) in &by_map {
            if added.get(&m).map_or(0, BTreeMap::len) <= cap {
                continue;
            }
            let mut sizes: Vec<(&str, usize)> = added[&m].iter().map(|(&k, &n)| (k, n)).collect();
            sizes.sort_by_key(|&(k, n)| (std::cmp::Reverse(n), k));
            let kept: BTreeSet<&str> = sizes.iter().take(cap).map(|&(k, _)| k).collect();
            for &i in spots {
                let body = pool.spots[from[i]].body.model.as_str();
                if pool.listed[m].contains(body)
                    || kept.contains(body)
                    || pool.spots[from[i]].boss.is_some()
                {
                    continue;
                }
                let s = &pool.spots[i];
                let Some(&j) = by_group[&(s.dlc, s.size, s.class)].iter().find(|&&j| {
                    let other = pool.spots[j].map_index;
                    let theirs = pool.spots[from[j]].body.model.as_str();
                    let there = added.get(&other);
                    other != m
                        && pool.spots[from[j]].boss.is_none()
                        && (pool.listed[m].contains(theirs) || kept.contains(theirs))
                        && (pool.listed[other].contains(body)
                            || there.is_none_or(|a| a.contains_key(body) || a.len() < cap))
                }) else {
                    continue;
                };
                let other = pool.spots[j].map_index;
                let theirs = pool.spots[from[j]].body.model.as_str();
                take(added.entry(m).or_default(), body);
                if !pool.listed[m].contains(theirs) {
                    *added.entry(m).or_default().entry(theirs).or_default() += 1;
                }
                if !pool.listed[other].contains(theirs) {
                    take(added.entry(other).or_default(), theirs);
                }
                if !pool.listed[other].contains(body) {
                    *added.entry(other).or_default().entry(body).or_default() += 1;
                }
                from.swap(i, j);
                swapped = true;
            }
        }
        if !swapped {
            break;
        }
    }
}

/// One fewer of `model`, forgetting it at none.
fn take(counts: &mut BTreeMap<&str, usize>, model: &str) {
    if let Some(n) = counts.get_mut(model) {
        *n -= 1;
        if *n == 0 {
            counts.remove(model);
        }
    }
}

/// What every moved body's new row and map edits came to.
pub struct Applied {
    /// The maps that changed.
    pub maps: Vec<(MapId, Msb)>,
    /// The event scripts that changed.
    pub scripts: Vec<(MapId, fs_emevd::Emevd)>,
    /// Health bar text to write: NpcName id and name.
    pub bar_names: BTreeMap<i32, String>,
    /// For each of those, the text id its name comes from, for other languages.
    pub bar_sources: BTreeMap<i32, i32>,
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
    // A mod may already have rows up here (ELDEN VINS does), so new ones follow them.
    let first_new = table
        .rows
        .iter()
        .map(|r| r.id)
        .max()
        .map_or(FIRST_NEW_ROW, |top| {
            top.saturating_add(1).max(FIRST_NEW_ROW)
        });
    let rows: HashMap<i32, Row> = table.rows.iter().map(|r| (r.id, r.clone())).collect();
    let mut made: BTreeMap<Vec<u8>, i32> = BTreeMap::new();
    let mut new_rows: Vec<Row> = Vec::new();
    let mut maps: BTreeMap<usize, Msb> = BTreeMap::new();
    let mut bar_names: BTreeMap<i32, String> = BTreeMap::new();
    let mut bar_sources: BTreeMap<i32, i32> = BTreeMap::new();

    for (i, spot) in pool.spots.iter().enumerate() {
        let body = &pool.spots[plan.from[i]].body;
        if plan.from[i] == i {
            continue;
        }
        let (Some(b), Some(p)) = (rows.get(&body.npc), rows.get(&spot.body.npc)) else {
            return Err(format_err("a body's row has gone missing"));
        };
        let data = row_for(b, p, options.scale, npc)?;
        let next = first_new
            .checked_add(made.len() as i32)
            .ok_or_else(|| format_err("NpcParam has no ids left for new rows"))?;
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
            bar_sources.entry(b.bar_text).or_insert(body.name_text);
        }
    }
    let new_rows_count = new_rows.len();
    table.rows.extend(new_rows);
    table.rows.sort_by_key(|r| r.id);
    reg.set_table("NpcParam", &table)?;
    for msb in maps.values_mut() {
        recount_models(msb)?;
    }
    let mut scripts = malenia::scripts(s, pool, plan)?;
    placidusax::apply(s, pool, plan, &mut scripts)?;
    Ok(Applied {
        maps: maps.into_iter().map(|(i, m)| (s.maps[i].0, m)).collect(),
        scripts,
        bar_names,
        bar_sources,
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
    let added = new_models(pool, &plan.from);
    let most = added.values().map(BTreeMap::len).max().unwrap_or(0);
    let total: usize = added.values().map(BTreeMap::len).sum();
    out += &format!(
        "  {} maps gain {total} models, at most {most} in one map; limit {}\n",
        added.len(),
        if options.models == 0 {
            "none".to_string()
        } else {
            options.models.to_string()
        }
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
                name_text: 0,
            },
            size,
            class: Class::Plain,
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
            listed: vec![BTreeSet::new()],
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
    fn invaders_and_humans_trade_only_among_themselves() {
        let mut p = sample();
        for (i, s) in p.spots.iter_mut().enumerate().skip(20) {
            s.class = [Class::Plain, Class::Human, Class::Invader][i % 3];
        }
        for seed in 0..20 {
            let plan = shuffle(&p, seed, &opts(100, 100, 100));
            let mut moved = 0;
            for (i, &f) in plan.from.iter().enumerate() {
                let (a, b) = (&p.spots[i], &p.spots[f]);
                if a.boss.is_none() && b.boss.is_none() {
                    assert_eq!(a.class, b.class);
                    moved += usize::from(i != f && a.class != Class::Plain);
                }
                // A boss sent out takes a plain spot, so no invader ends up in an arena.
                if a.boss.is_some() && b.boss.is_none() {
                    assert_eq!(b.class, Class::Plain);
                }
            }
            assert!(moved > 0);
        }
    }

    /// Written from Linux and held on every system CI runs, none of which has the game:
    /// a plan drawn differently anywhere would change every seed there.
    #[test]
    fn a_plan_is_the_same_on_every_system() {
        let plan = shuffle(&sample(), 12345, &opts(100, 100, 30));
        let hash = plan.from.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &f| {
            (h ^ f as u64).wrapping_mul(0x100_0000_01b3)
        });
        assert_eq!(hash, 2038416491583041321);
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
    fn a_bosss_model_does_not_count_against_the_limit() {
        let mut boss = at(0, true, [10, 0, 0, 0], Size::Large);
        boss.body.model = "c9000".into();
        let mut plain = at(1, false, [10, 0, 0, 0], Size::Small);
        plain.body.model = "c1000".into();
        let p = Pool {
            spots: vec![boss, plain],
            fixed: vec![],
            listed: vec![BTreeSet::new()],
        };
        let added = new_models(&p, &[0, 1]);
        assert_eq!(added[&0].keys().copied().collect::<Vec<_>>(), ["c1000"]);
    }

    #[test]
    fn a_limit_on_models_holds_and_every_body_still_lands_once() {
        let mut spots = Vec::new();
        let mut listed = Vec::new();
        for m in 0..10usize {
            listed.push(BTreeSet::from([format!("c{m}")]));
            for _ in 0..20 {
                let mut s = at(spots.len(), false, [60, 40, m as u8, 0], Size::Small);
                s.map_index = m;
                s.body.model = format!("c{m}");
                spots.push(s);
            }
        }
        let p = Pool {
            spots,
            fixed: vec![],
            listed,
        };
        let free = shuffle(
            &p,
            5,
            &Options {
                models: 0,
                ..opts(0, 100, 0)
            },
        );
        let most = |f: &[usize]| new_models(&p, f).values().map(BTreeMap::len).max().unwrap();
        assert!(most(&free.from) > 2);
        let capped = shuffle(
            &p,
            5,
            &Options {
                models: 2,
                ..opts(0, 100, 0)
            },
        );
        assert!(most(&capped.from) <= 2);
        let mut seen = capped.from.clone();
        seen.sort_unstable();
        assert_eq!(seen, (0..p.spots.len()).collect::<Vec<_>>());
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

//! The open world's other versions of a tile. Beside `m60_45_52_00` the game
//! holds `m60_45_52_10`, the same ground in a later state of the world: that
//! one links to the Ashen Capital (`m11_05`) where the first links to Leyndell
//! (`m11_00`), and `m61_44_45_10` links to the finished spiral tower of
//! Enir-Ilim. Of 67 such twins (`_10` to `_12` beside `_00` to `_02`), all but
//! six are byte for byte their tile. That the game loads a twin in place of its
//! tile once the world has changed is read from these links, not seen in play;
//! if so, a seed that moved enemies only on the tile would give them back after.
//!
//! A twin that was its tile's copy becomes a copy of the tile as the seed left
//! it. In a twin that differs, each changed enemy is found by its old bytes, its
//! entity id and model, or its name and model, and given the new body, AI,
//! model, place and entity id. Enemies only a twin has stay as they are, and so
//! do the tile's enemies no twin's part matches alone (seed 1: 124 of the
//! enemies changed in differing twins' tiles, 105 of them absent from the
//! twins outside Leyndell and in east Limgrave). A changed script follows into a twin whose
//! script was its tile's copy, and no other.

use std::collections::{HashMap, HashSet};

use fs_bytes::{Result, format_err};
use fs_emevd::Emevd;
use fs_msb::Msb;
use fs_msb::layout::{entry_name, field};
use gamedata::{GameFiles, MapId, Sources};

const PARTS: &str = "PARTS_PARAM_ST";
const MODELS: &str = "MODEL_PARAM_ST";

/// The fields a moved enemy changes, as `enemies` and its companions set them.
const MOVED: [&str; 7] = [
    "npc_param",
    "think",
    "chara_init",
    "talk_id",
    "position",
    "entity_id",
    "model",
];

/// The later version of a tile: `_00` to `_02` become `_10` to `_12`.
pub fn twin(m: MapId) -> Option<MapId> {
    let [a, b, c, d] = m.0;
    (m.is_open_world() && d < 10).then_some(MapId([a, b, c, d + 10]))
}

fn map_path(m: MapId) -> String {
    format!("/map/mapstudio/{m}.msb.dcx")
}

fn script_path(m: MapId) -> String {
    format!("/event/{m}.emevd.dcx")
}

/// Every twin's map and script a seed may read, for those that fetch ahead.
pub fn paths(files: &GameFiles) -> Vec<String> {
    Sources::paths(files)
        .iter()
        .filter_map(|p| MapId::find_in(p))
        .filter_map(twin)
        .collect::<HashSet<_>>()
        .into_iter()
        .flat_map(|t| [map_path(t), script_path(t)])
        .filter(|p| files.exists(p))
        .collect()
}

/// What the twins came to.
#[derive(Default)]
pub struct Twins {
    pub maps: Vec<(MapId, Msb)>,
    pub scripts: Vec<(MapId, Emevd)>,
    /// Twins that differ from their tile, so took enemies one at a time.
    pub differing: usize,
    /// Changed enemies in those that no part of the twin matched.
    pub unmatched: usize,
}

fn plain(files: &GameFiles, path: &str) -> Result<Vec<u8>> {
    Ok(fs_dcx::decompress(&files.read(path)?)?.1)
}

/// Carry the seed's changes to the tiles in `maps` and `scripts` over to their twins.
pub fn follow(
    files: &GameFiles,
    sources: &Sources,
    maps: &[(MapId, Msb)],
    scripts: &[(MapId, Emevd)],
) -> Result<Twins> {
    let old_maps: HashMap<MapId, &Msb> = sources.maps.iter().map(|(m, x)| (*m, x)).collect();
    let mut out = Twins::default();
    for (m, new) in maps {
        let Some(t) = twin(*m) else { continue };
        if !files.exists(&map_path(t)) {
            continue;
        }
        let old = old_maps
            .get(m)
            .ok_or_else(|| format_err(format!("{m}: changed but never read")))?;
        let theirs = Msb::read(&plain(files, &map_path(t))?)?;
        if &theirs == *old {
            out.maps.push((t, new.clone()));
        } else {
            out.differing += 1;
            let (msb, unmatched) = carry(old, new, theirs)?;
            out.unmatched += unmatched;
            out.maps.push((t, msb));
        }
    }
    for (m, new) in scripts {
        let Some(t) = twin(*m) else { continue };
        // A script follows only into a twin whose script was the tile's copy.
        if !files.exists(&script_path(t)) || !files.exists(&script_path(*m)) {
            continue;
        }
        if plain(files, &script_path(t))? == plain(files, &script_path(*m))? {
            out.scripts.push((t, new.clone()));
        }
    }
    Ok(out)
}

fn model_name(msb: &Msb, part: &[u8]) -> Option<String> {
    let i = field(PARTS, "model").get(part, 0)?;
    entry_name(msb.table(MODELS).entries.get(usize::try_from(i).ok()?)?)
}

/// `theirs` with each enemy that changed from `old` to `new` changed the same way.
fn carry(old: &Msb, new: &Msb, mut theirs: Msb) -> Result<(Msb, usize)> {
    let (before, after) = (&old.table(PARTS).entries, &new.table(PARTS).entries);
    let mut unmatched = 0;
    for (i, now) in after.iter().enumerate() {
        let was = before.get(i);
        if was == Some(now) {
            continue;
        }
        let Some(was) = was else {
            unmatched += 1;
            continue;
        };
        let entity = field(PARTS, "entity_id").get(was, 0).unwrap_or(0);
        let model = model_name(old, was);
        let parts = &theirs.table(PARTS).entries;
        let j = parts
            .iter()
            .position(|e| e == was)
            .or_else(|| {
                (entity > 0)
                    .then(|| {
                        parts.iter().position(|e| {
                            field(PARTS, "entity_id").get(e, 0) == Some(entity)
                                && model_name(&theirs, e) == model
                        })
                    })
                    .flatten()
            })
            .or_else(|| {
                // Names are a map's own, but a twin keeps most of its tile's.
                let name = entry_name(was);
                let mut same = parts
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| entry_name(e) == name && model_name(&theirs, e) == model);
                match (same.next(), same.next()) {
                    (Some((j, _)), None) => Some(j),
                    _ => None,
                }
            });
        let Some(j) = j else {
            unmatched += 1;
            continue;
        };
        let name = model_name(new, now).ok_or_else(|| format_err("a moved enemy has no model"))?;
        let at = enemies::model_index(&mut theirs, &name)?;
        let t = theirs
            .tables
            .iter_mut()
            .find(|t| t.name == PARTS)
            .ok_or_else(|| format_err("the twin has no parts"))?;
        let e = &mut t.entries[j];
        for f in MOVED {
            let f = field(PARTS, f);
            for k in 0..f.count {
                let v = if f.name == "model" {
                    Some(at as i64)
                } else {
                    f.get(now, k)
                };
                if let Some(v) = v {
                    f.set(e, k, v)?;
                }
            }
        }
    }
    recount(&mut theirs);
    Ok((theirs, unmatched))
}

/// Each creature model's count of the parts that use it.
fn recount(msb: &mut Msb) {
    let mut users: HashMap<i64, i64> = HashMap::new();
    for e in &msb.table(PARTS).entries {
        if let Some(m) = field(PARTS, "model").get(e, 0) {
            *users.entry(m).or_default() += 1;
        }
    }
    if let Some(t) = msb.tables.iter_mut().find(|t| t.name == MODELS) {
        for (i, e) in t.entries.iter_mut().enumerate() {
            let n = users.get(&(i as i64)).copied().unwrap_or(0);
            if entry_name(e).is_some_and(|n| n.starts_with('c')) && e.len() >= 0x20 {
                e[0x18..0x20].copy_from_slice(&n.to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_has_its_twin_one_level_up() {
        assert_eq!(twin(MapId([60, 45, 52, 0])), Some(MapId([60, 45, 52, 10])));
        assert_eq!(twin(MapId([61, 11, 11, 2])), Some(MapId([61, 11, 11, 12])));
        assert_eq!(twin(MapId([60, 45, 52, 10])), None);
        assert_eq!(twin(MapId([11, 0, 0, 0])), None);
    }
}

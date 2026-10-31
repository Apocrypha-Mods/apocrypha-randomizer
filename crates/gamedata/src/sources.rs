//! The game files the model is extracted from, read once: `regulation.bin`,
//! every map, event script and talk script, and the English item text.
//!
//! Maps and scripts are found by name (`mAA_BB_CC_DD`): 1,248 maps, 542
//! event scripts and 36 talk binders (all 365 talk scripts) in build 25080141. The
//! rest of each kind is named some other way, still unknown (docs/progress.md),
//! so the model does not see them.

use std::collections::HashMap;
use std::fmt;

use fs_bnd4::Bnd4;
use fs_bytes::{Result, format_err};
use fs_dvdbnd::Dvd;
use fs_emevd::Emevd;
use fs_esd::Esd;
use fs_fmg::Fmg;
use fs_msb::Msb;
use fs_param::Regulation;
use rayon::prelude::*;
use serde::Serialize;

/// A map, `mAA_BB_CC_DD`. `AA` is the area: 60 the open world, 61 the Land
/// of Shadow, the others legacy dungeons and smaller places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapId(pub [u8; 4]);

/// Not a map: talk scripts that run anywhere, such as the grace menu.
pub const EVERYWHERE: MapId = MapId([0, 0, 0, 0]);

impl MapId {
    pub fn area(self) -> u8 {
        self.0[0]
    }

    /// Whether the map is a tile of one of the two open worlds.
    pub fn is_open_world(self) -> bool {
        matches!(self.area(), 60 | 61)
    }

    /// A position in an open-world tile as a point on the whole world's
    /// ground (x, z), or `None` for any other map.
    ///
    /// A tile `m60_XX_YY_L` is 256 units wide at level 0, 512 at 1 and 1,024
    /// at 2, and positions in it are measured from its centre (the middle
    /// 98% of parts lie within ±128, ±256 and ±512). Checked on the graces:
    /// the world position of every one of the 213 open-world graces lands in
    /// a level-0 tile that exists, and for 202 of them that is the tile its
    /// entity id names (1033401950 is in m60_33_40).
    pub fn world(self, local: [f32; 3]) -> Option<[f32; 2]> {
        if !self.is_open_world() {
            return None;
        }
        let size = 256.0 * f32::from(1u8 << self.0[3].min(7));
        let at = |i: u8, v: f32| f32::from(i) * size + size / 2.0 + v;
        Some([at(self.0[1], local[0]), at(self.0[2], local[2])])
    }

    /// From a name containing `mAA_BB_CC_DD`, such as an archive path.
    pub fn find_in(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        (0..b.len().saturating_sub(11)).find_map(|i| {
            let w = &b[i..i + 12];
            let digits = |j: usize| -> Option<u8> {
                let (a, b) = (w[j], w[j + 1]);
                (a.is_ascii_digit() && b.is_ascii_digit()).then(|| (a - b'0') * 10 + (b - b'0'))
            };
            if w[0] != b'm' || w[3] != b'_' || w[6] != b'_' || w[9] != b'_' {
                return None;
            }
            Some(MapId([digits(1)?, digits(4)?, digits(7)?, digits(10)?]))
        })
    }
}

impl fmt::Display for MapId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "m{a:02}_{b:02}_{c:02}_{d:02}")
    }
}

impl Serialize for MapId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// The text binder that carries the base game's names and both DLC sets.
const ITEM_TEXT: &str = "/msg/engus/item_dlc02.msgbnd.dcx";

/// Text tables by name (`GoodsName`, `PlaceName`, …), each id's English text.
pub type Text = HashMap<String, HashMap<i32, String>>;

pub struct Sources {
    pub reg: Regulation,
    pub maps: Vec<(MapId, Msb)>,
    /// Each map's script, where it has one.
    pub scripts: Vec<(MapId, Emevd)>,
    /// The shared events that map scripts start (`common_func`).
    pub common_func: Emevd,
    /// The script that runs everywhere (`common`).
    pub common: Emevd,
    /// Each map's talk scripts (NPC dialogue), by name (`t…`).
    pub talk: Vec<(MapId, String, Esd)>,
    pub text: Text,
}

/// The game's files as the game will see them: a mod folder's copy of a file
/// over the archives' copy, as me3 serves it.
pub struct GameFiles {
    dvd: Dvd,
    /// By lowercase path from the game's root, `/msg/engus/item.msgbnd.dcx`:
    /// mods name folders in any case (`map/MapStudio`), and the game does not mind.
    loose: HashMap<String, std::path::PathBuf>,
    /// Paths more than one mod folder holds; the later one is used.
    pub shadowed: Vec<String>,
}

impl GameFiles {
    /// The game folder, with mod folders over it, lowest priority first.
    pub fn open(game: &std::path::Path, mods: &[std::path::PathBuf]) -> Result<Self> {
        fn walk(dir: &std::path::Path, rel: &str, found: &mut Vec<(String, std::path::PathBuf)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                let rel = format!("{rel}/{name}");
                if e.path().is_dir() {
                    walk(&e.path(), &rel, found);
                } else {
                    found.push((rel, e.path()));
                }
            }
        }
        let mut loose = HashMap::new();
        let mut shadowed = Vec::new();
        for dir in mods {
            let mut found = Vec::new();
            walk(dir, "", &mut found);
            for (rel, path) in found {
                if loose.insert(rel.clone(), path).is_some() {
                    shadowed.push(rel);
                }
            }
        }
        shadowed.sort();
        shadowed.dedup();
        Ok(GameFiles {
            dvd: Dvd::open_elden_ring(game)?,
            loose,
            shadowed,
        })
    }

    pub fn exists(&self, path: &str) -> bool {
        self.loose.contains_key(&path.to_lowercase()) || self.dvd.find(path).is_some()
    }

    /// A file's bytes as stored, compression and all.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        match self.loose.get(&path.to_lowercase()) {
            Some(file) => Ok(std::fs::read(file)?),
            None => Ok(self.dvd.read_path(path)?),
        }
    }
}

impl Sources {
    /// Read everything from a game folder (the stock copy, PLAN.md §5).
    pub fn open(game: &std::path::Path) -> Result<Self> {
        Self::open_with(game, &GameFiles::open(game, &[])?)
    }

    /// Read everything as `files` serves it. The tables stay the game's own:
    /// merging them is the caller's, as mods' tables combine and files do not.
    pub fn open_with(game: &std::path::Path, dvd: &GameFiles) -> Result<Self> {
        let read = |path: &str| -> Result<Vec<u8>> {
            let bytes = dvd
                .read(path)
                .map_err(|e| format_err(format!("{path}: {e}")))?;
            Ok(fs_dcx::decompress(&bytes)
                .map_err(|e| format_err(format!("{path}: {e}")))?
                .1)
        };
        let ids = map_ids(dvd);
        let maps = ids
            .par_iter()
            .map(|&m| {
                Ok((m, {
                    let path = format!("/map/mapstudio/{m}.msb.dcx");
                    Msb::read(&read(&path)?).map_err(|e| format_err(format!("{path}: {e}")))?
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        let scripts = ids
            .par_iter()
            .filter_map(|&m| {
                let path = format!("/event/{m}.emevd.dcx");
                dvd.exists(&path)
                    .then(|| Ok((m, Emevd::read(&read(&path)?)?)))
            })
            .collect::<Result<Vec<_>>>()?;
        // Talk binders are named by map too, but the open worlds' NPCs sit in
        // m60_00_00_00 and m61_00_00_00, which have no map file, and the
        // grace menu (Melina among it) in m00_00_00_00.
        let mut talk_ids = named(dvd, |m| format!("/script/talk/{m}.talkesdbnd.dcx"));
        if dvd.exists(&format!("/script/talk/{EVERYWHERE}.talkesdbnd.dcx")) {
            talk_ids.insert(0, EVERYWHERE);
        }
        let talk = talk_ids
            .par_iter()
            .filter_map(|&m| {
                let path = format!("/script/talk/{m}.talkesdbnd.dcx");
                dvd.exists(&path).then(|| -> Result<Vec<_>> {
                    Bnd4::read(&read(&path)?)?
                        .files
                        .iter()
                        .map(|f| {
                            let name = f.short_name().unwrap_or("").trim_end_matches(".esd");
                            Ok((m, name.to_string(), Esd::read(&f.data)?))
                        })
                        .collect()
                })
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(Sources {
            reg: Regulation::open(game)?,
            maps,
            scripts,
            common_func: Emevd::read(&read("/event/common_func.emevd.dcx")?)?,
            common: Emevd::read(&read("/event/common.emevd.dcx")?)?,
            talk,
            // The game's own names, whatever a mod calls things: the area map knows
            // an item or boss by its name, and a renaming changes neither.
            text: text(&dvd.dvd)?,
        })
    }

    pub fn text(&self, table: &str, id: i32) -> Option<&str> {
        self.text.get(table)?.get(&id).map(String::as_str)
    }
}

/// Every map the archives hold under a `mAA_BB_CC_DD` name, in order.
fn map_ids(dvd: &GameFiles) -> Vec<MapId> {
    named(dvd, |m| format!("/map/mapstudio/{m}.msb.dcx"))
}

/// Every map id for which the archives hold the file `path` names.
fn named(dvd: &GameFiles, path: impl Fn(MapId) -> String) -> Vec<MapId> {
    let mut out = Vec::new();
    for a in 10..100 {
        for b in 0..100 {
            for c in 0..100 {
                for d in 0..4 {
                    let m = MapId([a, b, c, d]);
                    if dvd.exists(&path(m)) {
                        out.push(m);
                    }
                }
            }
        }
    }
    out
}

/// Each name table, merged across `X`, `X_dlc01` and `X_dlc02`.
pub fn text(dvd: &Dvd) -> Result<Text> {
    text_of(&dvd.read_path(ITEM_TEXT)?)
}

/// [`text`], from the item text file's bytes.
fn text_of(file: &[u8]) -> Result<Text> {
    let (_, plain) = fs_dcx::decompress(file)?;
    let bnd = Bnd4::read(&plain)?;
    let mut out = Text::new();
    for f in &bnd.files {
        let Some(stem) = f.short_name().and_then(|s| s.strip_suffix(".fmg")) else {
            continue;
        };
        let base = stem.trim_end_matches("_dlc01").trim_end_matches("_dlc02");
        let table = out.entry(base.to_string()).or_default();
        for (id, text) in Fmg::read(&f.data)?.entries {
            if let Some(t) = text.filter(|t| !t.is_empty()) {
                table.insert(id, t);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_id_is_found_in_a_path_and_printed_back() {
        let m = MapId::find_in("/map/mapstudio/m60_42_36_00.msb.dcx").unwrap();
        assert_eq!(m, MapId([60, 42, 36, 0]));
        assert_eq!(m.to_string(), "m60_42_36_00");
        assert_eq!(MapId::find_in("/event/common_func.emevd.dcx"), None);
        assert_eq!(MapId::find_in("m1_00_00_00"), None);
    }

    #[test]
    fn open_world_positions_are_placed_on_the_whole_world() {
        // The centre of small tile 42_36 and of the large tile holding it.
        assert_eq!(
            MapId([60, 42, 36, 0]).world([0.0, 5.0, 0.0]),
            Some([10_880.0, 9_344.0])
        );
        assert_eq!(
            MapId([60, 10, 9, 2]).world([-512.0, 0.0, -512.0]),
            Some([10_240.0, 9_216.0])
        );
        assert_eq!(MapId([10, 0, 0, 0]).world([0.0; 3]), None);
    }
}

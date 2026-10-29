//! Names from the game's own English text, for reading tables by eye: an item
//! id becomes "Rowa Raisin" rather than 20651.

use std::collections::HashMap;

use fs_bnd4::Bnd4;
use fs_bytes::Result;
use fs_dvdbnd::Dvd;
use fs_fmg::Fmg;

/// The text binder that carries the base game's names and both DLC sets.
const ITEM_TEXT: &str = "/msg/engus/item_dlc02.msgbnd.dcx";

/// Each name table (e.g. `GoodsName`), merged across `X`, `X_dlc01` and
/// `X_dlc02`.
pub fn load(dvd: &Dvd) -> Result<HashMap<String, HashMap<i32, String>>> {
    let (_, plain) = fs_dcx::decompress(&dvd.read_path(ITEM_TEXT)?)?;
    let bnd = Bnd4::read(&plain)?;
    let mut out: HashMap<String, HashMap<i32, String>> = HashMap::new();
    for f in &bnd.files {
        let Some(short) = f.short_name() else {
            continue;
        };
        let Some(stem) = short.strip_suffix(".fmg") else {
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

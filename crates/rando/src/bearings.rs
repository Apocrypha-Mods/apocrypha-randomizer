//! Bell bearings given in talk (Nova, 2026-11-03). Story merchants can leave or
//! die in their quests, and what the seed put in their shop would go with them.
//! Each merchant the area map lists (`[[bearing]]`) gets a talk menu entry, "Get
//! bell-bearing", that gives their own bearing once. Handed to the Twin Maiden
//! Husks, the bearing opens the very ShopLineupParam rows the merchant's scripts
//! open (`gamedata::model::Bearing`), so whatever the seed put in a row is sold
//! there too, at the same price, in the same currency and quantity, out of the
//! same stock.
//!
//! What is written:
//! - an ItemLotParam_map row per merchant, numbered ten times its `given` flag so
//!   no lot runs on into the next (`lot_for`), giving
//!   the bearing and turning that flag on; laid out as the game's own bearing
//!   lots are (Sellen's 111000, Thops's 113600: one goods, weight 1000, a flag);
//! - in each of the merchant's talk scripts, in every menu that opens one of the
//!   bearing's ranges, the entry ([`fs_esd::menu::add_gift`]), offered while the
//!   `given` flag and the Husks' flag for the bearing are both off;
//! - the entry's words, [`words`], as EventTextForTalk [`TEXT`] in every
//!   language's `menu_dlc02` (written with the keepsakes' text).

use std::collections::BTreeMap;

use fs_bytes::{Result, format_err};
use fs_esd::menu::{Gift, add_gift, shop_menus};
use fs_param::Regulation;
use fs_param::layout::for_table;
use fs_param::param::Row;
use gamedata::model::{Bearing, Model};
use gamedata::{GameFiles, Sources};
use logic::{GivenBearing, Graph};

/// The talk text the new entry shows, an EventTextForTalk id no language uses.
pub const TEXT: i32 = 26_009_820;
/// Its words in English, as Nova put them.
pub const WORDS: &str = "Get bell-bearing";

/// The entry's words in a language. Each is made of the game's own words in that
/// language: the verb of Seluvis's "Obtain a puppet" (EventTextForTalk 23070007)
/// with the bell bearing of the Husks' "Offer a bell bearing" (26000000), in the
/// latter's case and article. Arabic and Thai keep the game's glyph forms.
pub fn words(language: &str) -> &'static str {
    match language {
        "araae" => {
            "\u{627}\u{fed7}\u{fe97}\u{fee7}\u{fe8e}\u{621} \u{fedb}\u{fead}\u{629} \u{fe9f}\u{fead}\u{633}"
        }
        "deude" => "Eine Klangperle erhalten",
        "frafr" => "Obtenir une perle cinéraire",
        "itait" => "Ricevi un globo cinereo",
        "jpnjp" => "鈴玉を手に入れる",
        "korkr" => "방울을 얻는다",
        "polpl" => "Otrzymaj dzwonek kulisty",
        "porbr" => "Obter um sino",
        "rusru" => "Получить колокольную сферу",
        "spaar" => "Obtener un rodamiento de campana",
        "spaes" => "Obtener una esfera",
        "thath" => {
            "\u{e23}\u{e31}\u{e1a}\u{200b}\u{e01}\u{e23}\u{e30}\u{e14}\u{e34}\u{f716}\u{e07}"
        }
        "zhocn" => "取得铃珠",
        "zhotw" => "取得鈴珠",
        _ => WORDS,
    }
}

const LOTS: &str = "ItemLotParam_map";
/// ItemLotParam's item category for goods.
const GOODS: i64 = 1;
/// A sure thing, out of 1000.
const ALWAYS: i64 = 1000;

/// A merchant given a bearing, with what the game says the bearing is.
pub struct Given<'a> {
    pub entry: &'a GivenBearing,
    pub bearing: &'a Bearing,
}

/// The merchants the area map gives a bearing, each with the model's bearing.
pub fn given<'a>(graph: &'a Graph, model: &'a Model) -> Result<Vec<Given<'a>>> {
    graph
        .bearings
        .iter()
        .map(|entry| {
            let bearing = model
                .bearings
                .iter()
                .find(|b| b.name.as_deref() == Some(&entry.item))
                .ok_or_else(|| {
                    format_err(format!("bearing {}: no talk script takes it", entry.item))
                })?;
            Ok(Given { entry, bearing })
        })
        .collect()
}

/// What giving the bearings changed.
#[derive(Default)]
pub struct Applied {
    /// Talk binders, by path from the game's root, and their scripts changed.
    pub binders: BTreeMap<String, BTreeMap<String, fs_esd::Esd>>,
    /// For the spoiler log: each merchant, and where the entry was added.
    pub log: Vec<String>,
}

/// Add each merchant's lot to `reg` and their entry to their talk scripts.
pub fn apply(
    sources: &Sources,
    graph: &Graph,
    model: &Model,
    reg: &mut Regulation,
) -> Result<Applied> {
    let given = given(graph, model)?;
    if given.is_empty() {
        return Ok(Applied::default());
    }
    add_lots(reg, &given)?;
    let mut out = Applied::default();
    for g in &given {
        let gift = Gift {
            text: TEXT,
            lot: lot_id(g.entry)?,
            flag: g.entry.given,
            hidden_by: vec![g.bearing.flag],
        };
        let mut places = Vec::new();
        for (map, name, esd) in &sources.talk {
            // Not the script that takes the bearing.
            if *name == g.bearing.script {
                continue;
            }
            let path = format!("/script/talk/{map}.talkesdbnd.dcx");
            let groups: Vec<i64> = shop_menus(esd)
                .iter()
                .filter(|m| m.ranges.iter().any(|r| g.bearing.ranges.contains(r)))
                .map(|m| esd.groups[m.group].id)
                .collect();
            if groups.is_empty() {
                continue;
            }
            let script = out
                .binders
                .entry(path)
                .or_default()
                .entry(name.clone())
                .or_insert_with(|| esd.clone());
            for id in &groups {
                let group = script
                    .groups
                    .iter()
                    .position(|x| x.id == *id)
                    .ok_or_else(|| format_err("a machine went missing"))?;
                add_gift(script, group, &gift)
                    .map_err(|e| format_err(format!("{name} machine {id:#x}: {e}")))?;
            }
            places.push(match groups.len() {
                1 => name.clone(),
                n => format!("{name} ({n} menus)"),
            });
        }
        if places.is_empty() {
            return Err(format_err(format!(
                "bearing {}: none of {}'s menus opens its rows",
                g.entry.item, g.entry.merchant
            )));
        }
        out.log.push(format!(
            "{}: {} (flag {}), in {}",
            g.entry.merchant,
            g.entry.item,
            g.entry.given,
            places.join(", ")
        ));
    }
    Ok(out)
}

fn lot_id(entry: &GivenBearing) -> Result<i32> {
    lot_for(entry.given)
}

/// The lot giving the bearing whose `given` flag is `flag`: ten times the flag.
/// The game runs a lot on into the rows numbered after it, so lots numbered as
/// the flags, one after another, gave every later merchant's bearing too (Kalé's
/// gave Gostoc's and Pidia's, seen in play 2026-11-05). Spaced by ten, none runs on.
pub fn lot_for(flag: u32) -> Result<i32> {
    i64::from(flag)
        .checked_mul(10)
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| format_err("a bearing's lot past i32"))
}

/// One lot per merchant, numbered by `lot_for`, laid out as the game's own lot
/// for a bearing of the model is.
fn add_lots(reg: &mut Regulation, given: &[Given]) -> Result<()> {
    let layout = for_table(LOTS).ok_or_else(|| format_err("no item lot layout"))?;
    let (item, category, weight, flag, count) = (
        layout.field("item_id"),
        layout.field("item_category"),
        layout.field("weight"),
        layout.field("get_item_flag"),
        layout.field("count"),
    );
    let mut table = reg.table(LOTS)?;
    let alone = |data: &[u8]| {
        category.get(data, 0) == GOODS
            && weight.get(data, 0) == ALWAYS
            && (1..item.count).all(|s| item.get(data, s) == 0 && category.get(data, s) == 0)
    };
    for g in given {
        let id = lot_id(g.entry)?;
        // The row after it must be free too, or the game would give it as well.
        if let Some(r) = table.rows.iter().find(|r| r.id == id || r.id == id + 1) {
            return Err(format_err(format!("{LOTS} already has a row {}", r.id)));
        }
        let own = |r: &&Row| alone(&r.data) && item.get(&r.data, 0) == i64::from(g.bearing.goods);
        let template = table
            .rows
            .iter()
            .find(own)
            .or_else(|| {
                table.rows.iter().find(|r| {
                    alone(&r.data)
                        && given
                            .iter()
                            .any(|o| item.get(&r.data, 0) == i64::from(o.bearing.goods))
                })
            })
            .ok_or_else(|| format_err("no lot gives a bell bearing to copy"))?;
        let mut data = template.data.clone();
        item.set(&mut data, 0, i64::from(g.bearing.goods))?;
        count.set(&mut data, 0, 1)?;
        flag.set(&mut data, 0, i64::from(g.entry.given))?;
        table.rows.push(Row {
            id,
            data,
            name: template.name.as_ref().map(|_| Vec::new()),
        });
    }
    table.rows.sort_by_key(|r| r.id);
    reg.set_table(LOTS, &table)
}

/// Write the changed scripts back into their binders, for the mod folder.
pub fn binders(files: &GameFiles, applied: &Applied) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for (path, scripts) in &applied.binders {
        let (header, plain) = fs_dcx::decompress(&files.read(path)?)?;
        let mut bnd = fs_bnd4::Bnd4::read(&plain)?;
        let mut found = 0;
        for f in &mut bnd.files {
            let name = f.short_name().unwrap_or("").trim_end_matches(".esd");
            if let Some(esd) = scripts.get(name) {
                f.data = esd.write()?;
                found += 1;
            }
        }
        if found != scripts.len() {
            return Err(format_err(format!("{path}: a changed script is not there")));
        }
        let data = fs_dcx::compress(
            &header.with_algorithm(fs_dcx::Algorithm::Zstd),
            &bnd.write()?,
        )?;
        out.push((path.trim_start_matches('/').to_string(), data));
    }
    Ok(out)
}

#[cfg(test)]
mod words_tests {
    use super::*;

    #[test]
    fn every_language_has_words_of_its_own() {
        for language in crate::LANGUAGES {
            let w = words(language);
            assert!(!w.is_empty());
            assert_eq!(w == WORDS, language == "engus", "{language}");
        }
    }
}

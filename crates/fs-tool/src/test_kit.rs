//! A kit for testing upgrades by hand: on entering the Chapel of Anticipation
//! the player is given 50 of every smithing stone and of Numen's Rune, for
//! the runes upgrades cost. Written over a mod folder `apoc-rando gen` built.
//! Never for a seed that is meant to be played as itself.

use std::path::PathBuf;

use fs_bytes::{Result, format_err};
use fs_dcx::Algorithm;
use fs_dvdbnd::Dvd;
use fs_emevd::{Emevd, Instruction};
use fs_param::Regulation;

const SCRIPT: &str = "/event/m10_01_00_00.emevd.dcx";
/// The smithing stones, regular then somber, and Numen's Rune.
const GOODS: [i64; 20] = [
    10100, 10101, 10102, 10103, 10104, 10105, 10106, 10107, 10140, 10160, 10161, 10162, 10163,
    10164, 10165, 10166, 10167, 10200, 10168, 2913,
];
const COUNT: i64 = 50;
/// Old test rows nothing gives: `ItemLotParam_map` in the model's gaps, as of
/// `apoc-fs model --dump` on 2.7.2. Rows near them are real awards.
const SPARE_LOTS: [i32; 25] = [
    40500, 40502, 40504, 40506, 40514, 40520, 40528, 40570, 40572, 40574, 40578, 40612, 40614,
    40616, 40618, 40632, 40634, 40636, 40638, 40640, 40644, 40650, 40662, 40664, 40666,
];

fn instruction(bank: u32, id: u32, args: Vec<u8>) -> Instruction {
    Instruction {
        bank,
        id,
        args,
        layer: None,
    }
}

pub fn run(game: PathBuf, out: PathBuf) -> Result<()> {
    // The mod's own regulation, opened with the key the game's exe holds.
    let file = std::fs::read(out.join("regulation.bin"))?;
    let exe = std::fs::read(game.join("eldenring.exe"))?;
    let mut reg = Regulation::read(&fs_param::crypto::find_key(&exe, &file)?, &file)?;

    // A lot runs on into the row after it, so only rows with none after them.
    let mut table = reg.table("ItemLotParam_map")?;
    let ids: std::collections::BTreeSet<i32> = table.rows.iter().map(|r| r.id).collect();
    let lots: Vec<i32> = ids
        .iter()
        .copied()
        .filter(|id| SPARE_LOTS.contains(id) && !ids.contains(&(id + 1)))
        .take(GOODS.len())
        .collect();
    if lots.len() < GOODS.len() {
        return Err(format_err(format!(
            "only {} spare lots: {lots:?}",
            lots.len()
        )));
    }
    let layout = fs_param::layout::for_table("ItemLotParam_map")
        .ok_or_else(|| format_err("no ItemLotParam_map layout"))?;
    for (&lot, &goods) in lots.iter().zip(&GOODS) {
        let row = table
            .rows
            .iter_mut()
            .find(|r| r.id == lot)
            .ok_or_else(|| format_err("no such lot"))?;
        for i in 0..8 {
            let first = i == 0;
            layout
                .field("item_id")
                .set(&mut row.data, i, if first { goods } else { 0 })?;
            layout
                .field("item_category")
                .set(&mut row.data, i, i64::from(first))?;
            layout
                .field("weight")
                .set(&mut row.data, i, if first { 1000 } else { 0 })?;
            layout
                .field("count")
                .set(&mut row.data, i, if first { COUNT } else { 0 })?;
        }
        layout.field("get_item_flag").set(&mut row.data, 0, 0)?;
    }
    reg.set_table("ItemLotParam_map", &table)?;
    std::fs::write(out.join("regulation.bin"), reg.write()?)?;
    eprintln!("lots: {lots:?}");

    // The awards, at the end of the event the script starts from: nothing
    // after them waits on their waits.
    let dvd = Dvd::open_elden_ring(&game)?;
    let (header, plain) = fs_dcx::decompress(&dvd.read_path(SCRIPT)?)?;
    let mut script = Emevd::read(&plain)?;
    let event = script
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("the chapel's script has no event 0"))?;
    for lot in lots {
        event
            .instructions
            .push(instruction(1001, 0, 1.0f32.to_le_bytes().to_vec()));
        event
            .instructions
            .push(instruction(2003, 4, lot.to_le_bytes().to_vec()));
    }
    let path = out.join(SCRIPT.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap_or(&out))?;
    std::fs::write(
        path,
        fs_dcx::compress(&header.with_algorithm(Algorithm::Zstd), &script.write()?)?,
    )?;
    eprintln!("{} goods, {COUNT} each, from {SCRIPT}", GOODS.len());
    Ok(())
}

/// Add a warp to the script of `from`, or the chapel's: 2003[14] takes the map's
/// four bytes, then the entity to land on, as the game's own scripts do.
pub fn warp(
    game: PathBuf,
    out: PathBuf,
    map: &str,
    entity: u32,
    from: Option<String>,
) -> Result<()> {
    let script_path = from.map_or(SCRIPT.to_string(), |m| format!("/event/{m}.emevd.dcx"));
    let bytes: Vec<u8> = map
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| format_err(format!("{map} is not four numbers")))?;
    if bytes.len() != 4 {
        return Err(format_err(format!("{map} is not four numbers")));
    }
    let dvd = Dvd::open_elden_ring(&game)?;
    let (header, plain) = fs_dcx::decompress(&dvd.read_path(&script_path)?)?;
    let mut script = Emevd::read(&plain)?;
    let event = script
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("the script has no event 0"))?;
    let mut args = bytes;
    args.extend(entity.to_le_bytes());
    args.extend(0u32.to_le_bytes());
    // Once only: a target near the map that sets it off would load that map again,
    // and go round for ever. Skip the set, the wait and the warp if the flag is on.
    const ONCE: u32 = 10_009_990;
    let mut skip = vec![3u8, 1, 0, 0];
    skip.extend(ONCE.to_le_bytes());
    event.instructions.push(instruction(1003, 1, skip));
    let mut set = vec![0u8; 4];
    set.extend(ONCE.to_le_bytes());
    set.extend(1u32.to_le_bytes());
    event.instructions.push(instruction(2003, 66, set));
    event
        .instructions
        .push(instruction(1001, 0, 3.0f32.to_le_bytes().to_vec()));
    event.instructions.push(instruction(2003, 14, args));
    let path = out.join(script_path.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap_or(&out))?;
    std::fs::write(
        path,
        fs_dcx::compress(&header.with_algorithm(Algorithm::Zstd), &script.write()?)?,
    )?;
    Ok(())
}

/// Leave a mod folder's moved enemies with one health, to see in the lab what
/// their death sets off. Rows from 800,000,000 up are the ones a seed makes.
pub fn weak_enemies(game: PathBuf, out: PathBuf) -> Result<()> {
    const HP: usize = 0x24;
    let file = out.join("regulation.bin");
    let bytes = std::fs::read(&file)?;
    let key = fs_param::crypto::find_key(&std::fs::read(game.join("eldenring.exe"))?, &bytes)?;
    let mut reg = fs_param::Regulation::read(&key, &bytes)?;
    let mut npc = reg.table("NpcParam")?;
    let mut count = 0;
    for row in npc.rows.iter_mut().filter(|r| r.id >= 800_000_000) {
        row.data[HP..HP + 4].copy_from_slice(&1i32.to_le_bytes());
        count += 1;
    }
    reg.set_table("NpcParam", &npc)?;
    // A new file: the folder may share this one with a copy.
    std::fs::remove_file(&file)?;
    std::fs::write(&file, reg.write()?)?;
    eprintln!("{count} rows left with one health");
    Ok(())
}

/// Kill an entity some seconds after its map loads (2004[4]: the entity, then 1 to
/// give runes), to see in the lab what a death sets off. Built on `over`'s copy of
/// the map's script, a mod's, if it has one. With `freeze`, stop its AI instead, so
/// a character in the lab can walk up and hit it.
pub fn kill(
    game: PathBuf,
    out: PathBuf,
    map: &str,
    entity: u32,
    after: f32,
    over: Option<PathBuf>,
    freeze: bool,
) -> Result<()> {
    let script_path = format!("/event/{map}.emevd.dcx");
    let bytes = match over
        .map(|d| d.join(script_path.trim_start_matches('/')))
        .filter(|f| f.is_file())
    {
        Some(file) => std::fs::read(file)?,
        None => Dvd::open_elden_ring(&game)?.read_path(&script_path)?,
    };
    let (header, plain) = fs_dcx::decompress(&bytes)?;
    let mut script = Emevd::read(&plain)?;
    let event = script
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("the script has no event 0"))?;
    if freeze {
        // 2004[1]: the entity, then 0 for its AI off. Again and again, as a fight's
        // own wake turns it back on.
        let mut args = entity.to_le_bytes().to_vec();
        args.extend(0u32.to_le_bytes());
        for _ in 0..120 {
            event
                .instructions
                .push(instruction(1001, 0, 1.0f32.to_le_bytes().to_vec()));
            event.instructions.push(instruction(2004, 1, args.clone()));
        }
    } else {
        let mut args = entity.to_le_bytes().to_vec();
        args.extend(1u32.to_le_bytes());
        event
            .instructions
            .push(instruction(1001, 0, after.to_le_bytes().to_vec()));
        event.instructions.push(instruction(2004, 4, args));
    }
    let path = out.join(script_path.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap_or(&out))?;
    std::fs::write(
        path,
        fs_dcx::compress(&header.with_algorithm(Algorithm::Zstd), &script.write()?)?,
    )?;
    Ok(())
}

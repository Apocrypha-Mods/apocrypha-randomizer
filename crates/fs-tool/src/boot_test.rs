//! The Phase 1 step-3 check: does the game accept files we write? Builds a
//! mod folder for me3 that changes what can be seen early, each file written
//! by our own code and compressed with zstd:
//!
//! - `regulation.bin`: the Vagabond (CharaInitParam 3000) starts with
//!   Moonveil in the right hand, shown at character creation;
//! - `msg/engus/item_dlc02.msgbnd.dcx`: Moonveil is renamed, so the class
//!   screen shows our text;
//! - the maps and event scripts of the Chapel of Anticipation
//!   (`m10_01_00_00`, where a new game starts) and the Stranded Graveyard
//!   (`m18_00_00_00`), parsed and written back unchanged, so the first areas
//!   load from our files.

use std::path::{Path, PathBuf};

use fs_bnd4::Bnd4;
use fs_bytes::{Result, format_err};
use fs_dcx::Algorithm;
use fs_dvdbnd::Dvd;
use fs_fmg::Fmg;
use fs_param::Regulation;

const VAGABOND: i32 = 3000;
const MOONVEIL: i64 = 9_060_000;
pub const NAME: &str = "Moonveil (Apocrypha boot test)";
const ITEM_TEXT: &str = "/msg/engus/item_dlc02.msgbnd.dcx";
const MAPS: [&str; 2] = ["m10_01_00_00", "m18_00_00_00"];

pub fn run(game: PathBuf, out: PathBuf) -> Result<()> {
    let dvd = Dvd::open_elden_ring(&game)?;

    // The Vagabond's right hand.
    let mut reg = Regulation::open(&game)?;
    let mut init = reg.table("CharaInitParam")?;
    let weapon = fs_param::layout::for_table("CharaInitParam")
        .ok_or_else(|| format_err("no CharaInitParam layout"))?
        .field("weapon");
    let row = init
        .rows
        .iter_mut()
        .find(|r| r.id == VAGABOND)
        .ok_or_else(|| format_err("no Vagabond row"))?;
    let was = weapon.get(&row.data, 0);
    weapon.set(&mut row.data, 0, MOONVEIL)?;
    reg.set_table("CharaInitParam", &init)?;
    put(&out, "regulation.bin", &reg.write()?)?;
    eprintln!("Vagabond's right hand: {was} -> {MOONVEIL}");

    // Moonveil's name, in every weapon-name table that lists it.
    let (header, plain) = fs_dcx::decompress(&dvd.read_path(ITEM_TEXT)?)?;
    let mut bnd = Bnd4::read(&plain)?;
    let mut renamed = 0;
    for f in &mut bnd.files {
        if !f.short_name().is_some_and(|n| n.starts_with("WeaponName")) {
            continue;
        }
        let mut fmg = Fmg::read(&f.data)?;
        if let Some(text) = fmg.entries.get_mut(&(MOONVEIL as i32))
            && text.is_some()
        {
            *text = Some(NAME.into());
            f.data = fmg.write()?;
            renamed += 1;
        }
    }
    if renamed == 0 {
        return Err(format_err("Moonveil is in no weapon-name table"));
    }
    let item_text = fs_dcx::compress(&header.with_algorithm(Algorithm::Zstd), &bnd.write()?)?;
    put(&out, ITEM_TEXT, &item_text)?;
    eprintln!("Moonveil renamed in {renamed} table(s)");

    // The first areas' maps and event scripts, through our parsers.
    let files = MAPS.iter().flat_map(|m| {
        [
            (
                format!("/map/mapstudio/{m}.msb.dcx"),
                rewrite_msb as fn(&[u8]) -> Result<Vec<u8>>,
            ),
            (format!("/event/{m}.emevd.dcx"), rewrite_emevd),
        ]
    });
    for (path, rewrite) in files {
        let path = path.as_str();
        let (header, plain) = fs_dcx::decompress(&dvd.read_path(path)?)?;
        let again = rewrite(&plain)?;
        if again != plain {
            return Err(format_err(format!(
                "{path} does not come back byte for byte"
            )));
        }
        put(
            &out,
            path,
            &fs_dcx::compress(&header.with_algorithm(Algorithm::Zstd), &again)?,
        )?;
        eprintln!("{path}: rewritten, zstd");
    }
    eprintln!("mod folder: {}", out.display());
    Ok(())
}

fn rewrite_msb(b: &[u8]) -> Result<Vec<u8>> {
    fs_msb::Msb::read(b)?.write()
}

fn rewrite_emevd(b: &[u8]) -> Result<Vec<u8>> {
    fs_emevd::Emevd::read(b)?.write()
}

/// Write `data` at a game path under `out`.
fn put(out: &Path, path: &str, data: &[u8]) -> Result<()> {
    let to = out.join(path.trim_start_matches('/'));
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&to, data)?;
    Ok(())
}

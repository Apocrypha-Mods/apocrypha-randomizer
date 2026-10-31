//! Building a seed on other mods, on the player's own game (`APOC_ER_STOCK`;
//! skipped when unset). The mods are made here from the game's own file.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use fs_param::Regulation;
use gamedata::{Sources, elden_ring};

fn stock() -> Option<PathBuf> {
    let dir = std::env::var_os("APOC_ER_STOCK").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("APOC_ER_STOCK is not set; skipping");
    }
    dir
}

fn work(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A mod: the game's file with one table's rows changed by `edit`.
fn make_mod(game: &Path, file: &Path, table: &str, edit: impl Fn(&mut fs_param::Row)) {
    let mut reg = Regulation::open(game).unwrap();
    let mut param = reg.table(table).unwrap();
    param.rows.iter_mut().for_each(edit);
    reg.set_table(table, &param).unwrap();
    std::fs::write(file, reg.write().unwrap()).unwrap();
}

fn apoc_rando(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_apoc-rando"))
        .args(args)
        .output()
        .unwrap()
}

fn generate(game: &Path, out: &Path, mods: &[&Path]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "gen".as_ref(),
        game.as_os_str(),
        "--seed".as_ref(),
        "1".as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ];
    for m in mods {
        args.push("--with-mod".as_ref());
        args.push(m.as_os_str());
    }
    apoc_rando(&args)
}

const WEAPON: i32 = 1_000_000;

/// A word no slot of the randomizer writes, in a table it leaves alone.
fn stamp(row: &mut fs_param::Row, id: i32, value: u8) {
    if row.id == id {
        row.data[0..4].fill(value);
    }
}

#[test]
fn a_seed_keeps_what_each_mod_changed_and_reports_their_disagreements() {
    let Some(game) = stock() else { return };
    let dir = work("merge-keeps");
    let (a, b) = (dir.join("a.bin"), dir.join("b.bin"));
    make_mod(&game, &a, "EquipParamWeapon", |r| {
        stamp(r, WEAPON, 0x11);
        stamp(r, WEAPON + 10_000, 0x22);
    });
    make_mod(&game, &b, "EquipParamWeapon", |r| stamp(r, WEAPON, 0x33));

    let out = dir.join("seed");
    let run = generate(&game, &out, &[&a, &b]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let key = fs_param::crypto::find_key(
        &std::fs::read(game.join("eldenring.exe")).unwrap(),
        &std::fs::read(game.join("regulation.bin")).unwrap(),
    )
    .unwrap();
    let reg = Regulation::read(&key, &std::fs::read(out.join("regulation.bin")).unwrap()).unwrap();
    let weapons = reg.table("EquipParamWeapon").unwrap();
    let word = |id| weapons.rows.iter().find(|r| r.id == id).unwrap().data[0];
    assert_eq!(word(WEAPON), 0x33, "the later mod wins");
    assert_eq!(
        word(WEAPON + 10_000),
        0x22,
        "the earlier mod's other edit stays"
    );

    // The seed still shuffled on top of them.
    let vanilla = Regulation::open(&game).unwrap();
    assert_ne!(
        reg.table("ItemLotParam_map").unwrap().rows,
        vanilla.table("ItemLotParam_map").unwrap().rows
    );

    let log = std::fs::read_to_string(out.with_extension("spoiler.txt")).unwrap();
    assert!(log.starts_with("Built on these mods"), "{}", &log[..200]);
    assert!(
        log.contains(&format!("EquipParamWeapon row {WEAPON}")),
        "no conflict in the log"
    );
}

#[test]
fn the_same_seed_on_the_same_mods_gives_the_same_file() {
    let Some(game) = stock() else { return };
    let dir = work("merge-same");
    let a = dir.join("a.bin");
    make_mod(&game, &a, "EquipParamWeapon", |r| stamp(r, WEAPON, 0x11));
    let (x, y) = (dir.join("x"), dir.join("y"));
    assert!(generate(&game, &x, &[&a]).status.success());
    assert!(generate(&game, &y, &[&a]).status.success());
    assert_eq!(
        std::fs::read(x.join("regulation.bin")).unwrap(),
        std::fs::read(y.join("regulation.bin")).unwrap()
    );
}

/// The seed is built from the merged tables: a mod that takes a key item out of
/// the game gets a refusal naming it, not a seed that cannot be finished.
#[test]
fn a_mod_that_takes_a_needed_key_item_away_is_refused_by_name() {
    let Some(game) = stock() else { return };
    let sources = Sources::open(&game).unwrap();
    let model = elden_ring::extract(&sources).unwrap();
    let graph = logic::elden_ring();
    let bound = logic::Bound::new(&graph, &model);
    let units = items::units(&model, &bound, &sources.reg).unwrap();
    let lots: Vec<i32> = units
        .iter()
        .filter(|u| {
            u.table == "ItemLotParam_map" && u.item.name.as_deref() == Some("Rold Medallion")
        })
        .map(|u| u.row)
        .collect();
    assert!(!lots.is_empty());

    let dir = work("merge-key");
    let a = dir.join("a.bin");
    make_mod(&game, &a, "ItemLotParam_map", |r| {
        if lots.contains(&r.id) {
            r.data.fill(0);
        }
    });
    let run = generate(&game, &dir.join("seed"), &[&a]);
    assert!(
        !run.status.success(),
        "a seed was written without the medallion"
    );
    assert!(String::from_utf8_lossy(&run.stderr).contains("Rold Medallion"));
    assert!(!dir.join("seed/regulation.bin").exists());
}

#[test]
fn a_file_that_is_not_a_regulation_stops_the_run_and_names_the_mod() {
    let Some(game) = stock() else { return };
    let dir = work("merge-bad");
    let bad = dir.join("broken.bin");
    std::fs::write(&bad, vec![0u8; 4096]).unwrap();
    let run = generate(&game, &dir.join("seed"), &[&bad]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("broken.bin"));
    assert!(!dir.join("seed").exists());
}

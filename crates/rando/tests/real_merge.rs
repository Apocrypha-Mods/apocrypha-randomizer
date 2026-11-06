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

/// One text table's entries in a file of the seed's or a mod's.
fn menu_entry(file: &[u8], table: &str, id: i32) -> Option<String> {
    let (_, plain) = fs_dcx::decompress(file).unwrap();
    let bnd = fs_bnd4::Bnd4::read(&plain).unwrap();
    bnd.files
        .iter()
        .filter(|f| f.short_name() == Some(table))
        .find_map(|f| {
            fs_fmg::Fmg::read(&f.data)
                .unwrap()
                .entries
                .get(&id)
                .cloned()
                .flatten()
        })
}

/// Files the seed rewrites are built on the mod's copy: its own text stays, the
/// seed's keepsake names go in, and a profile loads the seed after the mod. A mod
/// may also already use the rows enemy moves make for themselves.
#[test]
fn a_seed_builds_on_a_mods_own_files_and_rows() {
    const MENU: &str = "/msg/engus/menu_dlc02.msgbnd.dcx";
    let Some(game) = stock() else { return };
    let dir = work("merge-files");
    let m = dir.join("Mod");

    // The mod's menu text, with a line of its own.
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(&game).unwrap();
    let (header, plain) = fs_dcx::decompress(&dvd.read_path(MENU).unwrap()).unwrap();
    let mut bnd = fs_bnd4::Bnd4::read(&plain).unwrap();
    let file = bnd
        .files
        .iter_mut()
        .find(|f| f.short_name() == Some("GR_MenuText.fmg"))
        .unwrap();
    let mut fmg = fs_fmg::Fmg::read(&file.data).unwrap();
    fmg.entries.insert(999_999_001, Some("From the mod".into()));
    file.data = fmg.write().unwrap();
    let packed = fs_dcx::compress(
        &header.with_algorithm(fs_dcx::Algorithm::Zstd),
        &bnd.write().unwrap(),
    )
    .unwrap();
    // Mods name folders in any case.
    let at = m.join("MSG/engus/menu_dlc02.msgbnd.dcx");
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, packed).unwrap();

    // Its regulation, with a row where enemy moves begin their own.
    let mut reg = Regulation::open(&game).unwrap();
    let mut npc = reg.table("NpcParam").unwrap();
    let mut extra = npc.rows[0].clone();
    extra.id = 800_000_000;
    npc.rows.push(extra);
    reg.set_table("NpcParam", &npc).unwrap();
    std::fs::write(m.join("regulation.bin"), reg.write().unwrap()).unwrap();

    let out = dir.join("seed");
    let run = apoc_rando(&[
        "gen".as_ref(),
        game.as_os_str(),
        "--seed".as_ref(),
        "1".as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
        "--enemies".as_ref(),
        "bosses=100".as_ref(),
        "--with-mod".as_ref(),
        m.as_os_str(),
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let menu = std::fs::read(out.join(MENU.trim_start_matches('/'))).unwrap();
    assert_eq!(
        menu_entry(&menu, "GR_MenuText.fmg", 999_999_001).as_deref(),
        Some("From the mod")
    );
    let stock_name = menu_entry(&dvd.read_path(MENU).unwrap(), "GR_MenuText.fmg", 297_151);
    assert_ne!(
        menu_entry(&menu, "GR_MenuText.fmg", 297_151),
        stock_name,
        "the keepsake's name is the seed's"
    );

    let profile: toml::Table = std::fs::read_to_string(out.with_extension("me3"))
        .unwrap()
        .parse()
        .unwrap();
    let packages = profile["packages"].as_array().unwrap();
    assert_eq!(packages.last().unwrap()["path"].as_str(), Some("seed"));
    assert_eq!(packages.len(), 2);
}

/// Keepsakes are named in each language from that language's own item text.
#[test]
fn keepsake_names_come_in_every_language() {
    let Some(game) = stock() else { return };
    let dir = work("languages");
    let out = dir.join("seed");
    let run = generate(&game, &out, &[]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let menu = |language: &str| {
        std::fs::read(out.join(format!("msg/{language}/menu_dlc02.msgbnd.dcx"))).unwrap()
    };
    let name = |language: &str| menu_entry(&menu(language), "GR_MenuText.fmg", 297_151).unwrap();
    let (english, german, japanese) = (name("engus"), name("deude"), name("jpnjp"));
    assert_ne!(english, german);
    assert_ne!(english, japanese);
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(&game).unwrap();
    assert_ne!(
        menu_entry(
            &dvd.read_path("/msg/deude/menu_dlc02.msgbnd.dcx").unwrap(),
            "GR_MenuText.fmg",
            297_151
        ),
        Some(german),
        "the German keepsake is the seed's"
    );
}

/// What the last seed wrote and this one did not leaves the folder, what is not
/// ours stays, and every seed gets a profile with a save of its own. A hidden seed
/// takes away the spoiler log the last one left.
#[test]
fn a_reused_folder_keeps_nothing_of_the_last_seed() {
    let Some(game) = stock() else { return };
    let out = work("reused").join("seed");
    let build = |foes: &str, hidden: bool| {
        let mut enemies = rando::EnemyOptions::default();
        enemies.set(foes).unwrap();
        rando::generate(
            &rando::Request {
                game: game.clone(),
                seed: if hidden { rando::hidden_seed() } else { 7 },
                out: out.clone(),
                options: rando::ItemOptions::default(),
                enemies,
                native: None,
                mods: vec![],
                hidden,
            },
            &|_| {},
        )
        .unwrap()
    };
    let first = build("bosses=100", false);
    let spoiler = out.with_extension("spoiler.txt");
    assert_eq!(first.spoiler.as_deref(), Some(spoiler.as_path()));
    assert!(spoiler.is_file());
    let stormveil = out.join("map/mapstudio/m10_00_00_00.msb.dcx");
    assert!(stormveil.is_file());
    std::fs::write(out.join("mine.txt"), "not the seed's").unwrap();
    let done = build("", true);
    assert!(!stormveil.exists());
    assert!(!out.join("map").exists(), "an empty folder stayed");
    assert!(out.join("regulation.bin").is_file());
    assert!(out.join("mine.txt").is_file());
    assert_eq!(done.spoiler, None);
    assert!(!spoiler.exists(), "the last seed's spoiler log stayed");
    let profile = std::fs::read_to_string(done.profile.unwrap()).unwrap();
    assert!(profile.contains("savefile = \"seed.sl2\""), "{profile}");
}

#[test]
fn hidden_seeds_differ_and_fit_the_page() {
    let seeds: std::collections::BTreeSet<u64> = (0..50).map(|_| rando::hidden_seed()).collect();
    assert!(seeds.len() > 45);
    assert!(seeds.iter().all(|&s| s < 1_000_000_000_000_000));
}

/// With keepsakes at 0 nothing of theirs moves, so their names and help line stay
/// the game's in every language; the menu file is still written for the bell
/// bearings' talk entry, and only that changes (Nova, 2026-11-03).
#[test]
fn keepsakes_held_still_keep_the_games_text() {
    let Some(game) = stock() else { return };
    let out = work("keepsakes-still").join("seed");
    let run = apoc_rando(&[
        "gen".as_ref(),
        game.as_os_str(),
        "--seed".as_ref(),
        "1".as_ref(),
        "--shuffle".as_ref(),
        "keepsakes=0".as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(&game).unwrap();
    let ids = [
        297_151, 297_152, 297_153, 297_154, 297_156, 297_157, 297_158, 297_159, 297_160,
    ];
    for language in rando::LANGUAGES {
        let path = format!("msg/{language}/menu_dlc02.msgbnd.dcx");
        let Ok(stock) = dvd.read_path(&format!("/{path}")) else {
            continue;
        };
        let Ok(ours) = std::fs::read(out.join(&path)) else {
            continue;
        };
        for table in ["GR_MenuText.fmg", "GR_LineHelp.fmg"] {
            for id in ids {
                assert_eq!(
                    menu_entry(&ours, table, id),
                    menu_entry(&stock, table, id),
                    "{language} {table} {id}"
                );
            }
        }
    }
    // The talk entry is there, so the file was written for it.
    let english = std::fs::read(out.join("msg/engus/menu_dlc02.msgbnd.dcx")).unwrap();
    assert!(
        menu_entry(&english, "EventTextForTalk.fmg", rando::bearings::TEXT).is_some(),
        "the bell bearings' entry is missing"
    );
}

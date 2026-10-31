//! `apoc-rando`: build a randomized mod folder from a seed, or prove that
//! many seeds can each be finished (the Phase 4 gate).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use clap::{Parser, Subcommand};
use fs_bytes::{Result, format_err};
use fs_param::merge::{self, Contribution};
use gamedata::{Sources, elden_ring};
use rayon::prelude::*;

/// The `regulation.bin` version stamps the area map and the gate have been run
/// against. Anything else may have moved what the logic relies on.
const KNOWN_REGULATIONS: [&str; 2] = ["11711000", "11730034"];

/// Refuse a game patch that has not been checked.
fn known_patch(version: &str) -> Result<()> {
    if KNOWN_REGULATIONS.contains(&version) {
        Ok(())
    } else {
        Err(format_err(format!(
            "this game's regulation.bin is version {version}, and this randomizer has only \
             been checked on {}; it needs updating for this patch",
            KNOWN_REGULATIONS.join(" and ")
        )))
    }
}

#[derive(Parser)]
#[command(name = "apoc-rando", about = "Elden Ring item randomizer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Randomize items from a seed: write regulation.bin into a mod folder
    /// for me3, and the spoiler log beside it.
    Gen {
        /// The game folder (holding eldenring.exe and regulation.bin).
        game: PathBuf,
        #[arg(long)]
        seed: u64,
        /// The mod folder to write.
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        sliders: Sliders,
    },
    /// Place items for many seeds and prove each can be finished.
    Check {
        game: PathBuf,
        /// How many seeds, from `--from` on.
        #[arg(long, default_value_t = 10_000)]
        seeds: u64,
        #[arg(long, default_value_t = 0)]
        from: u64,
        #[command(flatten)]
        sliders: Sliders,
    },
}

/// The player's sliders, per category of item (docs/spec/items.md). A
/// category is one of keys, weapons, armour, talismans, spells,
/// ashes-of-war, spirit-ashes, upgrades, quest, consumables, or all.
#[derive(clap::Args)]
struct Sliders {
    /// How much of a category moves, 0 to 100 (default 100):
    /// `--shuffle all=50,keys=100`.
    #[arg(long, default_value = "")]
    shuffle: String,
    /// How strongly moved items lean towards deep slots, 0 to 100
    /// (default 0): `--bias keys=60,weapons=30`.
    #[arg(long, default_value = "")]
    bias: String,
    /// Keep base-game and DLC items on their own side (`separate`) or
    /// shuffle them together (`mixed`, the default).
    #[arg(long, default_value = "mixed", value_parser = ["mixed", "separate"])]
    dlc: String,
    /// Weapons picked up or bought for runes arrive at the highest level the
    /// player has raised one of their path to. Puts our native module in the
    /// mod folder.
    #[arg(long)]
    auto_upgrade: bool,
    /// The native module for `--auto-upgrade` (default: `apoc_native.dll`
    /// beside this program).
    #[arg(long)]
    native: Option<PathBuf>,
    /// Move enemies, as percentages: `--enemies bosses=100,ordinary=100,mix=0`.
    /// Bosses trade with bosses, ordinary enemies with those of about their
    /// size, and `mix` is the share of bosses that trade with an ordinary enemy.
    /// Nothing moves unless this is given: it has not yet been seen in the game.
    /// `models=N` limits the models ordinary enemies add to a map (default 20,
    /// 0 for none).
    #[arg(long, default_value = "")]
    enemies: String,
    /// Leave moved enemies at their own strength, not that of where they land.
    #[arg(long)]
    no_enemy_scaling: bool,
    /// Another mod's regulation.bin, or a folder holding one, to build the seed
    /// on. Repeat it for more, lowest priority first: where two mods set one
    /// value, the later one wins and the spoiler log says so. The seed is
    /// randomized and proven on the merged tables. Only regulation.bin is
    /// merged; a mod's other files are not read.
    #[arg(long = "with-mod", value_name = "PATH")]
    with_mods: Vec<PathBuf>,
}

impl Sliders {
    fn options(&self) -> Result<items::Options> {
        let mut o = items::Options::default();
        o.set(false, &self.shuffle).map_err(format_err)?;
        o.set(true, &self.bias).map_err(format_err)?;
        o.dlc_separate = self.dlc == "separate";
        o.auto_upgrade = self.auto_upgrade;
        Ok(o)
    }

    fn enemy_options(&self) -> Result<enemies::Options> {
        let mut o = enemies::Options::default();
        o.set(&self.enemies).map_err(format_err)?;
        o.scale = !self.no_enemy_scaling;
        o.dlc_separate = self.dlc == "separate";
        Ok(o)
    }
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Gen {
            game,
            seed,
            out,
            sliders,
        } => sliders.options().and_then(|o| {
            let foes = sliders.enemy_options()?;
            gen_seed(
                game,
                seed,
                out,
                &o,
                &foes,
                sliders.native.as_deref(),
                &sliders.with_mods,
            )
        }),
        Command::Check {
            game,
            seeds,
            from,
            sliders,
        } => sliders
            .options()
            .and_then(|o| check(game, from, seeds, &o, &sliders.with_mods)),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Where a mod keeps its `regulation.bin`: the path itself, in the folder, or
/// (as the mod manager stores them) under `root-files`.
fn mod_regulation(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    for inside in ["regulation.bin", "root-files/regulation.bin"] {
        let file = path.join(inside);
        if file.is_file() {
            return Ok(file);
        }
    }
    Err(format_err(format!(
        "--with-mod {}: there is no regulation.bin there",
        path.display()
    )))
}

/// The folder a mod lives in: the mod manager keeps files for the game's folder
/// in `root-files`, beside `main`.
fn mod_root(path: &Path) -> Option<&Path> {
    let dir = if path.is_file() { path.parent()? } else { path };
    match dir.file_name() {
        Some(n) if n == "root-files" => dir.parent(),
        _ if path.is_file() && path.file_name().is_some_and(|n| n != "regulation.bin") => None,
        _ => Some(dir),
    }
}

/// A mod's name in reports: its folder, or its file if that says more.
fn mod_label(path: &Path) -> String {
    let named = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let is_regulation = path.file_name().is_some_and(|n| n == "regulation.bin");
    let dir = if is_regulation {
        path.parent()
    } else {
        Some(path)
    };
    let dir = match dir {
        Some(d) if d.file_name().is_some_and(|n| n == "root-files") => d.parent(),
        d => d,
    };
    dir.and_then(named)
        .unwrap_or_else(|| path.display().to_string())
}

/// Merge the other mods onto the game's own file and put the result in
/// `sources`, so everything after this reads the tables that will ship. A mod
/// that cannot be merged stops the run, as the player asked for it by name.
/// Returns the report for the spoiler log, empty with no mods.
fn merge_mods(game: &Path, sources: &mut Sources, mods: &[PathBuf]) -> Result<String> {
    if mods.is_empty() {
        return Ok(String::new());
    }
    let base = std::fs::read(game.join("regulation.bin"))?;
    let key = fs_param::crypto::find_key(&std::fs::read(game.join("eldenring.exe"))?, &base)?;
    let contributions = mods
        .iter()
        .map(|m| {
            Ok(Contribution {
                label: mod_label(m),
                bytes: std::fs::read(mod_regulation(m)?)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let outcome = merge::merge(&key, &base, &contributions)?;
    let report = outcome.report;
    if !report.refused.is_empty() {
        let why: Vec<String> = report
            .refused
            .iter()
            .map(|r| format!("{}: {}", r.label, r.reason))
            .collect();
        return Err(format_err(format!(
            "these mods cannot be merged:\n  {}",
            why.join("\n  ")
        )));
    }
    let bytes = outcome
        .bytes
        .ok_or_else(|| format_err("the merge produced nothing"))?;
    sources.reg = fs_param::Regulation::read(&key, &bytes)?;
    let log = merge_log(&report);
    eprint!("{log}");
    Ok(log)
}

fn merge_log(report: &merge::MergeReport) -> String {
    let mut log = format!(
        "Built on these mods, lowest priority first (game {})\n",
        report.game_version
    );
    for m in &report.merged {
        log += &format!(
            "  {}, made for {}: {} rows changed, {} added, {} removed\n",
            m.label, m.version, m.rows_changed, m.rows_added, m.rows_removed
        );
    }
    for w in &report.warnings {
        log += &format!("  warning: {w}\n");
    }
    for c in &report.conflicts {
        log += &format!(
            "  conflict: {} row {} ({:?}): {} over {}\n",
            c.param,
            c.row_id,
            c.kind,
            c.winner,
            c.overridden.join(", ")
        );
    }
    log += "  The proof covers these tables. It does not see a mod's maps, scripts or text.\n";
    log
}

/// Files of a mod's folder, other than `regulation.bin`, that the seed also
/// writes: only one of each can load. A mod manager's `main` and `root-files`
/// stand for the game's folder.
fn overlaps(mod_dir: &Path, out: &Path) -> Vec<String> {
    fn walk(dir: &Path, rel: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let rel = rel.join(e.file_name());
            if e.path().is_dir() {
                walk(&e.path(), &rel, found);
            } else {
                found.push(rel);
            }
        }
    }
    let mut files = Vec::new();
    walk(mod_dir, Path::new(""), &mut files);
    let mut same: Vec<String> = files
        .iter()
        .map(|f| {
            f.strip_prefix("main")
                .or_else(|_| f.strip_prefix("root-files"))
                .unwrap_or(f)
        })
        .filter(|f| *f != Path::new("regulation.bin") && out.join(f).is_file())
        .map(|f| f.display().to_string())
        .collect();
    same.sort();
    same
}

fn gen_seed(
    game: PathBuf,
    seed: u64,
    out: PathBuf,
    options: &items::Options,
    foes: &enemies::Options,
    native: Option<&std::path::Path>,
    mods: &[PathBuf],
) -> Result<()> {
    let mut sources = Sources::open(&game)?;
    known_patch(&sources.reg.version())?;
    let merged = merge_mods(&game, &mut sources, mods)?;
    let model = elden_ring::extract(&sources)?;
    let graph = logic::elden_ring();
    let bound = logic::Bound::new(&graph, &model);
    if !bound.problems.is_empty() {
        return Err(format_err(format!(
            "the area map does not fit this game: {:?}",
            bound.problems
        )));
    }
    let units = items::units(&model, &bound, &sources.reg)?;
    let plan = items::shuffle(&bound, &units, seed, options).map_err(format_err)?;
    let mut reg = sources.reg.clone();
    items::apply(&mut reg, &units, &plan)?;
    let dressed = items::dress_npcs(&mut reg, &model, seed, options)?;
    std::fs::create_dir_all(&out)?;
    keepsake_text(&game, &out, &items::keepsake_names(&units, &plan))?;
    if options.auto_upgrade {
        native_module(&out, native)?;
    }
    let enemy_log = if foes.is_off() {
        None
    } else {
        Some(move_enemies(
            &game, &out, &sources, &model, &mut reg, seed, foes,
        )?)
    };
    std::fs::write(out.join("regulation.bin"), reg.write()?)?;
    let spoiler = out.with_extension("spoiler.txt");
    let mut log = merged;
    log += &items::spoiler(&bound, &units, &plan);
    log += "\nNPC equipment\n";
    for line in &dressed {
        log += &format!("  {line}\n");
    }
    log += enemy_log.as_deref().unwrap_or("");
    std::fs::write(&spoiler, log)?;
    if !mods.is_empty() {
        eprintln!(
            "this seed's regulation.bin already holds the mods above: don't let another mod's \
             regulation.bin load over it"
        );
    }
    for m in mods {
        for file in mod_root(m).map_or(Vec::new(), |root| overlaps(root, &out)) {
            eprintln!(
                "warning: {} and this seed both have {file}; if both are loaded, only one is used",
                mod_label(m)
            );
        }
    }
    eprintln!(
        "seed {seed}: {} slots shuffled; mod folder {}, spoiler log {}",
        units.len(),
        out.display(),
        spoiler.display()
    );
    Ok(())
}

/// Move the enemies and write what changed: maps, scripts and health bar names.
fn move_enemies(
    game: &std::path::Path,
    out: &std::path::Path,
    sources: &Sources,
    model: &gamedata::model::Model,
    reg: &mut fs_param::Regulation,
    seed: u64,
    foes: &enemies::Options,
) -> Result<String> {
    let pool = enemies::pool(sources, model)?;
    let plan = enemies::shuffle(&pool, seed, foes);
    let applied = enemies::apply(reg, sources, &pool, &plan, foes)?;
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(game)?;
    let write = |path: String, plain: Vec<u8>| -> Result<()> {
        let (header, _) = fs_dcx::decompress(&dvd.read_path(&path)?)?;
        let to = out.join(path.trim_start_matches('/'));
        std::fs::create_dir_all(to.parent().unwrap_or(out))?;
        let data = fs_dcx::compress(&header.with_algorithm(fs_dcx::Algorithm::Zstd), &plain)?;
        std::fs::write(to, data)?;
        Ok(())
    };
    applied
        .maps
        .par_iter()
        .try_for_each(|(m, msb)| write(format!("/map/mapstudio/{m}.msb.dcx"), msb.write()?))?;
    applied
        .scripts
        .par_iter()
        .try_for_each(|(m, script)| write(format!("/event/{m}.emevd.dcx"), script.write()?))?;
    bar_text(&dvd, out, &applied.bar_names)?;
    eprintln!(
        "enemies: {} maps changed, {} scripts, {} new NpcParam rows",
        applied.maps.len(),
        applied.scripts.len(),
        applied.new_rows
    );
    Ok(enemies::spoiler(&pool, &plan, foes))
}

/// A health bar names who stands there now. English only for now, as with the
/// keepsakes.
fn bar_text(
    dvd: &fs_dvdbnd::Dvd,
    out: &std::path::Path,
    names: &std::collections::BTreeMap<i32, String>,
) -> Result<()> {
    const ITEMS: &str = "/msg/engus/item_dlc02.msgbnd.dcx";
    let (header, plain) = fs_dcx::decompress(&dvd.read_path(ITEMS)?)?;
    let mut bnd = fs_bnd4::Bnd4::read(&plain)?;
    for f in &mut bnd.files {
        let is_npc_name = f
            .short_name()
            .is_some_and(|n| n.starts_with("NpcName") && n.ends_with(".fmg"));
        if !is_npc_name {
            continue;
        }
        let mut fmg = fs_fmg::Fmg::read(&f.data)?;
        let mut changed = false;
        for (id, name) in names {
            if fmg.entries.contains_key(id) {
                fmg.entries.insert(*id, Some(name.clone()));
                changed = true;
            }
        }
        if changed {
            f.data = fmg.write()?;
        }
    }
    let path = out.join(ITEMS.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap_or(out))?;
    let data = fs_dcx::compress(
        &header.with_algorithm(fs_dcx::Algorithm::Zstd),
        &bnd.write()?,
    )?;
    std::fs::write(path, data)?;
    Ok(())
}

/// Character creation names its keepsakes from menu text of its own, so write
/// what each now is, in the file the game reads (the last DLC's holds it all).
/// English only for now.
fn keepsake_text(
    game: &std::path::Path,
    out: &std::path::Path,
    names: &[(i32, String)],
) -> Result<()> {
    const MENU: &str = "/msg/engus/menu_dlc02.msgbnd.dcx";
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(game)?;
    let (header, plain) = fs_dcx::decompress(&dvd.read_path(MENU)?)?;
    let mut bnd = fs_bnd4::Bnd4::read(&plain)?;
    for f in &mut bnd.files {
        let line = match f.short_name() {
            Some("GR_MenuText.fmg") => None,
            Some("GR_LineHelp.fmg") => Some("Your keepsake, chosen by the seed."),
            _ => continue,
        };
        let mut fmg = fs_fmg::Fmg::read(&f.data)?;
        for (id, name) in names {
            let text = line.unwrap_or(name.as_str()).to_string();
            fmg.entries.insert(*id, Some(text));
        }
        f.data = fmg.write()?;
    }
    let path = out.join(MENU.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap_or(out))?;
    let data = fs_dcx::compress(
        &header.with_algorithm(fs_dcx::Algorithm::Zstd),
        &bnd.write()?,
    )?;
    std::fs::write(path, data)?;
    Ok(())
}

/// Put the native module in the mod folder, and write a me3 profile beside
/// it that loads both: `<out>.me3`.
fn native_module(out: &std::path::Path, native: Option<&std::path::Path>) -> Result<()> {
    const DLL: &str = "apoc_native.dll";
    let from = match native {
        Some(p) => p.to_path_buf(),
        None => std::env::current_exe()?.with_file_name(DLL),
    };
    if !from.is_file() {
        return Err(format_err(format!(
            "--auto-upgrade needs the native module, and {} is not there (give it with --native)",
            from.display()
        )));
    }
    std::fs::copy(&from, out.join(DLL))?;
    let name = out
        .file_name()
        .ok_or_else(|| format_err("the mod folder has no name"))?
        .to_string_lossy();
    let profile = format!(
        "profileVersion = \"v1\"\n\n[[supports]]\ngame = \"eldenring\"\n\n\
         [[packages]]\npath = \"{name}\"\n\n[[natives]]\npath = \"{name}/{DLL}\"\n"
    );
    std::fs::write(out.with_extension("me3"), profile)?;
    Ok(())
}

fn check(
    game: PathBuf,
    from: u64,
    seeds: u64,
    options: &items::Options,
    mods: &[PathBuf],
) -> Result<()> {
    let t = Instant::now();
    let mut sources = Sources::open(&game)?;
    known_patch(&sources.reg.version())?;
    merge_mods(&game, &mut sources, mods)?;
    let model = elden_ring::extract(&sources)?;
    let graph = logic::elden_ring();
    let bound = logic::Bound::new(&graph, &model);
    let units = items::units(&model, &bound, &sources.reg)?;
    let keys = bound.key_items();
    let key_slots = units
        .iter()
        .filter(|u| u.item.name.as_deref().is_some_and(|n| keys.contains(n)))
        .count();
    eprintln!(
        "{} slots ({} sure), {} holding key items; model built in {:.1}s",
        units.len(),
        units.iter().filter(|u| u.sure()).count(),
        key_slots,
        t.elapsed().as_secs_f64()
    );
    let t = Instant::now();
    let done = AtomicUsize::new(0);
    let failures: Vec<String> = (from..from + seeds)
        .into_par_iter()
        .filter_map(|seed| {
            let r = items::shuffle(&bound, &units, seed, options).err();
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(1000) {
                eprintln!("  {n} seeds");
            }
            r
        })
        .collect();
    eprintln!(
        "{} seeds placed and proven in {:.1}s; {} failed",
        seeds,
        t.elapsed().as_secs_f64(),
        failures.len()
    );
    for f in failures.iter().take(20) {
        eprintln!("  {f}");
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format_err(format!("{} seeds failed", failures.len())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mod_is_named_after_its_file_or_folder() {
        assert_eq!(mod_label(Path::new("/m/Balance/regulation.bin")), "Balance");
        assert_eq!(mod_label(Path::new("/m/Balance")), "Balance");
        assert_eq!(mod_label(Path::new("/m/balance.bin")), "balance.bin");
        assert_eq!(mod_label(Path::new("/m/Balance/root-files")), "Balance");
        assert_eq!(
            mod_label(Path::new("/m/Balance/root-files/regulation.bin")),
            "Balance"
        );
    }

    #[test]
    fn a_file_both_a_mod_and_the_seed_write_is_found_but_not_the_regulation() {
        let dir = std::env::temp_dir().join(format!("apoc-overlap-{}", std::process::id()));
        let (m, out) = (dir.join("mod"), dir.join("out"));
        for f in [
            m.join("main/msg/engus/item.dcx"),
            m.join("root-files/regulation.bin"),
            m.join("main/other.dcx"),
            out.join("msg/engus/item.dcx"),
            out.join("regulation.bin"),
        ] {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b"").unwrap();
        }
        let same = overlaps(&m, &out);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(same.len(), 1, "{same:?}");
        assert!(same[0].ends_with("item.dcx"), "{same:?}");
    }

    #[test]
    fn a_path_with_no_regulation_is_refused_by_name() {
        let e = mod_regulation(Path::new("/no/such/mod"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("/no/such/mod"), "{e}");
    }

    #[test]
    fn an_unknown_patch_is_refused() {
        assert!(known_patch("11730034").is_ok());
        let e = known_patch("11740000").unwrap_err().to_string();
        assert!(e.contains("11740000"), "{e}");
    }
}

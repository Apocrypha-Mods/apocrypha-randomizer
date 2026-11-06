//! Seeds as a library: what `apoc-rando` does, for whatever drives it, the command
//! line or the mod manager. Nothing here prints; what a run has to say goes to `say`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use std::collections::BTreeSet;

use fs_bytes::{Result, format_err};
use fs_param::merge;
pub use fs_param::merge::Contribution;
use gamedata::model::{Item, ItemKind};
use gamedata::sources::{Text, text_of};
use gamedata::{GameFiles, MapId, Sources, elden_ring};
use rayon::prelude::*;

pub mod bearings;
pub mod capital;
pub mod colours;
pub mod dlc_start;
pub mod far;
pub mod gestures;
pub mod kindling;
pub mod music;
pub mod night;
pub mod tweaks;
pub mod whole_map;

pub use enemies::Oops;
pub use enemies::Options as EnemyOptions;
pub use items::Options as ItemOptions;
pub use items::options::{DEFAULT_LEYNDELL_RUNES, MAX_LEYNDELL_RUNES};

/// Where a run's messages go. It may be called from several threads.
pub type Say<'a> = &'a (dyn Fn(&str) + Sync);

/// Where a seed's files go: a mod folder, or the mod manager's add-on frame, which
/// has no folders. `path` is from the mod's root, `map/mapstudio/m10_00_00_00.msb.dcx`.
pub trait Sink: Sync {
    fn put(&self, path: &str, bytes: Vec<u8>) -> Result<()>;
}

/// A mod folder, remembering what it was given.
pub struct Folder<'a>(pub &'a Path, pub std::sync::Mutex<BTreeSet<String>>);

impl Sink for Folder<'_> {
    fn put(&self, path: &str, bytes: Vec<u8>) -> Result<()> {
        let to = self.0.join(path);
        std::fs::create_dir_all(to.parent().unwrap_or(self.0))?;
        std::fs::write(to, bytes)?;
        if let Ok(mut written) = self.1.lock() {
            written.insert(path.replace('\\', "/"));
        }
        Ok(())
    }
}

/// Delete what the last seed wrote to this folder and this one did not, as me3
/// would serve it over the game, and list what this one wrote for the next. The
/// list sits beside the folder. Files not on the list are never touched, but a
/// folder with no list that already holds a game path is warned about.
fn tidy(out: &Path, mut written: BTreeSet<String>, say: Say) -> Result<()> {
    if out.join("apoc_native.dll").is_file() {
        written.insert("apoc_native.dll".to_string());
    }
    let list = out.with_extension("files");
    match std::fs::read_to_string(&list) {
        Ok(old) => {
            let stale: Vec<&str> = old
                .lines()
                .filter(|l| !written.contains(*l))
                .filter(|l| {
                    !l.is_empty() && !l.starts_with('/') && !l.split('/').any(|c| c == "..")
                })
                .collect();
            for path in &stale {
                let to = out.join(path);
                match std::fs::remove_file(&to) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
                // Folders left empty go too; one still in use refuses, and stays.
                let mut dir = to.parent();
                while let Some(d) = dir.filter(|d| *d != out) {
                    if std::fs::remove_dir(d).is_err() {
                        break;
                    }
                    dir = d.parent();
                }
            }
            if !stale.is_empty() {
                say(&format!("removed {} files of the last seed", stale.len()));
            }
        }
        Err(_) => {
            for dir in ["map", "event", "script", "msg", "chr"] {
                if !written.iter().any(|w| w.starts_with(dir)) && out.join(dir).is_dir() {
                    say(&format!(
                        "warning: {} holds a {dir} folder this seed did not write; it may be from an older seed",
                        out.display()
                    ));
                }
            }
        }
    }
    let mut body = written.into_iter().collect::<Vec<_>>().join("\n");
    body.push('\n');
    std::fs::write(list, body)?;
    Ok(())
}

/// What a seed is, whatever it is built from and wherever it goes.
#[derive(Clone, Copy)]
pub struct Job<'a> {
    pub seed: u64,
    pub options: &'a ItemOptions,
    pub enemies: &'a EnemyOptions,
}

/// A seed built by [`build`].
pub struct Built {
    pub slots: usize,
    pub spoiler: String,
}

/// One seed to build.
pub struct Request {
    /// The game folder, holding `eldenring.exe` and `regulation.bin`.
    pub game: PathBuf,
    pub seed: u64,
    /// The mod folder to write.
    pub out: PathBuf,
    pub options: ItemOptions,
    pub enemies: EnemyOptions,
    /// The native module for auto-upgrade, if not beside this program.
    pub native: Option<PathBuf>,
    /// Other mods to build on, lowest priority first.
    pub mods: Vec<PathBuf>,
    /// A hidden seed (Nova, 2026-11-06): no spoiler log is written, and one left by an
    /// earlier seed in its place is removed, so nothing beside the mod tells where
    /// anything went.
    pub hidden: bool,
}

/// What a built seed left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub slots: usize,
    pub folder: PathBuf,
    /// None for a hidden seed.
    pub spoiler: Option<PathBuf>,
    /// The me3 profile that loads it all, with a save of the seed's own.
    pub profile: Option<PathBuf>,
}

/// `NEEDED/COUNT` shards of Messmer's Kindling, as typed: 1 to 20 shards, at least one
/// needed and at most all.
pub fn shards_of(spec: &str) -> std::result::Result<(u8, u8), String> {
    let bad = || format!("'{spec}': shards are NEEDED/COUNT, such as 3/5, with 1 to 20 shards");
    let (n, c) = spec.trim().split_once('/').ok_or_else(bad)?;
    let (n, c): (u8, u8) = (
        n.trim().parse().map_err(|_| bad())?,
        c.trim().parse().map_err(|_| bad())?,
    );
    if c == 0 || c > items::kindling::MOST || n == 0 || n > c {
        return Err(bad());
    }
    Ok((n, c))
}

/// A seed nobody chose and nobody is told, for a hidden seed: drawn from the
/// randomness the standard library seeds its hash maps with, which differs from run to
/// run. At most 15 digits, as the page takes.
pub fn hidden_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish() % 1_000_000_000_000_000
}

/// The `regulation.bin` version stamps the area map and the gate have been run
/// against. Anything else may have moved what the logic relies on.
const KNOWN_REGULATIONS: [&str; 2] = ["11711000", "11730034"];

/// Refuse a game patch that has not been checked.
pub fn known_patch(version: &str) -> Result<()> {
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

/// Where a mod keeps its `regulation.bin`: the path itself, in the folder, or
/// (as the mod manager stores them) under `root-files`.
fn mod_regulation(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    for inside in [
        "regulation.bin",
        "root-files/regulation.bin",
        "mod/regulation.bin",
    ] {
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

/// The folders of a mod that hold the game's own paths, for me3 to serve and us
/// to read: where its `regulation.bin` is, or for the mod manager's layout, `main`
/// (its `root-files` holds the regulation alone). A bare file has none.
fn mod_dirs(path: &Path) -> Result<Vec<PathBuf>> {
    let file = mod_regulation(path)?;
    if file.file_name().is_none_or(|n| n != "regulation.bin") {
        return Ok(Vec::new());
    }
    let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok(match (dir.file_name(), dir.parent()) {
        (Some(n), Some(up)) if n == "root-files" => vec![up.join("main")]
            .into_iter()
            .filter(|d| d.is_dir())
            .collect(),
        _ => vec![dir],
    })
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
        Some(d)
            if d.file_name()
                .is_some_and(|n| n == "root-files" || n == "mod") =>
        {
            d.parent()
        }
        d => d,
    };
    dir.and_then(named)
        .unwrap_or_else(|| path.display().to_string())
}

/// Merge the other mods onto the game's own file and put the result in
/// `sources`, so everything after this reads the tables that will ship. A mod
/// that cannot be merged stops the run, as the player asked for it by name.
/// Returns the report for the spoiler log, empty with no mods.
fn merge_mods(game: &Path, sources: &mut Sources, mods: &[PathBuf], say: Say) -> Result<String> {
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
    let (merged, log) = merged_tables(&key, &base, &contributions)?;
    sources.reg = merged;
    say(log.trim_end());
    Ok(log)
}

/// The game's tables with each mod's changes on them, lowest priority first, and the report
/// for the spoiler log. A mod that cannot be merged stops the run, naming it.
pub fn merged_tables(
    key: &[u8; 32],
    base: &[u8],
    contributions: &[Contribution],
) -> Result<(fs_param::Regulation, String)> {
    let outcome = merge::merge(key, base, contributions)?;
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
    Ok((fs_param::Regulation::read(key, &bytes)?, merge_log(&report)))
}

/// A mod manager merges a mod's tables onto the game's at Apply, by what differs from them.
/// A seed built on other mods is taken whole there, by itself, so run that merge and refuse
/// a seed it would change, whose proof would no longer hold.
pub fn survives_merge(key: &[u8; 32], base: &[u8], seed: &[u8]) -> Result<()> {
    let (merged, _) = merged_tables(
        key,
        base,
        &[Contribution {
            label: "this seed".to_string(),
            bytes: seed.to_vec(),
        }],
    )?;
    let seed = fs_param::Regulation::read(key, seed)?;
    let lost: Vec<&str> = seed
        .table_names()
        .filter(|name| match (seed.table(name), merged.table(name)) {
            (Ok(a), Ok(b)) => a != b,
            _ => true,
        })
        .collect();
    if lost.is_empty() {
        Ok(())
    } else {
        Err(format_err(format!(
            "the mod manager's merge would change this seed's tables ({}), so its proof would not hold",
            lost.join(", ")
        )))
    }
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
    log += "  The proof covers these tables and the mods' maps, scripts, talk and text.\n";
    log
}

/// What a mod's own me3 profile loads as natives (DLLs), by full path. A DLL it
/// names and does not ship is left out and said: me3 would refuse the profile.
fn mod_natives(dirs: &[PathBuf], say: Say) -> Vec<PathBuf> {
    let mut places: Vec<&Path> = Vec::new();
    for d in dirs {
        places.push(d);
        places.extend(d.parent());
    }
    let mut out = Vec::new();
    let mut profile_found = false;
    for place in places {
        let Ok(entries) = std::fs::read_dir(place) else {
            continue;
        };
        let mut profiles: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "me3"))
            .collect();
        profiles.sort();
        // A mod with several profiles has one of them as its main one, which
        // cannot be told; the first is taken, and the run names it.
        let Some(profile) = profiles.first() else {
            continue;
        };
        profile_found = true;
        let parsed = std::fs::read_to_string(profile)
            .ok()
            .and_then(|t| t.parse::<toml::Table>().ok());
        let Some(table) = parsed else {
            say(&format!("warning: {} could not be read", profile.display()));
            continue;
        };
        for key in ["natives", "native"] {
            let list = table.get(key).and_then(|v| v.as_array());
            for path in list
                .into_iter()
                .flatten()
                .filter_map(|n| n.get("path")?.as_str())
            {
                let dll = place.join(path);
                if dll.is_file() {
                    out.push(dll);
                } else {
                    say(&format!(
                        "warning: {} names {path}, which is not there; left out",
                        profile.display()
                    ));
                }
            }
        }
        break;
    }
    // A mod with no profile of its own means its `dll` folder to be loaded whole.
    if !profile_found {
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir.join("dll")) else {
                continue;
            };
            let mut dlls: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")))
                .collect();
            dlls.sort();
            out.extend(dlls);
        }
    }
    out
}

/// A me3 profile beside the mod folder: the mods, lowest priority first, then the
/// seed, which me3 lets override them all, as it holds what the mods change.
fn write_profile(out: &Path, mods: &[PathBuf], native: bool, say: Say) -> Result<PathBuf> {
    let name = out
        .file_name()
        .ok_or_else(|| format_err("the mod folder has no name"))?
        .to_string_lossy();
    // A save of its own: a seed's progress and the player's real game never mix.
    let mut text = format!(
        "profileVersion = \"v1\"\nsavefile = \"{name}.sl2\"\n\n[[supports]]\ngame = \"eldenring\"\n"
    );
    let mut ids: Vec<String> = Vec::new();
    let mut natives: Vec<String> = Vec::new();
    for m in mods {
        let dirs = mod_dirs(m)?;
        natives.extend(
            mod_natives(&dirs, say)
                .iter()
                .map(|d| std::fs::canonicalize(d).map(|p| p.display().to_string()))
                .collect::<std::io::Result<Vec<_>>>()?,
        );
        for dir in &dirs {
            let id = format!("mod-{}", ids.len());
            let path = std::fs::canonicalize(dir)?;
            text += &format!(
                "\n[[packages]]\nid = \"{id}\"\npath = '{}'\n",
                path.display()
            );
            if let Some(before) = ids.last() {
                text += &format!("load_after = [{{ id = \"{before}\", optional = false }}]\n");
            }
            ids.push(id);
        }
    }
    text += &format!("\n[[packages]]\nid = \"apoc-rando-seed\"\npath = \"{name}\"\n");
    if let Some(before) = ids.last() {
        text += &format!("load_after = [{{ id = \"{before}\", optional = false }}]\n");
    }
    for dll in natives {
        text += &format!("\n[[natives]]\npath = '{dll}'\n");
    }
    if native {
        text += &format!("\n[[natives]]\npath = \"{name}/apoc_native.dll\"\n");
    }
    let profile = out.with_extension("me3");
    std::fs::write(&profile, text)?;
    Ok(profile)
}

/// The model of what will ship. A boss keeps the name it has in the unmodded
/// game, as the area map knows it by that: a mod may give a fight's health bar
/// a name of its own (ELDEN VINS does for Makar), and it is the same fight.
fn extract_model(
    game: &Path,
    sources: &Sources,
    mods: &[PathBuf],
) -> Result<gamedata::model::Model> {
    let mut model = elden_ring::extract(sources)?;
    if !mods.is_empty() {
        let stock = elden_ring::extract(&Sources::open(game)?)?;
        keep_boss_names(&mut model, &boss_names(stock));
    }
    Ok(model)
}

/// Each boss's name as the unmodded game has it, by its area.
pub fn boss_names(stock: gamedata::model::Model) -> BossNames {
    stock.bosses.into_iter().map(|b| (b.area, b.name)).collect()
}

/// [`boss_names`]' result.
pub type BossNames = std::collections::HashMap<i32, Option<String>>;

/// Put the unmodded names back on a model read from mods' files.
pub fn keep_boss_names(model: &mut gamedata::model::Model, names: &BossNames) {
    for boss in &mut model.bosses {
        if let Some(name) = names.get(&boss.area) {
            boss.name = name.clone();
        }
    }
}

/// The game's files with the mods' folders over them.
fn open_files(game: &Path, mods: &[PathBuf]) -> Result<GameFiles> {
    let mut dirs = Vec::new();
    for m in mods {
        dirs.extend(mod_dirs(m)?);
    }
    GameFiles::open(game, &dirs)
}

/// Refuse to write inside the game's folder, as a later seed removes what the last one
/// wrote there. `out` may not exist yet, so its nearest existing parent is what is compared.
fn not_in_game(game: &Path, out: &Path) -> Result<()> {
    let game = game.canonicalize()?;
    let absolute = std::path::absolute(out)?;
    let parent = absolute.ancestors().find(|a| a.exists());
    if parent
        .and_then(|a| a.canonicalize().ok())
        .is_some_and(|a| a.starts_with(&game))
    {
        return Err(format_err(format!(
            "{} is inside the game's folder; choose a mod folder elsewhere",
            out.display()
        )));
    }
    Ok(())
}

/// Build one seed's mod folder, its spoiler log and, where needed, its profile.
pub fn generate(request: &Request, say: Say) -> Result<Outcome> {
    let Request {
        game,
        seed,
        out,
        options,
        enemies: foes,
        native,
        mods,
        hidden,
    } = request;
    let (game, mods) = (game.as_path(), mods.as_slice());
    not_in_game(game, out)?;
    let files = open_files(game, mods)?;
    let mut sources = Sources::open_with(game, &files)?;
    known_patch(&sources.reg.version())?;
    let merged = merge_mods(game, &mut sources, mods, say)?;
    let model = extract_model(game, &sources, mods)?;
    std::fs::create_dir_all(out)?;
    if options.auto_upgrade {
        native_module(out, native.as_deref())?;
    }
    let folder = Folder(out, Default::default());
    let built = build(
        &files,
        &sources,
        &model,
        Job {
            seed: *seed,
            options,
            enemies: foes,
        },
        &folder,
        say,
    )?;
    tidy(out, folder.1.into_inner().unwrap_or_default(), say)?;
    let path = out.with_extension("spoiler.txt");
    let spoiler = if *hidden {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => None,
        }
    } else {
        std::fs::write(&path, merged + &built.spoiler)?;
        Some(path)
    };
    for file in &files.shadowed {
        say(&format!(
            "warning: more than one mod has {file}; the later one is used"
        ));
    }
    // Always: the profile is what gives a seed a save of its own.
    let profile = Some(write_profile(out, mods, options.auto_upgrade, say)?);
    Ok(Outcome {
        slots: built.slots,
        folder: out.clone(),
        spoiler,
        profile,
    })
}

/// The seed itself: place the items, move the enemies, and put what changed in
/// `sink`. Touches no folder, so it runs wherever the game's files can be handed
/// to it.
pub fn build(
    files: &GameFiles,
    sources: &Sources,
    model: &gamedata::model::Model,
    job: Job,
    sink: &dyn Sink,
    say: Say,
) -> Result<Built> {
    let Job {
        seed,
        options,
        enemies: foes,
    } = job;
    let graph = graph_for(options)?;
    let bound = logic::Bound::new(&graph, model);
    if !bound.problems.is_empty() {
        return Err(format_err(format!(
            "the area map does not fit this game: {:?}",
            bound.problems
        )));
    }
    let mut units = items::units(model, &bound, &sources.reg)?;
    items::kindling::split(model, &mut units, options.kindling_shards).map_err(format_err)?;
    let mut options = options.clone();
    options.enemies_move = !foes.is_off();
    let options = &options;
    let plan = items::shuffle(&bound, &units, seed, options).map_err(format_err)?;
    let mut reg = sources.reg.clone();
    items::apply(&mut reg, &units, &plan)?;
    let dressed = items::dress_npcs(&mut reg, model, seed, options)?;
    let mut given = bearings::apply(sources, &graph, model, &mut reg)?;
    let extras = dlc_start::Extras {
        level: options.dlc_start_level,
        flasks: options.dlc_start_flasks,
        smithing: options.dlc_start_smithing,
        shop: options.dlc_start_shop,
    };
    let shop = options.dlc_start && extras.shop;
    if shop {
        let stock = dlc_start::base_equipment(model, &units, &sources.reg)?;
        let last = dlc_start::shop_rows(&mut reg, &stock)?;
        let (map, _, grace) = sources
            .talk
            .iter()
            .find(|(_, name, _)| name == dlc_start::GRACE_SCRIPT)
            .ok_or_else(|| format_err("the Site of Grace's talk script is missing"))?;
        let scripts = given
            .binders
            .entry(format!("/script/talk/{map}.talkesdbnd.dcx"))
            .or_default();
        let current = scripts
            .get(dlc_start::GRACE_SCRIPT)
            .unwrap_or(grace)
            .clone();
        scripts.insert(
            dlc_start::GRACE_SCRIPT.to_string(),
            dlc_start::grace_script(&current, last)?,
        );
        say(&format!(
            "base-game equipment at graces: {} rows ({:?})",
            stock.len(),
            dlc_start::stock_counts(&stock)
        ));
    }
    for (path, data) in bearings::binders(files, &given)? {
        sink.put(&path, data)?;
    }
    let mut talk: Vec<(i32, Words)> = Vec::new();
    if !given.log.is_empty() {
        talk.push((bearings::TEXT, bearings::words));
    }
    if shop {
        talk.push((dlc_start::SHOP_TEXT, dlc_start::shop_words));
    }
    menu_text(files, sink, &items::keepsake_items(&units, &plan), &talk)?;
    // Always, so the game's gate asks what the proof asked (`capital`).
    let mut common = capital::apply(&sources.common, options.leyndell_runes)?;
    if options.whole_map {
        common = whole_map::apply(&common, &sources.reg)?;
    }
    common = kindling::apply(&common, options.kindling_shards, options.kindling_needed)?;
    kindling::goods(&mut reg, options.kindling_shards)?;
    if options.dlc_start {
        common = dlc_start::apply(&common, &extras)?;
        dlc_start::lots(&mut reg, &extras)?;
    }
    put_compressed(files, sink, "/event/common.emevd.dcx", &common.write()?)?;
    let night = options.night_bosses_always;
    let gestures = if options.random_gestures {
        gestures::plan(sources, seed)
    } else {
        gestures::Plan::default()
    };
    let edit = |m: MapId, script: &mut fs_emevd::Emevd| -> Result<bool> {
        let mut changed = false;
        if night {
            changed |= night::edit_script(script);
        }
        changed |= gestures.edit(tile_of(m), script)?;
        Ok(changed)
    };
    // Before the enemies move, so the rows they copy carry the colour along.
    if options.enemy_colours {
        let rows = colours::rows_to_colour(model);
        let n = colours::apply(&mut reg, &rows, seed)?;
        say(&format!("enemy colours: {} kinds of enemy", n.len()));
    }
    let moved = if foes.is_off() {
        Moved::default()
    } else {
        move_enemies(files, job, sources, model, &mut reg, sink, &edit, say)?
    };
    item_text(
        files,
        sink,
        &moved.bar_names,
        &moved.bar_sources,
        &kindling::text(options.kindling_shards),
    )?;
    let written = moved.written;
    let enemy_log = (!foes.is_off()).then_some(moved.log);
    if night {
        let n = night::apply(&mut reg)?;
        let b = night::birds(&mut reg)?;
        say(&format!(
            "night bosses at any hour: {n} rows, {b} birds' effects"
        ));
    }
    // The scripts the enemies left alone; those they changed were edited as written. A
    // later version of a tile whose script is the tile's copy takes the same edits.
    let gesture_maps = gestures.maps();
    let mut scripts = 0;
    for (m, script) in &sources.scripts {
        if written.contains(m) || !(night || gesture_maps.contains_key(m)) {
            continue;
        }
        let mut edited = script.clone();
        if !edit(*m, &mut edited)? {
            continue;
        }
        let bytes = edited.write()?;
        put_compressed(files, sink, &format!("/event/{m}.emevd.dcx"), &bytes)?;
        scripts += 1;
        if let Some(t) = far::twin(*m)
            && gesture_maps.contains_key(m)
            && !written.contains(&t)
        {
            let (tp, mp) = (
                format!("/event/{t}.emevd.dcx"),
                format!("/event/{m}.emevd.dcx"),
            );
            if files.exists(&tp)
                && fs_dcx::decompress(&files.read(&tp)?)?.1
                    == fs_dcx::decompress(&files.read(&mp)?)?.1
            {
                put_compressed(files, sink, &tp, &bytes)?;
                scripts += 1;
            }
        }
    }
    if scripts > 0 {
        say(&format!(
            "{scripts} map scripts edited for the night and the gestures"
        ));
    }
    if options.all_recipes {
        let n = tweaks::all_recipes(&mut reg)?;
        say(&format!("every crafting recipe known: {n} rows"));
    }
    if options.cheap_upgrades {
        let n = tweaks::cheap_upgrades(&mut reg)?;
        say(&format!("one stone per upgrade: {n} costs"));
    }
    if options.no_requirements {
        let n = tweaks::no_requirements(&mut reg)?;
        say(&format!("no stat requirements: {n} rows"));
    }
    if options.torrent_woods {
        tweaks::torrent_in_woods(&mut reg)?;
        say("Torrent in the Abyssal Woods");
    }
    if options.boss_music {
        let mut scripts: Vec<&fs_emevd::Emevd> = vec![&sources.common, &sources.common_func];
        scripts.extend(sources.scripts.iter().map(|(_, s)| s));
        let played = music::played(&scripts);
        let n = music::shuffle_boss(&mut reg, &played, seed)?;
        say(&format!("boss music shuffled: {n} themes"));
    }
    if options.area_music {
        let n = music::shuffle_areas(&mut reg, seed)?;
        say(&format!("area music shuffled: {} pieces", n.len()));
    }
    if options.no_malenia_heal {
        let n = tweaks::no_malenia_heal(&mut reg)?;
        say(&format!("Malenia heals on no hit: {} effects", n.len()));
    }
    if options.no_gargoyle_poison {
        let n = tweaks::no_gargoyle_poison(&mut reg)?;
        say(&format!(
            "the gargoyles' clouds hurt no one: {} bullets",
            n.len()
        ));
    }
    sink.put("regulation.bin", reg.write()?)?;
    let mut log = items::spoiler(&bound, &units, &plan);
    log += "\nNPC equipment\n";
    for line in &dressed {
        log += &format!("  {line}\n");
    }
    if !given.log.is_empty() {
        log += &format!(
            "\nBell bearings given in talk (\"{}\"), for the Twin Maiden Husks\n",
            bearings::WORDS
        );
        for line in &given.log {
            log += &format!("  {line}\n");
        }
    }
    log += enemy_log.as_deref().unwrap_or("");
    if options.random_gestures {
        log += &gestures.log(sources);
    }
    Ok(Built {
        slots: units.len(),
        spoiler: log,
    })
}

/// The area map as this seed's options set it.
pub fn graph_for(options: &items::Options) -> Result<logic::Graph> {
    let mut graph = logic::elden_ring();
    graph
        .set_leyndell_runes(usize::from(options.leyndell_runes))
        .map_err(format_err)?;
    graph.set_gaol_keys(options.gaol_keys).map_err(format_err)?;
    graph.set_dlc_start(options.dlc_start).map_err(format_err)?;
    graph
        .set_kindling_shards(
            &items::kindling::names(options.kindling_shards),
            usize::from(options.kindling_needed),
        )
        .map_err(format_err)?;
    Ok(graph)
}

/// Write a file of the game's in the DCX its own copy has, compressed with zstd.
fn put_compressed(files: &GameFiles, out: &dyn Sink, path: &str, plain: &[u8]) -> Result<()> {
    let (header, _) = fs_dcx::decompress(&files.read(path)?)?;
    let data = fs_dcx::compress(&header.with_algorithm(fs_dcx::Algorithm::Zstd), plain)?;
    out.put(path.trim_start_matches('/'), data)
}

/// What else of the seed goes into a map's script as it is written: the night bosses'
/// hours and the gestures. Whether it changed anything.
type ScriptEdit<'a> = dyn Fn(MapId, &mut fs_emevd::Emevd) -> Result<bool> + Sync + 'a;

/// The tile a later version of a tile stands for ([`far::twin`]), or the map itself.
fn tile_of(m: MapId) -> MapId {
    let [a, b, c, d] = m.0;
    if m.is_open_world() && d >= 10 {
        MapId([a, b, c, d - 10])
    } else {
        m
    }
}

/// What moving the enemies left for the rest of the seed.
#[derive(Default)]
struct Moved {
    log: String,
    /// The maps whose scripts were written.
    written: BTreeSet<MapId>,
    /// Health bar text for [`item_text`].
    bar_names: BTreeMap<i32, String>,
    bar_sources: BTreeMap<i32, i32>,
}

/// Move the enemies and write what changed: maps and scripts. Health bar names are
/// left for [`item_text`].
#[allow(clippy::too_many_arguments)]
fn move_enemies(
    files: &GameFiles,
    job: Job,
    sources: &Sources,
    model: &gamedata::model::Model,
    reg: &mut fs_param::Regulation,
    out: &dyn Sink,
    edit: &ScriptEdit<'_>,
    say: Say,
) -> Result<Moved> {
    let (seed, foes) = (job.seed, job.enemies);
    let pool = enemies::pool(sources, model)?;
    enemies::oops_body(&pool, seed, foes).map_err(format_err)?;
    let plan = enemies::shuffle(&pool, seed, foes);
    let applied = enemies::apply(reg, sources, &pool, &plan, foes)?;
    let write = |path: String, plain: Vec<u8>| put_compressed(files, out, &path, &plain);
    let twins = far::follow(files, sources, &applied.maps, &applied.scripts)?;
    applied
        .maps
        .par_iter()
        .chain(twins.maps.par_iter())
        .try_for_each(|(m, msb)| write(format!("/map/mapstudio/{m}.msb.dcx"), msb.write()?))?;
    applied
        .scripts
        .par_iter()
        .chain(twins.scripts.par_iter())
        .try_for_each(|(m, script)| {
            let mut script = script.clone();
            edit(*m, &mut script)?;
            write(format!("/event/{m}.emevd.dcx"), script.write()?)
        })?;
    let written: BTreeSet<MapId> = applied
        .scripts
        .iter()
        .chain(twins.scripts.iter())
        .map(|(m, _)| *m)
        .collect();
    say(&format!(
        "enemies: {} later versions of tiles follow, {} scripts; {} differ from their tile, {} changed enemies unmatched there",
        twins.maps.len(),
        twins.scripts.len(),
        twins.differing,
        twins.unmatched
    ));
    say(&format!(
        "enemies: {} maps changed, {} scripts, {} new NpcParam rows",
        applied.maps.len(),
        applied.scripts.len(),
        applied.new_rows
    ));
    if foes.onslaught > 1 {
        say(&format!(
            "onslaught: {} copies in {} maps",
            applied.onslaught.copies, applied.onslaught.maps
        ));
    }
    Ok(Moved {
        log: enemies::spoiler(&pool, &plan, foes),
        written,
        bar_names: applied.bar_names,
        bar_sources: applied.bar_sources,
    })
}

/// The languages the game ships text in; each is written where the game holds it.
pub const LANGUAGES: [&str; 15] = [
    "engus", "araae", "deude", "frafr", "itait", "jpnjp", "korkr", "polpl", "porbr", "rusru",
    "spaar", "spaes", "thath", "zhocn", "zhotw",
];

/// One language's item and name text, as the game will see it.
fn language_text(files: &GameFiles, language: &str) -> Result<Text> {
    text_of(&files.read(&format!("/msg/{language}/item_dlc02.msgbnd.dcx"))?)
}

/// Write one text binder back into the mod folder after `edit`.
fn rewrite_text(
    files: &GameFiles,
    out: &dyn Sink,
    path: &str,
    edit: impl Fn(&mut fs_bnd4::File) -> Result<()>,
) -> Result<()> {
    let (header, plain) = fs_dcx::decompress(&files.read(path)?)?;
    let mut bnd = fs_bnd4::Bnd4::read(&plain)?;
    for f in &mut bnd.files {
        edit(f)?;
    }
    let data = fs_dcx::compress(
        &header.with_algorithm(fs_dcx::Algorithm::Zstd),
        &bnd.write()?,
    )?;
    out.put(path.trim_start_matches('/'), data)
}

/// A goods row the seed adds, named in each language from a goods row of the game:
/// its name with `suffix`, and its help and caption as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGoods {
    pub id: i32,
    pub from: i32,
    pub suffix: String,
}

/// The item text binder, written once: a health bar names who stands there now, in each
/// language, from that language's own name for them, English where it has none; and new
/// goods take their names from the rows they copy.
fn item_text(
    files: &GameFiles,
    out: &dyn Sink,
    names: &BTreeMap<i32, String>,
    sources: &BTreeMap<i32, i32>,
    goods: &[NewGoods],
) -> Result<()> {
    if names.is_empty() && goods.is_empty() {
        return Ok(());
    }
    for language in LANGUAGES {
        let path = format!("/msg/{language}/item_dlc02.msgbnd.dcx");
        if !files.exists(&path) {
            continue;
        }
        let text = language_text(files, language)?;
        let npc = text.get("NpcName");
        rewrite_text(files, out, &path, |f| {
            let short = f.short_name().unwrap_or_default().to_string();
            let goods_table = ["GoodsName", "GoodsInfo", "GoodsCaption"]
                .into_iter()
                .find(|t| short.starts_with(t) && short.ends_with(".fmg"));
            if let Some(table) = goods_table
                && !goods.is_empty()
            {
                let mut fmg = fs_fmg::Fmg::read(&f.data)?;
                let mut changed = false;
                for g in goods {
                    // Into the file that holds the row copied, and only there.
                    let Some(Some(own)) = fmg.entries.get(&g.from).cloned() else {
                        continue;
                    };
                    if fmg.entries.get(&g.id).is_some_and(|t| t.is_some()) {
                        return Err(format_err(format!("{path}: goods text {} is taken", g.id)));
                    }
                    let shown = if table == "GoodsName" {
                        format!("{own}{}", g.suffix)
                    } else {
                        own
                    };
                    fmg.entries.insert(g.id, Some(shown));
                    changed = true;
                }
                if changed {
                    f.data = fmg.write()?;
                }
                return Ok(());
            }
            let is_npc_name = short.starts_with("NpcName") && short.ends_with(".fmg");
            if !is_npc_name || names.is_empty() {
                return Ok(());
            }
            let mut fmg = fs_fmg::Fmg::read(&f.data)?;
            let mut changed = false;
            for (id, english) in names {
                if fmg.entries.contains_key(id) {
                    let own = sources.get(id).and_then(|s| npc?.get(s));
                    let new = own.unwrap_or(english);
                    // The arena's own boss, whose title the bar keeps.
                    let shown = match npc.and_then(|n| n.get(id)) {
                        Some(old) => titled(new, old),
                        None => new.clone(),
                    };
                    fmg.entries.insert(*id, Some(shown));
                    changed = true;
                }
            }
            if changed {
                f.data = fmg.write()?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// What a boss's health bar shows for `new` standing in the arena of `old`: the new
/// one's name with the old one's title, as the bars write both, "Name, Title" (Nova,
/// 2026-11-05: "Malenia, Blade of Miquella" holding Godrick shows "Godrick, Blade of
/// Miquella"). The new one's name is what comes before its own title's comma, or
/// before " the " in a name such as "Godrick the Grafted". Where the old one has no
/// title so written (Tree Sentinel, or a language that puts the title first), the
/// bar shows the new one's whole name.
pub fn titled(new: &str, old: &str) -> String {
    let Some((_, title)) = old.split_once(", ") else {
        return new.to_string();
    };
    let name = match new.split_once(", ") {
        Some((name, _)) => name,
        None => match new.split_once(" the ") {
            Some((name, _)) if !name.contains(' ') => name,
            _ => new,
        },
    };
    format!("{name}, {title}")
}

/// The text table holding names of one kind of item.
fn item_table(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Goods => "GoodsName",
        ItemKind::Weapon | ItemKind::CustomWeapon => "WeaponName",
        ItemKind::Protector => "ProtectorName",
        ItemKind::Accessory => "AccessoryName",
        ItemKind::Gem => "GemName",
    }
}

/// Talk text's words in a language, by its code (`engus`).
type Words = fn(&str) -> &'static str;

/// Character creation names its keepsakes from menu text of its own, so write
/// what each moved one now is, in the file the game reads (the last DLC's holds it
/// all), in each language. The menu's one line of help is English in all of them.
/// `talk` adds talk menu text (EventTextForTalk), its words by language. With no
/// keepsake moved, their text is left alone, and with no talk text either the file
/// is not written at all.
fn menu_text(
    files: &GameFiles,
    out: &dyn Sink,
    keepsakes: &[(i32, Item)],
    talk: &[(i32, Words)],
) -> Result<()> {
    if keepsakes.is_empty() && talk.is_empty() {
        return Ok(());
    }
    for language in LANGUAGES {
        let path = format!("/msg/{language}/menu_dlc02.msgbnd.dcx");
        if !files.exists(&path) {
            continue;
        }
        let text = language_text(files, language)?;
        let names: Vec<(i32, String)> = keepsakes
            .iter()
            .filter_map(|(id, item)| {
                let own = text
                    .get(item_table(item.kind))
                    .and_then(|t| t.get(&item.id));
                let name = own.or(item.name.as_ref())?;
                Some((
                    *id,
                    if item.count > 1 {
                        format!("{name} x{}", item.count)
                    } else {
                        name.clone()
                    },
                ))
            })
            .collect();
        rewrite_text(files, out, &path, |f| {
            let line = match f.short_name() {
                Some("GR_MenuText.fmg") if !names.is_empty() => None,
                Some("GR_LineHelp.fmg") if !names.is_empty() => {
                    Some("Your keepsake, chosen by the seed.")
                }
                Some("EventTextForTalk.fmg") if !talk.is_empty() => {
                    let mut fmg = fs_fmg::Fmg::read(&f.data)?;
                    for (id, words) in talk {
                        if fmg.entries.get(id).is_some_and(|t| t.is_some()) {
                            return Err(format_err(format!(
                                "{path}: talk text {id} is already used"
                            )));
                        }
                        fmg.entries.insert(*id, Some(words(language).to_string()));
                    }
                    f.data = fmg.write()?;
                    return Ok(());
                }
                _ => return Ok(()),
            };
            let mut fmg = fs_fmg::Fmg::read(&f.data)?;
            for (id, name) in &names {
                let shown = line.unwrap_or(name.as_str()).to_string();
                fmg.entries.insert(*id, Some(shown));
            }
            f.data = fmg.write()?;
            Ok(())
        })?;
    }
    Ok(())
}

/// Put the native module in the mod folder.
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
    Ok(())
}

/// Place items for many seeds and prove each can be finished. Every seed that
/// fails is returned with its reason.
pub fn check(
    game: &Path,
    from: u64,
    seeds: u64,
    options: &items::Options,
    mods: &[PathBuf],
    say: Say,
) -> Result<Vec<String>> {
    let t = Instant::now();
    let files = open_files(game, mods)?;
    let mut sources = Sources::open_with(game, &files)?;
    known_patch(&sources.reg.version())?;
    merge_mods(game, &mut sources, mods, say)?;
    let model = extract_model(game, &sources, mods)?;
    let graph = graph_for(options)?;
    let bound = logic::Bound::new(&graph, &model);
    let mut units = items::units(&model, &bound, &sources.reg)?;
    items::kindling::split(&model, &mut units, options.kindling_shards).map_err(format_err)?;
    let keys = bound.key_items();
    let key_slots = units
        .iter()
        .filter(|u| u.item.name.as_deref().is_some_and(|n| keys.contains(n)))
        .count();
    say(&format!(
        "{} slots ({} sure), {} holding key items; model built in {:.1}s",
        units.len(),
        units.iter().filter(|u| u.sure()).count(),
        key_slots,
        t.elapsed().as_secs_f64()
    ));
    let t = Instant::now();
    let done = AtomicUsize::new(0);
    let failures: Vec<String> = (from..from + seeds)
        .into_par_iter()
        .filter_map(|seed| {
            let r = items::shuffle(&bound, &units, seed, options).err();
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(1000) {
                say(&format!("  {n} seeds"));
            }
            r
        })
        .collect();
    say(&format!(
        "{} seeds placed and proven in {:.1}s; {} failed",
        seeds,
        t.elapsed().as_secs_f64(),
        failures.len()
    ));
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shards_are_needed_of_count() {
        assert_eq!(shards_of("3/5"), Ok((3, 5)));
        assert_eq!(shards_of(" 20 / 20 "), Ok((20, 20)));
        for bad in ["0/5", "6/5", "1/21", "3", "a/b", "1/0"] {
            assert!(shards_of(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_bar_shows_the_new_name_with_the_old_title() {
        let t = titled;
        assert_eq!(
            t("Godrick the Grafted", "Malenia, Blade of Miquella"),
            "Godrick, Blade of Miquella"
        );
        assert_eq!(
            t("Rennala, Queen of the Full Moon", "Margit, the Fell Omen"),
            "Rennala, the Fell Omen"
        );
        assert_eq!(
            t("Crystalian", "Rykard, Lord of Blasphemy"),
            "Crystalian, Lord of Blasphemy"
        );
        assert_eq!(
            t("Radagon of the Golden Order", "Mohg, Lord of Blood"),
            "Radagon of the Golden Order, Lord of Blood"
        );
        assert_eq!(t("Godskin Apostle", "Tree Sentinel"), "Godskin Apostle");
        assert_eq!(
            t("Rennala, Queen of the Full Moon", "Tree Sentinel"),
            "Rennala, Queen of the Full Moon"
        );
        // A language that writes the title first, with no ", ": the whole new name.
        assert_eq!(
            t("接ぎ木のゴドリック", "ミケラの刃、マレニア"),
            "接ぎ木のゴドリック"
        );
    }

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
    fn a_profile_loads_the_seed_after_every_mod_and_carries_their_dlls() {
        let dir = std::env::temp_dir().join(format!("apoc-profile-{}", std::process::id()));
        let root = dir.join("Big Mod");
        let bin = root.join("mod/regulation.bin");
        std::fs::create_dir_all(root.join("mod/dll")).unwrap();
        std::fs::write(&bin, b"").unwrap();
        std::fs::write(root.join("mod/dll/here.dll"), b"").unwrap();
        std::fs::write(
            root.join("Big Mod.me3"),
            "[[natives]]\npath = \"mod/dll/here.dll\"\n[[natives]]\npath = \"mod/dll/gone.dll\"\n",
        )
        .unwrap();
        let out = dir.join("seed");
        write_profile(&out, std::slice::from_ref(&root), false, &|_| {}).unwrap();
        let text = std::fs::read_to_string(out.with_extension("me3")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let table: toml::Table = text.parse().unwrap();
        let packages = table["packages"].as_array().unwrap();
        assert_eq!(packages.len(), 2, "{text}");
        assert_eq!(table["savefile"].as_str(), Some("seed.sl2"));
        assert_eq!(packages[1]["path"].as_str(), Some("seed"));
        assert_eq!(packages[1]["load_after"][0]["id"].as_str(), Some("mod-0"));
        let natives = table["natives"].as_array().unwrap();
        assert_eq!(natives.len(), 1, "{text}");
        assert!(natives[0]["path"].as_str().unwrap().ends_with("here.dll"));
    }

    #[test]
    fn a_path_with_no_regulation_is_refused_by_name() {
        let e = mod_regulation(Path::new("/no/such/mod"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("/no/such/mod"), "{e}");
    }

    #[test]
    fn a_folder_inside_the_game_is_refused() {
        let dir = std::env::temp_dir().join(format!("apoc-in-game-{}", std::process::id()));
        let game = dir.join("game");
        std::fs::create_dir_all(&game).unwrap();
        assert!(not_in_game(&game, &game.join("mods/seed")).is_err());
        assert!(not_in_game(&game, &game).is_err());
        assert!(not_in_game(&game, &dir.join("seed")).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unknown_patch_is_refused() {
        assert!(known_patch("11730034").is_ok());
        let e = known_patch("11740000").unwrap_err().to_string();
        assert!(e.contains("11740000"), "{e}");
    }
}

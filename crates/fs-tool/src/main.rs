//! `apoc-fs`: the developer tool for the game's archives.
//!
//! It reads from a stock copy of the game (PLAN.md §5, Phase 0), never the live
//! install, and writes only where it is told to.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use clap::{Parser, Subcommand};
use fs_dvdbnd::Dvd;
use rayon::prelude::*;

mod boot_test;
mod check;
mod columns;
mod enemies;
mod flags;
mod gift_audit;
mod instructions;
mod labels;
mod logic;
mod model;
mod msb_columns;
mod names;
mod scripts;
mod test_kit;

#[derive(Parser)]
#[command(
    name = "apoc-fs",
    about = "Survey, extract and check the game's archives"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a list of archive paths from the game's own files, each one
    /// confirmed by a matching hash. The list is game data: never commit it.
    Names {
        game: PathBuf,
        /// Where to write the list.
        #[arg(long)]
        out: PathBuf,
        /// Where what was read from the game is kept between runs.
        #[arg(long, default_value = "scratch/names-cache")]
        cache: PathBuf,
        #[arg(long, default_value_t = 4)]
        jobs: usize,
    },
    /// Read every entry of every archive and report what is in them.
    Survey {
        /// The folder holding the archives and eldenring.exe.
        game: PathBuf,
        /// How many files to read at once. Each holds a whole file in memory.
        #[arg(long, default_value_t = 8)]
        jobs: usize,
    },
    /// The Phase 1 gate: every entry extracts, and every DCX and BND4 we write
    /// round-trips byte for byte.
    Check {
        game: PathBuf,
        /// A list of archive paths, one per line, to count names against.
        #[arg(long)]
        names: Option<PathBuf>,
        /// Skip recompressing every DCX, which is most of the time taken.
        #[arg(long)]
        no_recompress: bool,
        #[arg(long, default_value_t = 8)]
        jobs: usize,
    },
    /// Describe every table in regulation.bin: sizes, layout, and whether it
    /// rewrites byte for byte.
    Params {
        game: PathBuf,
        /// Write each table that differs, and our version, here.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// What a patch changed in regulation.bin: rows added, removed and
    /// changed in each table, and the offsets that changed.
    ParamDiff { old: PathBuf, new: PathBuf },
    /// Column statistics for one param table, for working out its layout.
    Columns {
        game: PathBuf,
        /// The table, e.g. `ItemLotParam_map`.
        table: String,
        /// Which widths to show: any of 4, 2 and 1 (bytes).
        #[arg(long, default_value = "4")]
        widths: String,
        #[arg(long, default_value_t = 0, value_parser = number)]
        from: usize,
        #[arg(long, value_parser = number)]
        to: Option<usize>,
        /// Keep only rows whose i32 at a hex offset has a value, e.g.
        /// `--where 20=1`. Repeatable.
        #[arg(long = "where")]
        filter: Vec<columns::Filter>,
    },
    /// For each small-valued column, every value with the names of rows that
    /// have it.
    Labels {
        game: PathBuf,
        table: String,
        /// The name table rows are named from, e.g. `GoodsName`.
        names: String,
        #[arg(long, default_value_t = 0, value_parser = number)]
        from: usize,
        #[arg(long, value_parser = number)]
        to: Option<usize>,
        /// Skip columns with more distinct values than this.
        #[arg(long, default_value_t = 12)]
        max_distinct: usize,
        /// How many row names to show for each value.
        #[arg(long, default_value_t = 6)]
        samples: usize,
    },
    /// How the param fields named as flags show up in the event scripts.
    Flags { game: PathBuf },
    /// Statistics for every instruction in the event scripts.
    Instructions {
        game: PathBuf,
        /// Where the decompressed scripts are cached between runs.
        #[arg(long, default_value = "scratch/emevd-cache")]
        cache: PathBuf,
        /// Only these instructions, as `bank:id,bank:id`.
        #[arg(long)]
        only: Option<String>,
    },
    /// Column statistics for one MSB table across every map.
    MsbColumns {
        game: PathBuf,
        /// e.g. `PARTS_PARAM_ST`.
        table: String,
        /// Keep only entries whose i32 at a hex offset has a value, e.g. `c=2`.
        #[arg(long = "type")]
        type_at: Option<columns::Filter>,
        #[arg(long, default_value_t = 0, value_parser = number)]
        from: usize,
        #[arg(long, value_parser = number)]
        to: Option<usize>,
        /// Measure from the section whose offset the entry holds here.
        #[arg(long, value_parser = number)]
        base: Option<usize>,
        #[arg(long, default_value = "scratch/emevd-cache")]
        cache: PathBuf,
    },
    /// The game model: every item location, enemy, boss and grace, with a
    /// coverage report by area and kind.
    Model {
        game: PathBuf,
        /// Write the whole model here as TOML, for reading through.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// How the placed enemies divide, and each boss as the scripts name it.
    Enemies { game: PathBuf },
    /// How NpcParam rows differ, word by word.
    NpcDiff { game: PathBuf, rows: Vec<i32> },
    /// How SpEffectParam rows differ, word by word.
    SpDiff { game: PathBuf, rows: Vec<i32> },
    /// Every event run in a map's script that names one of these entities.
    EntityEvents {
        game: PathBuf,
        map: String,
        entities: Vec<u32>,
    },
    /// A map's creature models as bytes.
    MsbModels { game: PathBuf, map: String },
    /// Every map entry holding one of these numbers, with the floats near its start.
    MsbFind {
        game: PathBuf,
        map: String,
        values: Vec<u32>,
    },
    /// What in a map lies near an entity, and what lies just beyond a distance.
    MsbNear {
        game: PathBuf,
        map: String,
        entity: u32,
        radius: f32,
    },
    /// Every placed enemy with its NpcParam row in hex, a line each.
    EnemyDump { game: PathBuf, out: PathBuf },
    /// Every command the scripts give each movable boss: map, spot, event,
    /// instruction and arguments, a line each.
    BossCommands { game: PathBuf, out: PathBuf },
    /// Every use of one instruction, with its arguments filled in.
    InstructionUses { game: PathBuf, bank: u32, id: u32 },
    /// The area graph checked against the game, and the vanilla game walked
    /// from the start.
    Logic { game: PathBuf },
    /// Read a local BND4 and write it back next to it as `<file>.rewritten`.
    Rewrite { file: PathBuf },
    /// Build a mod folder of files we write, for checking that the game
    /// accepts them (Phase 1, step 3).
    BootTest {
        game: PathBuf,
        /// The mod folder to write.
        out: PathBuf,
    },
    /// Give a mod folder's new characters 50 of every smithing stone and of
    /// Numen's Rune in the chapel, for testing upgrades by hand.
    TestKit {
        game: PathBuf,
        /// The mod folder to change.
        out: PathBuf,
    },
    /// Warp the player on every load of the chapel, to look at a far place in
    /// the lab. Never for a seed that is meant to be played as itself.
    TestWarp {
        game: PathBuf,
        /// The mod folder to change.
        out: PathBuf,
        /// The map to land in, as `60,42,36,0`.
        #[arg(long)]
        map: String,
        /// The entity to land on.
        #[arg(long)]
        entity: u32,
        /// The map whose loading sets it off, as `m60_42_36_00` (default the chapel).
        #[arg(long)]
        from: Option<String>,
    },
    /// Leave a mod folder's moved enemies with one health, for lab checks of what
    /// their death sets off. Never for a seed that is meant to be played.
    TestWeak {
        game: PathBuf,
        /// The mod folder to change.
        out: PathBuf,
    },
    /// Kill an entity some seconds after its map loads, for lab checks of what a
    /// death sets off. Never for a seed that is meant to be played.
    TestKill {
        game: PathBuf,
        /// The mod folder to change.
        out: PathBuf,
        /// The map, as `m10_00_00_00`.
        #[arg(long)]
        map: String,
        #[arg(long)]
        entity: u32,
        /// Seconds after loading.
        #[arg(long, default_value_t = 20.0)]
        after: f32,
        /// A mod folder whose copy of the map's script to build on.
        #[arg(long)]
        over: Option<PathBuf>,
        /// Stop the entity's AI every second instead of killing it.
        #[arg(long)]
        freeze: bool,
    },
    /// Every instruction in every map's script holding one of these numbers.
    ScriptFind { game: PathBuf, values: Vec<i32> },
    /// For each gift that may take a key item: what its talk script asks once
    /// failed conditions count, and what can turn on a flag it needs off.
    GiftAudit { game: PathBuf },
    /// Where flags appear, and what stands on each way an event turns them on.
    GiftBlockers { game: PathBuf, flags: Vec<u32> },
    /// Whole events, by id, and every use of instructions given as `bank:id`.
    ScriptDump {
        game: PathBuf,
        /// Event ids to print whole.
        #[arg(long, value_delimiter = ',')]
        events: Vec<i64>,
        /// Instructions to list, as `2003:14`.
        #[arg(long, value_delimiter = ',')]
        uses: Vec<String>,
    },
    /// Write one file out of the archives, by path or by hash.
    Extract {
        game: PathBuf,
        /// An archive path such as `/event/common.emevd.dcx`, or a hash as
        /// 16 hex digits.
        file: String,
        /// Where to write it.
        out: PathBuf,
        /// Decompress a DCX instead of writing it as stored.
        #[arg(long)]
        decompress: bool,
    },
}

/// A number in decimal or, with `0x`, hexadecimal.
fn number(s: &str) -> Result<usize, String> {
    match s.strip_prefix("0x") {
        Some(h) => usize::from_str_radix(h, 16),
        None => s.parse(),
    }
    .map_err(|e| e.to_string())
}

/// Print events whole, and each use of some instructions, with the raw bytes.
fn script_dump(game: PathBuf, events: &[i64], uses: &[String]) -> fs_bytes::Result<()> {
    let s = gamedata::Sources::open(&game)?;
    for (map, script) in &s.scripts {
        for event in &script.events {
            let whole = events.contains(&{ event.id });
            for i in &event.instructions {
                let key = format!("{}:{}", i.bank, i.id);
                if whole || uses.contains(&key) {
                    let words: Vec<i32> = i
                        .args
                        .chunks_exact(4)
                        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                        .collect();
                    println!(
                        "{map} event {} {}[{}] {:?} {words:?}",
                        event.id,
                        i.bank,
                        i.id,
                        &i.args[..i.args.len().min(4)]
                    );
                }
            }
        }
    }
    Ok(())
}

/// Which map, event and instruction hold these numbers as whole words.
fn script_find(game: PathBuf, values: &[i32]) -> fs_bytes::Result<()> {
    let s = gamedata::Sources::open(&game)?;
    let scripts = s.scripts.iter().map(|(m, e)| (m.to_string(), e)).chain([
        ("common".to_string(), &s.common),
        ("common_func".to_string(), &s.common_func),
    ]);
    for (map, script) in scripts {
        for event in &script.events {
            for i in &event.instructions {
                let words: Vec<i32> = i
                    .args
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                    .collect();
                if words.iter().any(|w| values.contains(w)) {
                    println!("{map} event {} {}[{}] {words:?}", event.id, i.bank, i.id);
                }
            }
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Survey { game, jobs } => survey(game, jobs),
        Command::Names {
            game,
            out,
            cache,
            jobs,
        } => names::run(game, out, cache, jobs),
        Command::Check {
            game,
            names,
            no_recompress,
            jobs,
        } => check::run(game, names, !no_recompress, jobs),
        Command::Rewrite { file } => rewrite(file),
        Command::BootTest { game, out } => boot_test::run(game, out),
        Command::TestKit { game, out } => test_kit::run(game, out),
        Command::Model { game, dump } => model::run(game, dump),
        Command::Logic { game } => logic::run(game),
        Command::Enemies { game } => enemies::run(game),
        Command::NpcDiff { game, rows } => enemies::diff(game, rows),
        Command::SpDiff { game, rows } => enemies::sp_diff(game, rows),
        Command::EntityEvents {
            game,
            map,
            entities,
        } => enemies::events(game, map, entities),
        Command::MsbModels { game, map } => enemies::models(game, map),
        Command::MsbFind { game, map, values } => enemies::find(game, map, values),
        Command::MsbNear {
            game,
            map,
            entity,
            radius,
        } => enemies::near(game, map, entity, radius),
        Command::EnemyDump { game, out } => enemies::dump(game, out),
        Command::BossCommands { game, out } => enemies::boss_commands(game, out),
        Command::InstructionUses { game, bank, id } => enemies::instruction_uses(game, bank, id),
        Command::Flags { game } => flags::run(game),
        Command::MsbColumns {
            game,
            table,
            type_at,
            from,
            to,
            base,
            cache,
        } => msb_columns::run(
            game,
            cache,
            msb_columns::Query {
                table,
                type_at: type_at.map(|f| (f.off, f.value)),
                from,
                to,
                base,
            },
        ),
        Command::Instructions { game, cache, only } => instructions::run(game, cache, only),
        Command::Labels {
            game,
            table,
            names,
            from,
            to,
            max_distinct,
            samples,
        } => labels::run(game, &table, &names, from, to, max_distinct, samples),
        Command::Params { game, dump } => params(game, dump),
        Command::ParamDiff { old, new } => param_diff(old, new),
        Command::Columns {
            game,
            table,
            widths,
            from,
            to,
            filter,
        } => columns::run(game, &table, &widths, from, to, &filter),
        Command::TestKill {
            game,
            out,
            map,
            entity,
            after,
            over,
            freeze,
        } => test_kit::kill(game, out, &map, entity, after, over, freeze),
        Command::ScriptFind { game, values } => script_find(game, &values),
        Command::GiftAudit { game } => gift_audit::run(game),
        Command::GiftBlockers { game, flags } => gift_audit::blockers(game, &flags),
        Command::ScriptDump { game, events, uses } => script_dump(game, &events, &uses),
        Command::TestWeak { game, out } => test_kit::weak_enemies(game, out),
        Command::TestWarp {
            game,
            out,
            map,
            entity,
            from,
        } => test_kit::warp(game, out, &map, entity, from),
        Command::Extract {
            game,
            file,
            out,
            decompress,
        } => extract(game, &file, out, decompress),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn extract(game: PathBuf, file: &str, out: PathBuf, decompress: bool) -> fs_bytes::Result<()> {
    let dvd = Dvd::open_elden_ring(&game)?;
    let hash = match u64::from_str_radix(file, 16) {
        Ok(h) if file.len() == 16 => h,
        _ => fs_dvdbnd::hash_path(file),
    };
    let r = dvd
        .find_hash(hash)
        .ok_or_else(|| fs_bytes::format_err(format!("{file} ({hash:016x}) is in no archive")))?;
    let mut data = dvd.read(r)?;
    if decompress && fs_dcx::is_dcx(&data) {
        data = fs_dcx::decompress(&data)?.1;
    }
    std::fs::write(&out, &data)?;
    eprintln!(
        "{} bytes from {} to {}",
        data.len(),
        dvd.archives[r.archive].name,
        out.display()
    );
    Ok(())
}

fn rewrite(file: PathBuf) -> fs_bytes::Result<()> {
    let bytes = std::fs::read(&file)?;
    let written = fs_bnd4::Bnd4::read(&bytes)?.write()?;
    let mut out = file.into_os_string();
    out.push(".rewritten");
    std::fs::write(&out, &written)?;
    eprintln!(
        "{} bytes in, {} out, identical: {}",
        bytes.len(),
        written.len(),
        bytes == written
    );
    Ok(())
}

fn param_diff(old: PathBuf, new: PathBuf) -> fs_bytes::Result<()> {
    let old = fs_param::Regulation::open(&old)?;
    let new = fs_param::Regulation::open(&new)?;
    println!("regulation {} -> {}", old.version(), new.version());
    let names: Vec<String> = new.table_names().map(str::to_owned).collect();
    for name in &names {
        let (a, b) = (old.table(name)?, new.table(name)?);
        let before: std::collections::BTreeMap<i32, &[u8]> =
            a.rows.iter().map(|r| (r.id, r.data.as_slice())).collect();
        let after: std::collections::BTreeMap<i32, &[u8]> =
            b.rows.iter().map(|r| (r.id, r.data.as_slice())).collect();
        let added = after.keys().filter(|id| !before.contains_key(id)).count();
        let removed = before.keys().filter(|id| !after.contains_key(id)).count();
        let mut changed = 0;
        let mut offsets = std::collections::BTreeMap::<usize, usize>::new();
        for (id, row) in &after {
            let Some(was) = before.get(id).filter(|was| *was != row) else {
                continue;
            };
            changed += 1;
            for (o, (x, y)) in was.iter().zip(row.iter()).enumerate() {
                if x != y {
                    *offsets.entry(o).or_default() += 1;
                }
            }
        }
        if added + removed + changed > 0 {
            println!("{name}: +{added} -{removed} ~{changed} at offsets {offsets:?}");
        }
    }
    let gone = old.table_names().filter(|n| !names.iter().any(|m| m == n));
    for name in gone {
        println!("{name}: table removed");
    }
    Ok(())
}

fn params(game: PathBuf, dump: Option<PathBuf>) -> fs_bytes::Result<()> {
    let t = Instant::now();
    let reg = fs_param::Regulation::open(&game)?;
    println!(
        "regulation {} opened in {:.1?}, {} tables",
        reg.version(),
        t.elapsed(),
        reg.bnd.files.len()
    );
    let mut same = 0;
    for f in &reg.bnd.files {
        let name = f.short_name().unwrap_or("?");
        let p = match fs_param::Param::read(&f.data) {
            Ok(p) => p,
            Err(e) => {
                println!("{name}: READ ERROR {e}");
                continue;
            }
        };
        let w = p.write()?;
        let identical = w == f.data;
        if let (Some(dir), false) = (&dump, identical) {
            std::fs::write(dir.join(name), &f.data)?;
            std::fs::write(dir.join(format!("{name}.ours")), &w)?;
        }
        same += usize::from(identical);
        let b = &f.data;
        let u32at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let i64at = |o: usize| i64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let named = p
            .rows
            .iter()
            .filter(|r| r.name.as_ref().is_some_and(|n| !n.is_empty()))
            .count();
        println!(
            "{name:40} {:28} rows {:6} size {:5} named {:6} strings@{:#x} type@{:#x} len {:#x} hdr04 {:02x?} 2c..30 {:02x?} {}",
            String::from_utf8_lossy(&p.type_name),
            p.rows.len(),
            p.row_size,
            named,
            u32at(0),
            i64at(0x10),
            b.len(),
            &b[4..0x0A],
            &b[0x2C..0x30],
            if identical {
                "SAME".to_string()
            } else {
                let at = w
                    .iter()
                    .zip(b.iter())
                    .position(|(x, y)| x != y)
                    .unwrap_or(w.len().min(b.len()));
                format!("DIFF at {at:#x} (ours {:#x} long)", w.len())
            }
        );
    }
    println!("{same}/{} rewrite byte for byte", reg.bnd.files.len());
    Ok(())
}

/// What one entry turned out to be.
#[derive(Default)]
struct Tally {
    files: u64,
    stored_bytes: u64,
    plain_bytes: u64,
    /// The outer format: a DCX compression, or the file's own magic.
    outer: BTreeMap<String, u64>,
    /// The format inside a DCX.
    inner: BTreeMap<String, u64>,
    /// Distinct DCX headers, with the size fields blanked, and how many share
    /// each.
    dcx_headers: BTreeMap<String, u64>,
    largest: (u64, String),
    errors: Vec<String>,
}

fn magic(b: &[u8]) -> String {
    let m = &b[..b.len().min(4)];
    if m.iter().all(|c| c.is_ascii_graphic() || *c == b' ') && !m.is_empty() {
        String::from_utf8_lossy(m).into_owned()
    } else {
        format!(
            "0x{}",
            m.iter().map(|c| format!("{c:02x}")).collect::<String>()
        )
    }
}

fn survey(game: PathBuf, jobs: usize) -> fs_bytes::Result<()> {
    let start = Instant::now();
    let dvd = Dvd::open_elden_ring(&game)?;
    eprintln!(
        "opened {} archives, {} entries, in {:.1?}",
        dvd.archives.len(),
        dvd.len(),
        start.elapsed()
    );
    for a in &dvd.archives {
        eprintln!(
            "  {:6} {:6} entries  salt {:?}  flag {}",
            a.name,
            a.bhd.entries.len(),
            a.bhd.salt,
            a.bhd.flag
        );
    }
    eprintln!(
        "  duplicate hashes across archives: {}",
        dvd.duplicates.len()
    );

    let tally = Mutex::new(Tally::default());
    let refs: Vec<_> = dvd.entries().map(|(r, _)| r).collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .unwrap();
    pool.install(|| {
        refs.par_iter().for_each(|&r| {
            let entry = dvd.entry(r);
            let name = format!("{}:{:016x}", dvd.archives[r.archive].name, entry.hash);
            let outcome = dvd.read(r).and_then(|data| {
                if fs_dcx::is_dcx(&data) {
                    let (h, plain) = fs_dcx::decompress(&data)?;
                    let mut fp = *h.raw();
                    fp[0x1C..0x24].fill(0);
                    let fp: String = fp.iter().map(|c| format!("{c:02x}")).collect();
                    Ok((
                        data.len(),
                        Some((
                            format!("DCX/{:?}", h.algorithm),
                            fp,
                            magic(&plain),
                            plain.len(),
                        )),
                    ))
                } else {
                    Ok((data.len(), None))
                }
            });
            let mut t = tally.lock().unwrap();
            t.files += 1;
            match outcome {
                Ok((stored, dcx)) => {
                    t.stored_bytes += stored as u64;
                    let plain_len = match dcx {
                        Some((outer, fp, inner, len)) => {
                            *t.outer.entry(outer).or_default() += 1;
                            *t.dcx_headers.entry(fp).or_default() += 1;
                            *t.inner.entry(inner).or_default() += 1;
                            len as u64
                        }
                        None => {
                            // Read again only for the magic; cheap next to the rest.
                            let m = dvd.read(r).map(|d| magic(&d)).unwrap_or_default();
                            *t.outer.entry(m).or_default() += 1;
                            stored as u64
                        }
                    };
                    t.plain_bytes += plain_len;
                    if plain_len > t.largest.0 {
                        t.largest = (plain_len, name);
                    }
                }
                Err(e) => t.errors.push(format!("{name}: {e}")),
            }
            drop(t);
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n.is_multiple_of(5000) {
                eprintln!("  {n}/{} read, {:.0?}", refs.len(), start.elapsed());
            }
        })
    });

    let t = tally.into_inner().unwrap();
    println!(
        "files read: {}  errors: {}  in {:.1?}",
        t.files,
        t.errors.len(),
        start.elapsed()
    );
    println!(
        "stored bytes: {}  uncompressed bytes: {}",
        t.stored_bytes, t.plain_bytes
    );
    println!(
        "largest uncompressed: {} bytes ({})",
        t.largest.0, t.largest.1
    );
    println!("\nouter formats:");
    for (k, v) in &t.outer {
        println!("  {v:7}  {k}");
    }
    println!("\nformats inside DCX:");
    for (k, v) in &t.inner {
        println!("  {v:7}  {k}");
    }
    println!("\nDCX headers (sizes blanked):");
    for (k, v) in &t.dcx_headers {
        println!("  {v:7}  {k}");
    }
    for e in t.errors.iter().take(50) {
        println!("ERROR {e}");
    }
    Ok(())
}

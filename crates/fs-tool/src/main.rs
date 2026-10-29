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

mod check;
mod columns;
mod flags;
mod labels;
mod names;

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
    /// Column statistics for one param table, for working out its layout.
    Columns {
        game: PathBuf,
        /// The table, e.g. `ItemLotParam_map`.
        table: String,
        /// Which widths to show: any of 4, 2 and 1 (bytes).
        #[arg(long, default_value = "4")]
        widths: String,
        #[arg(long, default_value_t = 0)]
        from: usize,
        #[arg(long)]
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
        #[arg(long, default_value_t = 0)]
        from: usize,
        #[arg(long)]
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
    /// Read a local BND4 and write it back next to it as `<file>.rewritten`.
    Rewrite { file: PathBuf },
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

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Survey { game, jobs } => survey(game, jobs),
        Command::Check {
            game,
            names,
            no_recompress,
            jobs,
        } => check::run(game, names, !no_recompress, jobs),
        Command::Rewrite { file } => rewrite(file),
        Command::Flags { game } => flags::run(game),
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
        Command::Columns {
            game,
            table,
            widths,
            from,
            to,
            filter,
        } => columns::run(game, &table, &widths, from, to, &filter),
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

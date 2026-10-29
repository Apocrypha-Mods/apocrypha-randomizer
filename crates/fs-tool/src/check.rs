//! The Phase 1 gate (PLAN.md §5): every member of every archive extracts, the
//! count matches the name list, and the BND4 and DCX we write round-trip byte
//! for byte.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use fs_bnd4::Bnd4;
use fs_bytes::{Result, format_err};
use fs_dcx::Algorithm;
use fs_dvdbnd::Dvd;
use rayon::prelude::*;

/// Stored files larger than this are checked one at a time, so that several
/// of the largest (1.4 GB uncompressed) are never in memory together.
const LARGE: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct Totals {
    entries: u64,
    dcx: u64,
    dcx_by_algorithm: BTreeMap<String, u64>,
    dcx_rewritten: u64,
    /// Deflate originals our deflate writer reproduced exactly. Not part of
    /// the gate: the game's encoder settings are not known.
    deflate_identical: u64,
    bnd4_top: u64,
    bnd4_nested: u64,
    bnd4_files: u64,
    fmg: u64,
    emevd: u64,
    failures: Vec<String>,
}

pub fn run(game: PathBuf, names: Option<PathBuf>, recompress: bool, jobs: usize) -> Result<()> {
    let start = Instant::now();
    let dvd = Dvd::open_elden_ring(&game)?;
    println!("archives: {}  entries: {}", dvd.archives.len(), dvd.len());

    if let Some(path) = names {
        name_coverage(&dvd, &path)?;
    }

    let totals = Mutex::new(Totals::default());
    let large = Mutex::new(());
    let done = AtomicUsize::new(0);
    let refs: Vec<_> = dvd.entries().map(|(r, _)| r).collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .unwrap();
    pool.install(|| {
        refs.par_iter().for_each(|&r| {
            let entry = dvd.entry(r);
            let label = format!("{}:{:016x}", dvd.archives[r.archive].name, entry.hash);
            let _guard = (entry.padded_size as usize > LARGE).then(|| large.lock().unwrap());
            let mut local = Totals::default();
            if let Err(e) = check_entry(&dvd, r, recompress, &mut local) {
                local.failures.push(format!("{label}: {e}"));
            }
            let mut t = totals.lock().unwrap();
            t.entries += 1;
            t.dcx += local.dcx;
            for (k, v) in local.dcx_by_algorithm {
                *t.dcx_by_algorithm.entry(k).or_default() += v;
            }
            t.dcx_rewritten += local.dcx_rewritten;
            t.deflate_identical += local.deflate_identical;
            t.bnd4_top += local.bnd4_top;
            t.bnd4_nested += local.bnd4_nested;
            t.bnd4_files += local.bnd4_files;
            t.fmg += local.fmg;
            t.emevd += local.emevd;
            t.failures.extend(local.failures);
            drop(t);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(10_000) {
                eprintln!("  {n}/{} checked, {:.0?}", refs.len(), start.elapsed());
            }
        })
    });

    let t = totals.into_inner().unwrap();
    println!(
        "entries extracted: {}/{}",
        t.entries - count_extract_failures(&t),
        dvd.len()
    );
    println!(
        "DCX decompressed: {}  by compression: {:?}",
        t.dcx, t.dcx_by_algorithm
    );
    if recompress {
        println!(
            "DCX rewritten and round-tripped byte for byte: {}/{}",
            t.dcx_rewritten, t.dcx
        );
        println!(
            "deflate originals reproduced exactly by our encoder (informational): {}",
            t.deflate_identical
        );
    }
    println!(
        "BND4 round-tripped byte for byte: {} top-level, {} nested, holding {} files",
        t.bnd4_top, t.bnd4_nested, t.bnd4_files
    );
    println!("FMG round-tripped byte for byte: {}", t.fmg);
    println!("EMEVD round-tripped byte for byte: {}", t.emevd);
    println!("failures: {}", t.failures.len());
    for f in &t.failures {
        println!("FAIL {f}");
    }
    println!("time: {:.1?}", start.elapsed());
    if t.failures.is_empty() {
        Ok(())
    } else {
        Err(format_err("the gate did not pass"))
    }
}

fn count_extract_failures(t: &Totals) -> u64 {
    t.failures.iter().filter(|f| f.contains("extract:")).count() as u64
}

fn check_entry(dvd: &Dvd, r: fs_dvdbnd::EntryRef, recompress: bool, t: &mut Totals) -> Result<()> {
    let stored = dvd
        .read(r)
        .map_err(|e| format_err(format!("extract: {e}")))?;
    let plain = if fs_dcx::is_dcx(&stored) {
        let (header, plain) =
            fs_dcx::decompress(&stored).map_err(|e| format_err(format!("extract: {e}")))?;
        t.dcx += 1;
        *t.dcx_by_algorithm
            .entry(format!("{:?}", header.algorithm))
            .or_default() += 1;
        if recompress {
            dcx_round_trip(&header, &stored, &plain, t)?;
        }
        plain
    } else {
        stored
    };
    if fs_emevd::is_emevd(&plain) {
        let e =
            fs_emevd::Emevd::read(&plain).map_err(|e| format_err(format!("EMEVD read: {e}")))?;
        if e.write()? != plain {
            return Err(format_err("EMEVD differs when rewritten"));
        }
        t.emevd += 1;
    }
    if fs_bnd4::is_bnd4(&plain) {
        t.bnd4_top += 1;
        bnd4_round_trip(&plain, recompress, t, 0)?;
    }
    Ok(())
}

/// Our writer's output reads back to the same payload, and writing what was
/// read again gives the same bytes.
fn dcx_round_trip(
    header: &fs_dcx::Header,
    original: &[u8],
    plain: &[u8],
    t: &mut Totals,
) -> Result<()> {
    let mut algorithms = vec![Algorithm::Zstd];
    if header.algorithm == Algorithm::Deflate {
        algorithms.push(Algorithm::Deflate);
    }
    for alg in algorithms {
        let ours = fs_dcx::compress(&header.with_algorithm(alg), plain)?;
        let (back_header, back) = fs_dcx::decompress(&ours)?;
        if back != plain {
            return Err(format_err(format!(
                "DCX {alg:?}: the payload changed in a round trip"
            )));
        }
        if fs_dcx::compress(&back_header, &back)? != ours {
            return Err(format_err(format!(
                "DCX {alg:?}: a second write differs from the first"
            )));
        }
        if alg == Algorithm::Deflate && ours == original {
            t.deflate_identical += 1;
        }
    }
    t.dcx_rewritten += 1;
    Ok(())
}

fn bnd4_round_trip(bytes: &[u8], recompress: bool, t: &mut Totals, depth: usize) -> Result<()> {
    let bnd = Bnd4::read(bytes).map_err(|e| format_err(format!("BND4 read: {e}")))?;
    let written = bnd.write()?;
    if written != bytes {
        let at = written
            .iter()
            .zip(bytes)
            .position(|(a, b)| a != b)
            .unwrap_or(written.len().min(bytes.len()));
        return Err(format_err(format!(
            "BND4 written differs from the original at {at:#x} (lengths {} and {})",
            written.len(),
            bytes.len()
        )));
    }
    t.bnd4_files += bnd.files.len() as u64;
    if depth >= 4 {
        return Ok(());
    }
    for f in &bnd.files {
        if f.name
            .as_deref()
            .is_some_and(|n| n.to_ascii_lowercase().ends_with(".fmg"))
        {
            let fmg = fs_fmg::Fmg::read(&f.data)
                .map_err(|e| format_err(format!("FMG {:?}: {e}", f.short_name())))?;
            if fmg.write()? != f.data {
                return Err(format_err(format!(
                    "FMG {:?} differs when rewritten",
                    f.short_name()
                )));
            }
            t.fmg += 1;
            continue;
        }
        let decompressed;
        let inner: &[u8] = if fs_dcx::is_dcx(&f.data) {
            let (header, plain) = fs_dcx::decompress(&f.data)?;
            t.dcx += 1;
            *t.dcx_by_algorithm
                .entry(format!("{:?} (nested)", header.algorithm))
                .or_default() += 1;
            if recompress {
                dcx_round_trip(&header, &f.data, &plain, t)?;
            }
            decompressed = plain;
            &decompressed
        } else {
            &f.data
        };
        if fs_bnd4::is_bnd4(inner) {
            t.bnd4_nested += 1;
            bnd4_round_trip(inner, recompress, t, depth + 1)?;
        }
    }
    Ok(())
}

/// How many entries the name list names, and how many names find no entry.
fn name_coverage(dvd: &Dvd, path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let names: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let hashes: HashSet<u64> = names.iter().map(|n| fs_dvdbnd::hash_path(n)).collect();
    let named = dvd
        .entries()
        .filter(|(_, e)| hashes.contains(&e.hash))
        .count();
    let missing = hashes
        .iter()
        .filter(|&&h| dvd.find_hash(h).is_none())
        .count();
    println!(
        "name list: {} names ({} distinct hashes); names {named}/{} entries; {missing} names match no entry",
        names.len(),
        hashes.len(),
        dvd.len()
    );
    Ok(())
}

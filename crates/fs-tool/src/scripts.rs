//! Every event script in the game, decompressed, with a local cache.
//!
//! Finding the 589 scripts means decompressing all 120,408 entries, over a
//! minute, so the first run writes them to a cache folder (inside `scratch/`,
//! never committed) and later runs read them from there.

use std::path::Path;

use fs_bytes::Result;
use fs_dvdbnd::Dvd;
use fs_emevd::Emevd;
use rayon::prelude::*;

/// Each script's label (`Data0:<hash>`) and its contents.
pub fn load(game: &Path, cache: &Path) -> Result<Vec<(String, Emevd)>> {
    let mut files: Vec<_> = std::fs::read_dir(cache)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "emevd"))
                .collect()
        })
        .unwrap_or_else(|_| Vec::new());
    if files.is_empty() {
        std::fs::create_dir_all(cache)?;
        let dvd = Dvd::open_elden_ring(game)?;
        let refs: Vec<_> = dvd.entries().map(|(r, _)| r).collect();
        refs.par_iter().try_for_each(|&r| -> Result<()> {
            let Ok(stored) = dvd.read(r) else {
                return Ok(());
            };
            if !fs_dcx::is_dcx(&stored) {
                return Ok(());
            }
            let Ok((_, plain)) = fs_dcx::decompress(&stored) else {
                return Ok(());
            };
            if fs_emevd::is_emevd(&plain) {
                let name = format!(
                    "{}_{:016x}.emevd",
                    dvd.archives[r.archive].name,
                    dvd.entry(r).hash
                );
                std::fs::write(cache.join(name), &plain)?;
            }
            Ok(())
        })?;
        files = std::fs::read_dir(cache)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
    }
    files.sort();
    files
        .par_iter()
        .map(|p| {
            let label = p
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .replacen('_', ":", 1);
            Ok((label, Emevd::read(&std::fs::read(p)?)?))
        })
        .collect()
}

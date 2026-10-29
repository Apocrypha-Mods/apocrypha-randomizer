//! `apoc-rando`: build a randomized mod folder from a seed, or prove that
//! many seeds can each be finished (the Phase 4 gate).

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use clap::{Parser, Subcommand};
use fs_bytes::{Result, format_err};
use gamedata::{Sources, elden_ring};
use rayon::prelude::*;

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
    },
    /// Place items for many seeds and prove each can be finished.
    Check {
        game: PathBuf,
        /// How many seeds, from `--from` on.
        #[arg(long, default_value_t = 10_000)]
        seeds: u64,
        #[arg(long, default_value_t = 0)]
        from: u64,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Gen { game, seed, out } => gen_seed(game, seed, out),
        Command::Check { game, seeds, from } => check(game, from, seeds),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn gen_seed(game: PathBuf, seed: u64, out: PathBuf) -> Result<()> {
    let sources = Sources::open(&game)?;
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
    let plan = items::shuffle(&bound, &units, seed).map_err(format_err)?;
    let mut reg = fs_param::Regulation::open(&game)?;
    items::apply(&mut reg, &units, &plan)?;
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("regulation.bin"), reg.write()?)?;
    let spoiler = out.with_extension("spoiler.txt");
    std::fs::write(&spoiler, items::spoiler(&bound, &units, &plan))?;
    eprintln!(
        "seed {seed}: {} slots shuffled; mod folder {}, spoiler log {}",
        units.len(),
        out.display(),
        spoiler.display()
    );
    Ok(())
}

fn check(game: PathBuf, from: u64, seeds: u64) -> Result<()> {
    let t = Instant::now();
    let sources = Sources::open(&game)?;
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
            let r = items::shuffle(&bound, &units, seed).err();
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

//! `apoc-rando`: build a randomized mod folder from a seed, or prove that
//! many seeds can each be finished (the Phase 4 gate).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fs_bytes::{Result, format_err};
use rando::{EnemyOptions, ItemOptions, Request};

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
    /// (default 0): `--bias keys=60,weapons=30`. Item categories and gathering
    /// only: merchants, starting gear, keepsakes and NPC gear are refused, as
    /// what lands there leans by its own category.
    #[arg(long, default_value = "")]
    bias: String,
    /// Keep base-game and DLC items on their own side (`separate`) or
    /// shuffle them together (`mixed`, the default).
    #[arg(long, default_value = "mixed", value_parser = ["mixed", "separate"])]
    dlc: String,
    /// Whether enemies stay on their own side of the base game and the DLC, apart
    /// from `--dlc`, which is for items. Mixed may crash the game.
    #[arg(long, default_value = "separate", value_parser = ["mixed", "separate"])]
    enemy_dlc: String,
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
    /// randomized and proven on the merged tables, and on the mods' own maps,
    /// scripts, talk and text. A me3 profile beside the seed loads them all.
    #[arg(long = "with-mod", value_name = "PATH")]
    with_mods: Vec<PathBuf>,
    /// How many of Godrick's, Radahn's, Rykard's, Mohg's and Malenia's Great Runes,
    /// each restored or not, and Rennala's Great Rune of the Unborn, open Leyndell's gate, 0 to 4 (default 2, as the game). The
    /// seed's script counts them, and the proof asks the same. The gate also waits on
    /// the Roundtable Hold having been entered, as the game's does.
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(0..=4))]
    leyndell_runes: u8,
    /// Leave the Whetstone Knife and the whetblades where the game has them.
    #[arg(long)]
    keep_whetblades: bool,
    /// Leave map fragments where the game has them.
    #[arg(long)]
    keep_maps: bool,
    /// Draw every piece of the world map from the start, as if every map fragment
    /// were held. Graces still show only once touched.
    #[arg(long)]
    whole_map: bool,
    /// Night-only bosses at any hour: Night's Cavalry and their horses, for now.
    /// Not yet seen in the game.
    #[arg(long)]
    night_bosses_always: bool,
    /// Give each class starting gear it can use: weapons its stats can wield
    /// (two-handed if need be, a bow only with its arrows), and spells it has the
    /// catalyst, the stats and the memory for. Without it, starting gear is random.
    #[arg(long)]
    usable_start: bool,
}

impl Sliders {
    fn options(&self) -> Result<ItemOptions> {
        let mut o = ItemOptions::default();
        o.set(false, &self.shuffle).map_err(format_err)?;
        o.set(true, &self.bias).map_err(format_err)?;
        o.dlc_separate = self.dlc == "separate";
        o.auto_upgrade = self.auto_upgrade;
        o.keep_whetblades = self.keep_whetblades;
        o.keep_maps = self.keep_maps;
        o.whole_map = self.whole_map;
        o.night_bosses_always = self.night_bosses_always;
        o.usable_start = self.usable_start;
        o.set_leyndell_runes(self.leyndell_runes)
            .map_err(format_err)?;
        o.enemies_move = !self.enemy_options()?.is_off();
        Ok(o)
    }

    fn enemy_options(&self) -> Result<EnemyOptions> {
        let mut o = EnemyOptions::default();
        o.set(&self.enemies).map_err(format_err)?;
        o.scale = !self.no_enemy_scaling;
        o.dlc_separate = self.enemy_dlc == "separate";
        Ok(o)
    }
}

/// A run's messages go to standard error.
fn say(line: &str) {
    eprintln!("{line}");
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Gen {
            game,
            seed,
            out,
            sliders,
        } => {
            let done = rando::generate(
                &Request {
                    game,
                    seed,
                    out,
                    options: sliders.options()?,
                    enemies: sliders.enemy_options()?,
                    native: sliders.native,
                    mods: sliders.with_mods,
                },
                &say,
            )?;
            say(&format!(
                "seed {seed}: {} slots shuffled; mod folder {}, spoiler log {}",
                done.slots,
                done.folder.display(),
                done.spoiler.display()
            ));
            Ok(())
        }
        Command::Check {
            game,
            seeds,
            from,
            sliders,
        } => {
            let options = sliders.options()?;
            let failures = rando::check(&game, from, seeds, &options, &sliders.with_mods, &say)?;
            for f in failures.iter().take(20) {
                say(&format!("  {f}"));
            }
            if failures.is_empty() {
                Ok(())
            } else {
                Err(format_err(format!("{} seeds failed", failures.len())))
            }
        }
    }
}

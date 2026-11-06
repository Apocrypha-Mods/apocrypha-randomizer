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
        /// The seed; needed unless `--no-spoiler`, which draws one of its own.
        #[arg(
            long,
            required_unless_present = "no_spoiler",
            conflicts_with = "no_spoiler"
        )]
        seed: Option<u64>,
        /// A hidden seed: one is drawn at random and never shown, and no spoiler log is
        /// written (one left by an earlier seed beside the mod folder is removed).
        #[arg(long)]
        no_spoiler: bool,
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
    /// One enemy on every spot, bosses' included: a name as its health bar shows it
    /// ("Margit, the Fell Omen", or the start of one, "margit"), or `random`. The
    /// enemy sliders then do nothing; with `--enemy-dlc separate` only the chosen
    /// enemy's side of the game changes.
    #[arg(long, value_name = "NAME")]
    oops_all: Option<String>,
    /// Onslaught: how many of each ordinary enemy stand at its spot, 2 to 10 (1, the
    /// default, as the game has it). Only enemies no script names are copied; bosses
    /// stay one. Past 3 the game may slow or crash.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=10))]
    onslaught: u8,
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
    /// Lamenter's Gaol's two keys move with the rest, and the gaol asks for both. Without
    /// it they stay where the game has them.
    #[arg(long)]
    gaol_keys: bool,
    /// DLC Start: a new character is taken from the Chapel of Anticipation to the
    /// Gravesite Plain, given Torrent's whistle and the Spirit Calling Bell, and the seed
    /// is proven from there to the DLC's last boss.
    #[arg(long)]
    dlc_start: bool,
    /// With DLC Start: Lord's Runes enough to take a level 1 character to this level,
    /// 2 to 200 (0, the default, for none).
    #[arg(long, value_name = "LEVEL", default_value_t = 0, requires = "dlc_start",
          value_parser = clap::value_parser!(u8).range(0..=200))]
    dlc_start_level: u8,
    /// With DLC Start: every Golden Seed and Sacred Tear, the three talisman pouches,
    /// the eight memory stones and the Flask of Wondrous Physick.
    #[arg(long, requires = "dlc_start")]
    dlc_start_flasks: bool,
    /// With DLC Start: every smithing bell bearing, the Whetstone Knife and every
    /// whetblade.
    #[arg(long, requires = "dlc_start")]
    dlc_start_smithing: bool,
    /// With DLC Start: every weapon, armour piece, talisman, spell, Ash of War and
    /// spirit ash of the base game, for nothing, from any Site of Grace's menu.
    #[arg(long, requires = "dlc_start")]
    dlc_start_shop: bool,
    /// The gestures the event scripts give change places (O Mother, which opens the
    /// Shadow Keep's statue, stays).
    #[arg(long)]
    random_gestures: bool,
    /// Messmer's Kindling in shards, as NEEDED/COUNT: `--kindling-shards 3/5` puts five
    /// shards in the world, any three of which burn the Sealing Tree. 1 to 20 shards.
    #[arg(long, value_name = "NEEDED/COUNT")]
    kindling_shards: Option<String>,
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
    /// Night-only bosses at any hour: Night's Cavalry and the four shacks' Bell
    /// Bearing Hunters. Deathbirds keep to the night. Not yet seen in the game.
    #[arg(long)]
    night_bosses_always: bool,
    /// Give each class starting gear it can use: weapons its stats can wield
    /// (two-handed if need be, a bow only with its arrows), and spells it has the
    /// catalyst, the stats and the memory for. Without it, starting gear is random.
    #[arg(long)]
    usable_start: bool,
    /// With usable starting gear: each class's weapons, armour and talismans also
    /// weigh no more than a medium roll allows (70% of its equip load).
    #[arg(long, requires = "usable_start")]
    light_start: bool,
    /// Know every crafting recipe from the start, cookbooks or not.
    #[arg(long)]
    all_recipes: bool,
    /// Each regular upgrade costs one Smithing Stone.
    #[arg(long)]
    cheap_upgrades: bool,
    /// Weapons and spells ask no stats.
    #[arg(long)]
    no_requirements: bool,
    /// A moved boss pays the runes it pays at home, and whoever takes its arena pays
    /// its own.
    #[arg(long)]
    boss_runes_follow: bool,
    /// Malenia no longer heals when she hits.
    #[arg(long)]
    no_malenia_heal: bool,
    /// The poison clouds the Valiant Gargoyles' breath leaves behind no longer hurt.
    #[arg(long)]
    no_gargoyle_poison: bool,
    /// Torrent can be called in the Abyssal Woods.
    #[arg(long)]
    torrent_woods: bool,
    /// Each boss arena plays another boss's theme.
    #[arg(long)]
    boss_music: bool,
    /// The music of the base game's areas changes places among them.
    #[arg(long)]
    area_music: bool,
    /// Each kind of ordinary enemy is drawn in a colour of the seed's.
    #[arg(long)]
    enemy_colours: bool,
    /// Fog gates: boss fogs and dungeon entrances lead somewhere else, and the seed is
    /// proven finishable through them. Works with the item sliders, or alone with
    /// `--shuffle all=0`.
    #[arg(long)]
    fog_gates: bool,
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
        o.light_start = self.light_start;
        o.all_recipes = self.all_recipes;
        o.cheap_upgrades = self.cheap_upgrades;
        o.no_requirements = self.no_requirements;
        o.no_malenia_heal = self.no_malenia_heal;
        o.gaol_keys = self.gaol_keys;
        o.dlc_start = self.dlc_start;
        o.dlc_start_level = self.dlc_start_level;
        o.dlc_start_flasks = self.dlc_start_flasks;
        o.dlc_start_smithing = self.dlc_start_smithing;
        o.dlc_start_shop = self.dlc_start_shop;
        o.random_gestures = self.random_gestures;
        if let Some(spec) = &self.kindling_shards {
            let (needed, count) = rando::shards_of(spec).map_err(format_err)?;
            o.kindling_shards = count;
            o.kindling_needed = needed;
        }
        o.no_gargoyle_poison = self.no_gargoyle_poison;
        o.torrent_woods = self.torrent_woods;
        o.boss_music = self.boss_music;
        o.area_music = self.area_music;
        o.enemy_colours = self.enemy_colours;
        o.fog_gates = self.fog_gates;
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
        o.runes_follow = self.boss_runes_follow;
        o.oops = self.oops_all.as_deref().and_then(rando::Oops::parse);
        o.onslaught = self.onslaught;
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
            no_spoiler,
            out,
            sliders,
        } => {
            let done = rando::generate(
                &Request {
                    game,
                    seed: seed.unwrap_or_else(rando::hidden_seed),
                    out,
                    options: sliders.options()?,
                    enemies: sliders.enemy_options()?,
                    native: sliders.native,
                    mods: sliders.with_mods,
                    hidden: no_spoiler,
                },
                &say,
            )?;
            match (&done.spoiler, seed) {
                (Some(spoiler), Some(seed)) => say(&format!(
                    "seed {seed}: {} slots shuffled; mod folder {}, spoiler log {}",
                    done.slots,
                    done.folder.display(),
                    spoiler.display()
                )),
                _ => say(&format!(
                    "hidden seed: {} slots shuffled; mod folder {}, no spoiler log",
                    done.slots,
                    done.folder.display()
                )),
            }
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

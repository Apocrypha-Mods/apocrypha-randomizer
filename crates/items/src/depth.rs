//! How deep a location is, from 0 (the start, in plain sight) to 1 (the end
//! of the game, behind its hardest fights), which the bias sliders lean on.
//! Our own measure:
//!
//! - **Lateness** (three quarters of it): the order in which the unmodded
//!   game, walked from the start, opens the location's area, from 0 for the
//!   first area to 1 for the last. A location in no area counts as the start.
//! - **Hardness** (a quarter): what kind of location it is. A boss's reward
//!   is hardest (1), then a one-time drop from a notable enemy (0.7), an
//!   item in a dungeon or legacy dungeon (0.5), an NPC's gift or a scripted
//!   award (0.4), and an item lying in the open world (0).
//!
//! An item's worth is the depth of the location the unmodded game gives it
//! at: an item the game keeps for late and hard places is worth more.

use gamedata::model::Kind;
use logic::Bound;

const LATENESS: f64 = 0.75;
const HARDNESS: f64 = 0.25;

fn hardness(kind: Kind, open_world: bool) -> f64 {
    match kind {
        Kind::BossReward => 1.0,
        Kind::EnemyDrop => 0.7,
        Kind::Gift | Kind::Event => 0.4,
        _ if !open_world => 0.5,
        _ => 0.0,
    }
}

/// Every location's depth, by index into the model's locations.
pub fn depths(bound: &Bound) -> Vec<f64> {
    let walk = bound.walk(&bound.vanilla());
    let n = bound.graph.areas.len();
    let mut rank = vec![0.0; n];
    let last = (walk.order.len().max(2) - 1) as f64;
    for (i, &a) in walk.order.iter().enumerate() {
        rank[a] = i as f64 / last;
    }
    bound
        .model
        .locations
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let late = bound.location_area[i].map_or(0.0, |a| rank[a]);
            let open = l.map.is_none_or(|m| m.is_open_world());
            LATENESS * late + HARDNESS * hardness(l.kind, open)
        })
        .collect()
}

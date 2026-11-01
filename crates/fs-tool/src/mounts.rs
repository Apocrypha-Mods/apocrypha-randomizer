//! The maps' mount events (event type 21): two parts, tied as rider and mount.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fs_bytes::Result;
use fs_msb::layout::{entry_name, field};
use gamedata::Sources;

const PARTS: &str = "PARTS_PARAM_ST";
const EVENTS: &str = "EVENT_PARAM_ST";
const MODELS: &str = "MODEL_PARAM_ST";

pub fn run(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut unset = 0;
    for (m, msb) in &s.maps {
        let parts = &msb.table(PARTS).entries;
        let models = &msb.table(MODELS).entries;
        let model_of = |i: i32| -> Option<String> {
            let p = parts.get(usize::try_from(i).ok()?)?;
            let t = field(PARTS, "type").get(p, 0)?;
            let mi = usize::try_from(field(PARTS, "model").get(p, 0)?).ok()?;
            Some(format!("{}/{t}", entry_name(models.get(mi)?)?))
        };
        for e in &msb.table(EVENTS).entries {
            if field(EVENTS, "type").get(e, 0) != Some(21) {
                continue;
            }
            let at = i64::from_le_bytes(e[0x20..0x28].try_into().unwrap()) as usize;
            let word = |o: usize| i32::from_le_bytes(e[at + o..at + o + 4].try_into().unwrap());
            let (a, b) = (word(0), word(4));
            if a < 0 && b < 0 {
                unset += 1;
                continue;
            }
            let (ma, mb) = (model_of(a), model_of(b));
            println!("{m} {a} {ma:?} on {b} {mb:?}");
            *pairs
                .entry((ma.unwrap_or_default(), mb.unwrap_or_default()))
                .or_default() += 1;
        }
    }
    println!("unset: {unset}");
    for ((a, b), n) in pairs {
        println!("{n:4}  {a} / {b}");
    }
    Ok(())
}

//! What instructions do, as far as the game's own scripts show it, stored as
//! our own TOML (`catalogue/elden_ring.toml`).
//!
//! No third-party instruction definitions are used. Every entry was worked
//! out with `apoc-fs instructions`:
//! - which values each argument word takes across all 589 scripts, with the
//!   placeholders that parameters fill in replaced by the values each call
//!   passes ([`resolve_calls`]);
//! - which known sets those values fall in: event ids, param flags, item lots,
//!   text ids, param rows.
//!
//! Names are deliberately plain and say only what the evidence shows.
//! `fs-tool/tests/real_events.rs` re-checks the claims on the real files.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::{Emevd, Event, Instruction, Parameter};

const ELDEN_RING: &str = include_str!("../catalogue/elden_ring.toml");

/// How sure an entry is, with the same meaning as in the param layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Confirmed,
    Probable,
}

/// What an argument word holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgKind {
    /// The id of an event.
    EventId,
    /// An event flag.
    Flag,
    /// 0 or 1: off or on.
    State,
    /// A row id of the named param table.
    Param(String),
    /// An id in the named text table (e.g. `NpcName`).
    Text(String),
    /// The id of a character or other map entity (to be checked against MSB).
    Entity,
    /// A small number whose meaning is not settled.
    Number,
    /// Several small values packed into one word.
    Packed,
    /// The rest of the arguments, passed on as they are.
    Rest,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arg {
    pub word: usize,
    pub name: String,
    pub kind: ArgKind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub bank: u32,
    pub id: u32,
    pub name: String,
    /// The argument length every use has, in bytes, when it is fixed.
    #[serde(default)]
    pub length: Option<usize>,
    pub confidence: Confidence,
    pub evidence: String,
    #[serde(default, rename = "arg")]
    pub args: Vec<Arg>,
}

impl Entry {
    /// The word at `word` of an instruction's arguments.
    pub fn word(ins: &Instruction, word: usize) -> Option<u32> {
        ins.args
            .get(word * 4..word * 4 + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn arg(&self, name: &str) -> &Arg {
        self.args
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("{} has no argument {name}", self.name))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(rename = "instruction")]
    entries: Vec<Entry>,
}

/// Every catalogued Elden Ring instruction.
pub fn elden_ring() -> &'static [Entry] {
    static CATALOGUE: OnceLock<Vec<Entry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        let f: File = toml::from_str(ELDEN_RING).expect("catalogue/elden_ring.toml parses");
        f.entries
    })
}

/// The entry for an instruction, if it is catalogued.
pub fn lookup(bank: u32, id: u32) -> Option<&'static Entry> {
    elden_ring().iter().find(|e| e.bank == bank && e.id == id)
}

/// The entry with this name.
pub fn named(name: &str) -> &'static Entry {
    elden_ring()
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no instruction named {name}"))
}

/// The instructions that start an event: word 0 a slot, word 1 the event id,
/// the rest the event's own arguments.
pub const EVENT_STARTS: [(u32, u32); 2] = [(2000, 0), (2000, 6)];

/// One placeholder word filled in at one call: the instruction it is in, the
/// word, and the value the call passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filled {
    pub bank: u32,
    pub id: u32,
    pub word: usize,
    pub value: u32,
}

/// Every placeholder word's value at every call that starts its event.
///
/// A parameter copies `size` bytes from byte `source` of the call's event
/// arguments (after the slot and the event id) to byte `target` of one of the
/// event's instructions. The event is looked up in the calling script first,
/// then in `common` (the shared `common_func` script).
pub fn resolve_calls<'a>(
    scripts: impl IntoIterator<Item = &'a Emevd>,
    common: Option<&Emevd>,
) -> Vec<Filled> {
    let index = |e: &'a Emevd| -> HashMap<u32, &'a Event> {
        e.events.iter().map(|ev| (ev.id as u32, ev)).collect()
    };
    let common_index: HashMap<u32, &Event> = common
        .map(|e| e.events.iter().map(|ev| (ev.id as u32, ev)).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for script in scripts {
        let local = index(script);
        for ins in script.events.iter().flat_map(|ev| &ev.instructions) {
            if !EVENT_STARTS.contains(&(ins.bank, ins.id)) || ins.args.len() < 8 {
                continue;
            }
            let target = u32::from_le_bytes(ins.args[4..8].try_into().unwrap());
            let Some(ev) = local.get(&target).or_else(|| common_index.get(&target)) else {
                continue;
            };
            let call = &ins.args[8..];
            for p in &ev.parameters {
                let Some(dest) = ev.instructions.get(p.instruction as usize) else {
                    continue;
                };
                for k in 0..(p.size.max(0) as usize / 4) {
                    let src = p.source as usize + k * 4;
                    let Some(word) = call.get(src..src + 4) else {
                        continue;
                    };
                    out.push(Filled {
                        bank: dest.bank,
                        id: dest.id,
                        word: p.target as usize / 4 + k,
                        value: u32::from_le_bytes(word.try_into().unwrap()),
                    });
                }
            }
        }
    }
    out
}

/// One use of an instruction, with the arguments it runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// The event the instruction is in.
    pub event: i64,
    /// For an event started from the script, the event that started it.
    pub caller: Option<i64>,
    /// For an event started from the script, the arguments it was started
    /// with: uses with the same event and call ran together.
    pub call: Option<Vec<u8>>,
    /// The arguments, with every placeholder filled in from the call.
    pub args: Vec<u8>,
}

impl Use {
    pub fn word(&self, word: usize) -> Option<u32> {
        self.args
            .get(word * 4..word * 4 + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
}

/// Every use of instruction `bank[id]` that `script` runs: the uses written
/// in it with no placeholders, and one use per chain of calls that starts an
/// event holding the instruction (from `script` itself, or from `common`),
/// with the calls' arguments copied in. A called event that starts another
/// with its own placeholders is followed too, as deep as the scripts go.
pub fn uses(script: &Emevd, common: Option<&Emevd>, bank: u32, id: u32) -> Vec<Use> {
    let local: HashMap<u32, &Event> = script.events.iter().map(|e| (e.id as u32, e)).collect();
    let shared: HashMap<u32, &Event> = common
        .map(|c| c.events.iter().map(|e| (e.id as u32, e)).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for ev in &script.events {
        let walk = Walk {
            local: &local,
            shared: &shared,
            want: (bank, id),
        };
        walk.event(ev, None, true, 0, &mut out);
    }
    out
}

/// How deep calls are followed; Elden Ring's scripts go 3 deep.
const MAX_CALL_DEPTH: usize = 8;

struct Walk<'a> {
    local: &'a HashMap<u32, &'a Event>,
    shared: &'a HashMap<u32, &'a Event>,
    want: (u32, u32),
}

impl Walk<'_> {
    /// The instructions of `ev` that a call fills in (`call` is the call's
    /// event arguments), or, with no call, those that need nothing filled in.
    fn event(
        &self,
        ev: &Event,
        call: Option<(&[u8], i64)>,
        in_script: bool,
        depth: usize,
        out: &mut Vec<Use>,
    ) {
        for (j, ins) in ev.instructions.iter().enumerate() {
            let params: Vec<&Parameter> = ev
                .parameters
                .iter()
                .filter(|p| p.instruction == j as i64)
                .collect();
            // With no call, only what is written whole; with one, only what
            // the call fills in (the rest is reported without the call).
            let args = match call {
                None if params.is_empty() => ins.args.clone(),
                None => continue,
                Some(_) if params.is_empty() => continue,
                Some((from, _)) => {
                    let mut args = ins.args.clone();
                    for p in params {
                        let (src, dst, n) = (p.source as usize, p.target as usize, p.size as usize);
                        if let (Some(v), true) = (from.get(src..src + n), dst + n <= args.len()) {
                            args[dst..dst + n].copy_from_slice(v);
                        }
                    }
                    args
                }
            };
            if (ins.bank, ins.id) == self.want {
                out.push(Use {
                    event: ev.id,
                    caller: call.map(|c| c.1),
                    call: call.map(|c| c.0.to_vec()),
                    args: args.clone(),
                });
            }
            if !EVENT_STARTS.contains(&(ins.bank, ins.id))
                || args.len() < 8
                || depth >= MAX_CALL_DEPTH
            {
                continue;
            }
            let target = u32::from_le_bytes(args[4..8].try_into().unwrap());
            // A shared event can only start shared events.
            let found = in_script
                .then(|| self.local.get(&target))
                .flatten()
                .map(|e| (e, true))
                .or_else(|| self.shared.get(&target).map(|e| (e, false)));
            if let Some((called, local)) = found {
                self.event(called, Some((&args[8..], ev.id)), local, depth + 1, out);
            }
        }
    }
}

/// Every value an instruction's word takes: written directly, or filled in at
/// a call. Placeholders themselves are left out.
pub fn values(
    scripts: &[&Emevd],
    common: Option<&Emevd>,
    bank: u32,
    id: u32,
    word: usize,
) -> Vec<u32> {
    let mut out = Vec::new();
    for script in scripts {
        for ev in &script.events {
            for (i, ins) in ev.instructions.iter().enumerate() {
                if (ins.bank, ins.id) != (bank, id) {
                    continue;
                }
                let placeholder = ev.parameters.iter().any(|p| {
                    p.instruction == i as i64
                        && (p.target / 4..(p.target + i64::from(p.size)) / 4)
                            .contains(&(word as i64))
                });
                if let (false, Some(v)) = (placeholder, Entry::word(ins, word)) {
                    out.push(v);
                }
            }
        }
    }
    out.extend(
        resolve_calls(scripts.iter().copied(), common)
            .into_iter()
            .filter(|f| (f.bank, f.id, f.word) == (bank, id, word))
            .map(|f| f.value),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_is_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for e in elden_ring() {
            assert!(
                seen.insert((e.bank, e.id)),
                "{}[{}] is listed twice",
                e.bank,
                e.id
            );
            assert!(
                !e.evidence.trim().is_empty(),
                "{} gives no evidence",
                e.name
            );
            if let Some(len) = e.length {
                for a in &e.args {
                    assert!(
                        a.word * 4 < len,
                        "{}.{} lies past its {len} bytes",
                        e.name,
                        a.name
                    );
                }
            }
        }
        assert_eq!(lookup(2003, 66).unwrap().name, "set_flag");
    }

    #[test]
    fn a_call_fills_in_the_placeholders_of_the_event_it_starts() {
        let called = Event {
            id: 90005000,
            rest: 0,
            instructions: vec![Instruction {
                bank: 2003,
                id: 66,
                args: [0u32, 0, 1].iter().flat_map(|v| v.to_le_bytes()).collect(),
                layer: None,
            }],
            parameters: vec![Parameter {
                instruction: 0,
                target: 4,
                source: 4,
                size: 4,
                unk: 1,
            }],
        };
        let common = Emevd {
            events: vec![called],
            ..Default::default()
        };
        let call = Instruction {
            bank: 2000,
            id: 6,
            args: [0u32, 90005000, 111, 4242]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
            layer: None,
        };
        let map = Emevd {
            events: vec![Event {
                id: 0,
                rest: 0,
                instructions: vec![call],
                parameters: vec![],
            }],
            ..Default::default()
        };
        let filled = resolve_calls([&map], Some(&common));
        assert_eq!(
            filled,
            vec![Filled {
                bank: 2003,
                id: 66,
                word: 1,
                value: 4242
            }]
        );
        assert_eq!(
            values(&[&map, &common], Some(&common), 2003, 66, 1),
            vec![4242],
            "the placeholder 0 itself is left out"
        );
    }

    #[test]
    fn a_use_carries_every_word_with_the_call_filled_in() {
        let ins = |bank, id, words: &[u32]| Instruction {
            bank,
            id,
            args: words.iter().flat_map(|v| v.to_le_bytes()).collect(),
            layer: None,
        };
        // Event 7 awards a lot for a boss; the call fills in only the lot.
        let called = Event {
            id: 7,
            rest: 0,
            instructions: vec![ins(2004, 76, &[1234, 0])],
            parameters: vec![Parameter {
                instruction: 0,
                target: 4,
                source: 0,
                size: 4,
                unk: 0,
            }],
        };
        let caller = Event {
            id: 0,
            rest: 0,
            instructions: vec![ins(2000, 0, &[0, 7, 555]), ins(2004, 76, &[9, 10])],
            parameters: vec![],
        };
        let script = Emevd {
            events: vec![caller, called],
            ..Default::default()
        };
        let mut got = uses(&script, None, 2004, 76);
        got.sort_by_key(|u| u.event);
        assert_eq!(got.len(), 2, "one written directly, one through the call");
        assert_eq!((got[0].event, got[0].caller), (0, None));
        assert_eq!((got[0].word(0), got[0].word(1)), (Some(9), Some(10)));
        assert_eq!((got[1].event, got[1].caller), (7, Some(0)));
        assert_eq!((got[1].word(0), got[1].word(1)), (Some(1234), Some(555)));
    }

    #[test]
    fn calls_are_followed_through_a_shared_event_that_starts_another() {
        let ins = |bank, id, words: &[u32]| Instruction {
            bank,
            id,
            args: words.iter().flat_map(|v| v.to_le_bytes()).collect(),
            layer: None,
        };
        let pass = |instruction, target, source| Parameter {
            instruction,
            target,
            source,
            size: 4,
            unk: 0,
        };
        // Shared event 7 starts shared event 8 with its own argument 0;
        // event 8 awards the lot it is given.
        let outer = Event {
            id: 7,
            rest: 0,
            instructions: vec![ins(2000, 6, &[0, 8, 0])],
            parameters: vec![pass(0, 8, 0)],
        };
        let inner = Event {
            id: 8,
            rest: 0,
            instructions: vec![ins(2003, 4, &[0])],
            parameters: vec![pass(0, 0, 0)],
        };
        let common = Emevd {
            events: vec![outer, inner],
            ..Default::default()
        };
        let map = Emevd {
            events: vec![Event {
                id: 0,
                rest: 0,
                instructions: vec![ins(2000, 6, &[0, 7, 555])],
                parameters: vec![],
            }],
            ..Default::default()
        };
        let got = uses(&map, Some(&common), 2003, 4);
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].event, got[0].caller, got[0].word(0)),
            (8, Some(7), Some(555))
        );
        assert!(
            uses(&common, None, 2003, 4).is_empty(),
            "on its own the shared script awards nothing whole"
        );
    }
}

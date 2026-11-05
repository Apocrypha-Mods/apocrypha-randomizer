//! What must hold for an event to reach each of its instructions: the
//! conditions it waits on and the branches it takes, read as the catalogue's
//! control-flow entries describe them (`catalogue/elden_ring.toml`).
//!
//! Every path through a run is followed. On each, what held is collected, and
//! at an instruction asked about it becomes a way to reach it: tests that
//! must all hold. Any one way is enough.
//!
//! What is not understood can only lose ways, never add them, so a reader of
//! the ways never takes an instruction for reachable when the game would not
//! reach it:
//! - a control instruction (banks 1000 to 1999) that the catalogue does not
//!   describe ends the path;
//! - so does a jump backwards, or to a label that is not there;
//! - past a budget of steps or of ways, the rest is dropped.
//!
//! Conditions other than a flag's state are kept whole, as [`Test::Other`],
//! for the reader to judge.

use std::collections::{BTreeMap, BTreeSet};

use crate::Instruction;
use crate::catalogue::{self, Entry};

/// What a condition tests.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Test {
    /// An event flag is on.
    Flag(u32),
    /// Anything else: the instruction that tests it, its group byte cleared.
    Other { bank: u32, id: u32, args: Vec<u8> },
}

/// What an unread test is, as the catalogue reads it, for a reader to judge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The character is dead (`if_character_dead` asking for state 1).
    Dead(u32),
    /// The player holds a good, or does not (`if_item`, category 3).
    Good { id: i32, held: bool },
    /// The player did the action at an object (`if_object_action`).
    ObjectAction(u32),
    /// Whether the player is in their own world (`if_my_world`), or asked not to be.
    MyWorld { wanted: bool },
}

impl Test {
    /// What this test is, when the catalogue has read it. `None` for a state
    /// or a category not read, and for anything else.
    pub fn reading(&self) -> Option<Reading> {
        let Test::Other { bank, id, args } = self else {
            return None;
        };
        let ins = Instruction {
            bank: *bank,
            id: *id,
            args: args.clone(),
            layer: None,
        };
        let e = catalogue::lookup(*bank, *id)?;
        let word = |name| Entry::word(&ins, e.arg(name).word);
        match e.name.as_str() {
            "if_character_dead" => {
                (word("state")? == 1).then_some(Reading::Dead(word("character")?))
            }
            "if_item" => (e.byte(&ins, "category")? == 3).then_some(Reading::Good {
                id: word("item")? as i32,
                held: word("state")? == 1,
            }),
            "if_object_action" => Some(Reading::ObjectAction(word("object")?)),
            "if_my_world" => Some(Reading::MyWorld {
                wanted: e.byte(&ins, "not_wanted")? == 0,
            }),
            _ => None,
        }
    }
}

/// A test that must hold, or must not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lit {
    pub test: Test,
    pub holds: bool,
}

/// Tests that must all hold.
pub type Way = BTreeSet<Lit>;

/// How many ways to keep for one instruction: the shortest. Dropping ways only
/// makes an instruction harder to reach.
pub const MAX_WAYS: usize = 16;
/// How many instructions to visit in one run, over every path.
const MAX_STEPS: usize = 200_000;
/// Condition groups run from -15 to 15.
const GROUPS: usize = 31;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Expr {
    Lit(Lit),
    All(Vec<Expr>),
    Any(Vec<Expr>),
}

impl Expr {
    fn not(self) -> Expr {
        match self {
            Expr::Lit(l) => Expr::Lit(Lit {
                holds: !l.holds,
                ..l
            }),
            Expr::All(v) => Expr::Any(v.into_iter().map(Expr::not).collect()),
            Expr::Any(v) => Expr::All(v.into_iter().map(Expr::not).collect()),
        }
    }

    /// The ways for this to hold, shortest first.
    fn ways(&self) -> Vec<Way> {
        match self {
            Expr::Lit(l) => vec![BTreeSet::from([l.clone()])],
            Expr::All(v) => v
                .iter()
                .fold(vec![Way::new()], |acc, e| both(&acc, &e.ways())),
            Expr::Any(v) => trim(v.iter().flat_map(Expr::ways).collect()),
        }
    }
}

/// Every way to have one of `a` and one of `b`, leaving out contradictions.
fn both(a: &[Way], b: &[Way]) -> Vec<Way> {
    let mut out = Vec::new();
    for x in a {
        for y in b {
            let w: Way = x.union(y).cloned().collect();
            let contradicts = w.iter().any(|l| {
                w.contains(&Lit {
                    test: l.test.clone(),
                    holds: !l.holds,
                })
            });
            if !contradicts {
                out.push(w);
            }
        }
    }
    trim(out)
}

/// Those asking the fewest things to hold first, then the shortest, without
/// repeats or ways that ask more than another does, and no more than
/// [`MAX_WAYS`]. A test that must fail costs a reader little (a flag off is
/// taken to be off), one that must pass may need explaining in turn: Ymir's
/// quest event (common 4339) sets 2051459723 on 22 ways or more, and the 16
/// shortest all ask for 4323, a flag nothing is known to set, while longer
/// ones ask for it off.
fn trim(mut ways: Vec<Way>) -> Vec<Way> {
    let asks = |w: &Way| w.iter().filter(|l| l.holds).count();
    ways.sort_by(|a, b| {
        asks(a)
            .cmp(&asks(b))
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    ways.dedup();
    let mut out: Vec<Way> = Vec::new();
    for w in ways {
        if !out.iter().any(|o| o.is_subset(&w)) {
            out.push(w);
        }
        if out.len() == MAX_WAYS {
            break;
        }
    }
    out
}

struct Path {
    at: usize,
    /// What has held so far.
    needs: Vec<Way>,
    groups: Vec<Vec<Expr>>,
}

impl Path {
    /// Group `g`'s value, emptying it: all of a positive group's conditions,
    /// any of a negative one's.
    fn take(&mut self, g: i8) -> Option<Expr> {
        let v = std::mem::take(&mut self.groups[group_index(g)?]);
        match g {
            0 => None,
            g if g > 0 => Some(Expr::All(v)),
            _ => Some(Expr::Any(v)),
        }
    }

    /// The same path, having also met `e`; `None` when it cannot be.
    fn with(&self, e: Expr, at: usize) -> Option<Path> {
        let needs = both(&self.needs, &e.ways());
        (!needs.is_empty()).then(|| Path {
            at,
            needs,
            groups: self.groups.clone(),
        })
    }
}

fn group_index(g: i8) -> Option<usize> {
    (-15..=15).contains(&g).then(|| (g as i32 + 15) as usize)
}

/// What a condition instruction tests: a flag's state when it is `if_flag`
/// on an event flag, else the instruction itself.
fn test_of(ins: &Instruction) -> Lit {
    let f = catalogue::named("if_flag");
    if (ins.bank, ins.id) == (f.bank, f.id)
        && let (Some(state @ 0..=1), Some(0), Some(flag)) = (
            f.byte(ins, "state"),
            f.byte(ins, "flag_kind"),
            Entry::word(ins, f.arg("flag").word),
        )
        && flag != 0
    {
        return Lit {
            test: Test::Flag(flag),
            holds: state == 1,
        };
    }
    other(ins)
}

/// A range test as the exe evaluates it (`EcTestMultiEventFlagState`, +0x580210 in
/// 2.7.2): 0 all on, 1 all off, 2 any on, 3 any off.
fn range(state: u8, first: u32, last: u32) -> Option<Expr> {
    if first == 0 || first > last || last - first >= 64 {
        return None;
    }
    let lits = |holds| {
        (first..=last)
            .map(|f| {
                Expr::Lit(Lit {
                    test: Test::Flag(f),
                    holds,
                })
            })
            .collect()
    };
    match state {
        0 => Some(Expr::All(lits(true))),
        1 => Some(Expr::All(lits(false))),
        2 => Some(Expr::Any(lits(true))),
        3 => Some(Expr::Any(lits(false))),
        _ => None,
    }
}

/// What a condition instruction puts in its group: a flag's state, a range's
/// (`if_flag_range`), or the instruction's own test.
fn condition(ins: &Instruction) -> Expr {
    let r = catalogue::named("if_flag_range");
    if (ins.bank, ins.id) == (r.bank, r.id)
        && let (Some(state), Some(first), Some(last)) = (
            r.byte(ins, "state"),
            Entry::word(ins, r.arg("first").word),
            Entry::word(ins, r.arg("last").word),
        )
        && let Some(e) = range(state, first, last)
    {
        return e;
    }
    Expr::Lit(test_of(ins))
}

/// An instruction's own test, met. A condition's group byte is cleared, so
/// that the same test in two groups is one.
fn other(ins: &Instruction) -> Lit {
    let mut args = ins.args.clone();
    if ins.bank < 1000
        && let Some(b) = args.first_mut()
    {
        *b = 0;
    }
    Lit {
        test: Test::Other {
            bank: ins.bank,
            id: ins.id,
            args,
        },
        holds: true,
    }
}

/// Where a flag test of a control instruction (`skip_if_flag` and the like)
/// leads: the flag in the state its bytes name.
fn flag_test(e: &Entry, ins: &Instruction) -> Option<Expr> {
    let state = e.byte(ins, "state")?;
    let flag = Entry::word(ins, e.arg("flag").word)?;
    if state > 1 {
        return None;
    }
    if e.byte(ins, "flag_kind") != Some(0) || flag == 0 {
        return Some(Expr::Lit(other(ins)));
    }
    Some(Expr::Lit(Lit {
        test: Test::Flag(flag),
        holds: state == 1,
    }))
}

/// Where a control instruction can go next: `None` for the end of the path,
/// else the instruction to go to and what must hold to go there.
type Next = Vec<(Option<usize>, Option<Expr>)>;

/// For each instruction `want` picks out, the ways the run reaches it.
pub fn ways_to(
    instructions: &[Instruction],
    want: impl Fn(&Instruction) -> bool,
) -> BTreeMap<usize, Vec<Way>> {
    let mut found: BTreeMap<usize, Vec<Way>> = BTreeMap::new();
    // Paths waiting to go on, by where they are and their open groups. Every
    // jump goes forward (a jump backwards ends the path), so taking the nearest
    // first means every path that can meet another at an instruction, with the
    // same groups open, has arrived before it goes on: from there they walk the
    // same way, so they go on as one, needing what either needed. Followed one
    // by one, some events branch on the same tests so often that their paths
    // ran into the thousands (common 4739 took 20 s of a seed's 70).
    let mut todo: BTreeMap<(usize, Vec<Vec<Expr>>), Vec<Way>> = BTreeMap::new();
    todo.insert((0, vec![Vec::new(); GROUPS]), vec![Way::new()]);
    let mut steps = 0;
    while let Some(((at, groups), needs)) = todo.pop_first() {
        let mut p = Path { at, needs, groups };
        while let Some(ins) = instructions.get(p.at) {
            steps += 1;
            if steps > MAX_STEPS {
                return found;
            }
            if want(ins) {
                let ways = found.entry(p.at).or_default();
                ways.extend(p.needs.iter().cloned());
                *ways = trim(std::mem::take(ways));
            }
            let Some(next) = step(&mut p, instructions, ins) else {
                break;
            };
            match &next[..] {
                // Straight on, with nothing new to meet.
                [(Some(to), None)] if *to > p.at => p.at = *to,
                _ => {
                    for (to, e) in next {
                        let Some(to) = to else { continue };
                        let path = match e {
                            Some(e) => p.with(e, to),
                            None => Some(Path {
                                at: to,
                                needs: p.needs.clone(),
                                groups: p.groups.clone(),
                            }),
                        };
                        if let Some(path) = path {
                            let needs = todo.entry((path.at, path.groups)).or_default();
                            needs.extend(path.needs);
                            *needs = trim(std::mem::take(needs));
                        }
                    }
                    break;
                }
            }
        }
    }
    found
}

/// One instruction's effect on a path; `None` ends it.
fn step(p: &mut Path, all: &[Instruction], ins: &Instruction) -> Option<Next> {
    let here = p.at;
    let on = |to: usize| (to <= all.len()).then_some(to);
    let label = |l: u8| {
        all.iter()
            .enumerate()
            .skip(here + 1)
            .find(|(_, i)| i.bank == 1014 && i.id == l as u32)
            .map(|(k, _)| k)
    };
    let straight = Some(vec![(Some(here + 1), None)]);
    // Labels, and actions (banks 2000 and up), go straight on.
    if ins.bank == 1014 || ins.bank >= 2000 {
        return straight;
    }
    let entry = catalogue::lookup(ins.bank, ins.id);
    let name = entry.map(|e| e.name.as_str());
    if ins.bank < 1000 {
        let g = *ins.args.first()? as i8;
        let e = if name == Some("combine_group") {
            let e = entry?;
            let v = p.take(e.byte(ins, "source")? as i8)?;
            if e.byte(ins, "state")? == 1 {
                v
            } else {
                v.not()
            }
        } else {
            condition(ins)
        };
        if g == 0 {
            // The event waits here until it holds.
            return Some(vec![(Some(here + 1), Some(e))]);
        }
        p.groups[group_index(g)?].push(e);
        return straight;
    }
    let e = entry?;
    // Taken, or not: where to and on what.
    let branch = |to: Option<usize>, cond: Expr| -> Next {
        vec![(to, Some(cond.clone())), (Some(here + 1), Some(cond.not()))]
    };
    let group_test = |p: &mut Path| -> Option<Expr> {
        let v = p.take(e.byte(ins, "group")? as i8)?;
        Some(if e.byte(ins, "state")? == 1 {
            v
        } else {
            v.not()
        })
    };
    Some(match e.name.as_str() {
        "wait_seconds" | "wait_frames" | "wait_random_seconds" | "wait_frames_b" => {
            return straight;
        }
        "skip" => vec![(on(here + 1 + e.byte(ins, "lines")? as usize), None)],
        "goto" => vec![(Some(label(e.byte(ins, "label")?)?), None)],
        "end" => return None,
        "skip_if_group" => {
            let c = group_test(p)?;
            branch(on(here + 1 + e.byte(ins, "lines")? as usize), c)
        }
        "end_if_group" => branch(None, group_test(p)?),
        "goto_if_group" | "goto_if_group_b" => {
            let c = group_test(p)?;
            branch(Some(label(e.byte(ins, "label")?)?), c)
        }
        "skip_if_flag" => branch(
            on(here + 1 + e.byte(ins, "lines")? as usize),
            flag_test(e, ins)?,
        ),
        "end_if_flag" => branch(None, flag_test(e, ins)?),
        "goto_if_flag" => branch(Some(label(e.byte(ins, "label")?)?), flag_test(e, ins)?),
        "skip_if_flag_range" => {
            let lines = e.byte(ins, "lines")? as usize;
            let first = Entry::word(ins, e.arg("first").word)?;
            let last = Entry::word(ins, e.arg("last").word)?;
            let c =
                range(e.byte(ins, "state")?, first, last).unwrap_or_else(|| Expr::Lit(other(ins)));
            branch(on(here + 1 + lines), c)
        }
        // What these three test is 0 in a world of one's own (see end_if in
        // the catalogue): they jump exactly when they ask for 0.
        "skip_if" => match e.byte(ins, "state")? {
            0 => vec![(on(here + 1 + e.byte(ins, "lines")? as usize), None)],
            _ => return straight,
        },
        "end_if" => match e.byte(ins, "state")? {
            0 => return None,
            _ => return straight,
        },
        "goto_if" => match e.byte(ins, "state")? {
            0 => vec![(Some(label(e.byte(ins, "label")?)?), None)],
            _ => return straight,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(bank: u32, id: u32, words: &[u32]) -> Instruction {
        Instruction {
            bank,
            id,
            args: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
            layer: None,
        }
    }

    #[test]
    fn ways_asking_fewest_flags_on_are_kept_first() {
        let lit = |f, holds| Lit {
            test: Test::Flag(f),
            holds,
        };
        // More one-flag ways than are kept, and one longer way asking only
        // for flags off.
        let mut ways: Vec<Way> = (1..=MAX_WAYS as u32 + 4)
            .map(|f| BTreeSet::from([lit(f, true)]))
            .collect();
        let off = BTreeSet::from([lit(100, false), lit(101, false), lit(102, false)]);
        ways.push(off.clone());
        let kept = trim(ways);
        assert_eq!(kept.len(), MAX_WAYS);
        assert_eq!(kept[0], off);
    }

    fn bytes(b: [i8; 3]) -> u32 {
        u32::from_le_bytes([b[0] as u8, b[1] as u8, b[2] as u8, 0])
    }

    fn if_flag(group: i8, on: bool, flag: u32) -> Instruction {
        ins(3, 0, &[bytes([group, on as i8, 0]), flag])
    }

    /// The marker the tests ask about.
    fn mark() -> Instruction {
        ins(2003, 66, &[0, 999, 1])
    }

    fn flag(f: u32, holds: bool) -> Lit {
        Lit {
            test: Test::Flag(f),
            holds,
        }
    }

    fn ways(script: &[Instruction]) -> Vec<Way> {
        ways_to(script, |i| i == &mark())
            .into_values()
            .next()
            .unwrap_or_default()
    }

    #[test]
    fn a_skip_on_a_flag_splits_the_path() {
        // Skip the mark when flag 10 is on.
        let s = [ins(1003, 1, &[bytes([1, 1, 0]), 10]), mark()];
        assert_eq!(ways(&s), vec![Way::from([flag(10, false)])]);
    }

    #[test]
    fn a_negative_group_is_any_and_a_positive_one_all() {
        let any = [
            if_flag(-1, true, 1),
            if_flag(-1, true, 2),
            ins(0, 0, &[bytes([0, 1, -1])]),
            mark(),
        ];
        assert_eq!(
            ways(&any),
            vec![Way::from([flag(1, true)]), Way::from([flag(2, true)])]
        );
        let all = [
            if_flag(1, true, 1),
            if_flag(1, true, 2),
            ins(0, 0, &[bytes([0, 1, 1])]),
            mark(),
        ];
        assert_eq!(ways(&all), vec![Way::from([flag(1, true), flag(2, true)])]);
    }

    #[test]
    fn a_group_skipped_on_when_false_asks_it_to_be_true() {
        // Skip the mark unless group 1 (flags 1 and 2) holds; the other way
        // to the mark is through either flag off, which skips nothing.
        let s = [
            if_flag(1, true, 1),
            if_flag(1, true, 2),
            ins(1000, 1, &[bytes([1, 0, 1])]),
            mark(),
        ];
        assert_eq!(ways(&s), vec![Way::from([flag(1, true), flag(2, true)])]);
    }

    #[test]
    fn gotos_and_ends() {
        // On flag 5, go to label 3, past an end, to the mark.
        let s = [
            ins(1003, 101, &[bytes([3, 1, 0]), 5]),
            ins(1000, 4, &[0]),
            ins(1014, 3, &[]),
            mark(),
        ];
        assert_eq!(ways(&s), vec![Way::from([flag(5, true)])]);
        // A label that is not ahead ends the path.
        let s = [ins(1014, 3, &[]), ins(1000, 103, &[3]), mark()];
        assert!(ways(&s).is_empty());
    }

    #[test]
    fn what_is_not_understood_only_loses_ways() {
        // An uncatalogued control instruction ends the path.
        let s = [ins(1000, 999, &[0]), mark()];
        assert!(ways(&s).is_empty());
        // An uncatalogued condition is kept whole, for the reader.
        let s = [ins(4, 3, &[bytes([0, 1, 0]), 77]), mark()];
        let w = ways(&s);
        assert_eq!(w.len(), 1);
        assert!(matches!(
            w[0].first().unwrap().test,
            Test::Other { bank: 4, id: 3, .. }
        ));
    }

    #[test]
    fn a_range_skipped_on_any_on() {
        // Unless a stage of 10 to 12 is on, set the mark.
        let s = [ins(1003, 3, &[bytes([1, 2, 0]), 10, 12]), mark()];
        assert_eq!(
            ways(&s),
            vec![Way::from([
                flag(10, false),
                flag(11, false),
                flag(12, false)
            ])]
        );
        // Past it only with one on.
        let s = [
            ins(1003, 3, &[bytes([1, 2, 0]), 10, 12]),
            ins(1000, 4, &[0]),
            mark(),
        ];
        assert_eq!(
            ways(&s),
            vec![
                Way::from([flag(10, true)]),
                Way::from([flag(11, true)]),
                Way::from([flag(12, true)])
            ]
        );
    }

    #[test]
    fn a_range_means_what_the_exe_evaluates() {
        // Each state, skipping past `end` when the test holds: the mark is reached
        // when it fails.
        let at = |state| {
            let s = [
                ins(1003, 3, &[bytes([1, state, 0]), 10, 11]),
                ins(1000, 4, &[0]),
                mark(),
            ];
            ways(&s)
        };
        let one = |f, h| Way::from([flag(f, h)]);
        let both = |h| Way::from([flag(10, h), flag(11, h)]);
        assert_eq!(at(0), vec![both(true)]);
        assert_eq!(at(1), vec![both(false)]);
        assert_eq!(at(2), vec![one(10, true), one(11, true)]);
        assert_eq!(at(3), vec![one(10, false), one(11, false)]);
    }

    #[test]
    fn a_reading_is_only_of_what_the_catalogue_has_read() {
        let of = |bank, id, words: &[u32]| other(&ins(bank, id, words)).test.reading();
        // A character's death, by its state.
        let dead = |state| of(4, 0, &[0, 77, state, 1.0f32.to_bits()]);
        assert_eq!(dead(1), Some(Reading::Dead(77)));
        assert_eq!(dead(0), None);
        // Goods by their category; weapons are not read.
        assert_eq!(
            of(3, 4, &[bytes([0, 3, 0]), 8, 1]),
            Some(Reading::Good { id: 8, held: true })
        );
        assert_eq!(
            of(3, 4, &[bytes([0, 3, 0]), 8, 0]),
            Some(Reading::Good { id: 8, held: false })
        );
        assert_eq!(of(3, 4, &[0, 8, 1]), None);
        assert_eq!(of(5, 2, &[0, 9]), Some(Reading::ObjectAction(9)));
        // A flag is no unread test, and the unknown stay unknown.
        assert_eq!(Test::Flag(1).reading(), None);
        assert_eq!(of(4, 3, &[0, 77]), None);
    }

    #[test]
    fn contradictions_close_a_path() {
        // Wait for flag 1, then skip the mark unless it is off.
        let s = [
            if_flag(0, true, 1),
            ins(1003, 1, &[bytes([1, 1, 0]), 1]),
            mark(),
        ];
        assert!(ways(&s).is_empty());
    }
}

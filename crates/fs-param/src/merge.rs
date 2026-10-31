//! Merging several mods' `regulation.bin` onto the game's own.
//!
//! This is the method the community's tools share (ERModsMerger, ERRT, ERBM,
//! and merging by hand in Smithbox), done natively:
//!
//! 1. Read the game's unmodified file — the *base*.
//! 2. For each mod, lowest priority first, find what it changed relative to the
//!    base: rows edited, rows added, rows removed.
//! 3. Apply every mod's changes to the base in that order. Where two mods set
//!    the same value differently, the later one wins **and the report says so**.
//!
//! # Granularity
//!
//! Without field definitions — Paramdex, which the other tools use, carries no
//! licence and cannot be shipped — a row is compared in aligned four-byte words.
//! Almost every field is a four-byte value on a four-byte boundary, so for
//! nearly all edits a word *is* a field. Where several small fields share a
//! word, two mods changing different ones of them are reported as a conflict
//! rather than combined: the merge can be stricter than a per-field one, never
//! looser, so it never produces a value neither mod wrote.
//!
//! # Mods made for another game version
//!
//! A mod's file is a whole copy of the table it was made against. If the game
//! has been patched since, every value the patch changed looks like an edit by
//! the mod. Where the two versions differ in *shape* — a table missing, a row a
//! different size, rows gone — no diff can tell the mod's work from the patch's,
//! and that mod is refused with the reason, rather than merged into a file the
//! game may not load. Where only values could have drifted, it is merged and a
//! warning says what that costs.
//!
//! Lifted from `apoc-fsparams` (`merge.rs`, Nova's own code), without its loose
//! Dark Souls III containers, which come back with that game.

use std::collections::{BTreeMap, HashMap};

use fs_bytes::{Error, Result};

use crate::param::{Param, Row};
use crate::regulation::{Regulation, display_version};

/// The unit rows are compared in.
const WORD: usize = 4;

/// One mod's copy of the container.
#[derive(Debug, Clone)]
pub struct Contribution {
    /// How the mod is named in the report.
    pub label: String,
    pub bytes: Vec<u8>,
}

/// Everything the merge did, for the interface to show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// The base file's version, as people write it (`1.17.1`).
    pub game_version: String,
    pub merged: Vec<MergedMod>,
    pub refused: Vec<RefusedMod>,
    pub conflicts: Vec<RowConflict>,
    pub warnings: Vec<String>,
}

impl MergeReport {
    /// The number of values settled by load order rather than combined.
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedMod {
    pub label: String,
    pub version: String,
    pub rows_changed: usize,
    pub rows_added: usize,
    pub rows_removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedMod {
    pub label: String,
    /// `None` when the file could not be read far enough to have one.
    pub version: Option<String>,
    /// A sentence someone can act on.
    pub reason: String,
}

/// What two mods disagreed about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConflictKind {
    /// Both set values in the same place in one row.
    Values,
    /// Both renamed the row.
    Name,
    /// Both added a row with this id, differently.
    AddedTwice,
    /// One removed the row and another edited it.
    RemovedAndEdited,
    /// One changed how many rows share a repeated id, and another changed
    /// those rows too. Rows sharing an id cannot be paired up between two
    /// mods once their number differs, so the later mod's set is used whole.
    DuplicateRun,
}

/// One row two or more mods disagreed about, and who won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowConflict {
    /// The table, e.g. `EquipParamWeapon`.
    pub param: String,
    pub row_id: i32,
    /// Which row with this id, counting from 0. The game's own tables repeat
    /// ids; without this, conflicts on two different rows would read as one.
    pub occurrence: u32,
    pub kind: ConflictKind,
    /// The mod whose value is used: the highest in load order.
    pub winner: String,
    /// The mods whose values were replaced.
    pub overridden: Vec<String>,
    /// Byte offsets within the row, for [`ConflictKind::Values`].
    pub offsets: Vec<u32>,
}

/// The merged container, and what went into it.
#[derive(Debug, Clone)]
pub struct MergeOutcome {
    /// The file to deploy, or `None` when no mod could be merged — in which
    /// case the game's own file is the right one to use, and deploying nothing
    /// leaves it in place.
    pub bytes: Option<Vec<u8>>,
    pub report: MergeReport,
}

/// A row's place: id, and which occurrence of that id. The game's own file
/// repeats ids (`RandomAppearParam`), so an id alone is not a key.
type Key = (i32, u32);

fn keys(rows: &[Row]) -> Vec<Key> {
    let mut seen: HashMap<i32, u32> = HashMap::new();
    rows.iter()
        .map(|r| {
            let n = seen.entry(r.id).or_insert(0);
            let key = (r.id, *n);
            *n += 1;
            key
        })
        .collect()
}

/// The words of one row a mod changed: word index, and the new bytes.
type WordEdits = Vec<(usize, Vec<u8>)>;

/// A row's name: UTF-16 units, or none at all.
type RowName = Option<Vec<u16>>;

/// What one mod did to one table.
#[derive(Debug, Default)]
struct TableDiff {
    /// Base row index, the words changed, and a new name.
    edits: Vec<(usize, WordEdits, Option<RowName>)>,
    adds: Vec<(Key, Row)>,
    removes: Vec<usize>,
    /// Repeated ids whose number of rows this mod changed: its rows for each,
    /// whole. See [`diff_table`].
    runs: Vec<(i32, Vec<Row>)>,
}

fn changed_words(base: &[u8], theirs: &[u8]) -> Vec<(usize, Vec<u8>)> {
    base.chunks(WORD)
        .zip(theirs.chunks(WORD))
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(w, (_, b))| (w, b.to_vec()))
        .collect()
}

fn diff_table(name: &str, base: &Param, theirs: &Param) -> std::result::Result<TableDiff, String> {
    if base.type_name != theirs.type_name {
        return Err(format!("its {name} table is a different kind of table"));
    }
    if base.row_size != theirs.row_size {
        return Err(format!(
            "its {name} rows are {} bytes where this game's are {}",
            theirs.row_size, base.row_size
        ));
    }
    // A repeated id cannot be matched row for row once a mod changes how many
    // rows carry it. Pairing by occurrence would read one row inserted at the
    // front as an edit to every row after it, and another mod's edit to one of
    // those would then land on the wrong row — making a row neither mod wrote.
    // Such an id is taken as a unit instead: this mod's rows for it, whole.
    let count = |rows: &[Row]| {
        let mut c: HashMap<i32, u32> = HashMap::new();
        for r in rows {
            *c.entry(r.id).or_insert(0) += 1;
        }
        c
    };
    let (base_count, their_count) = (count(&base.rows), count(&theirs.rows));
    let mut runs: BTreeMap<i32, Vec<Row>> = BTreeMap::new();
    for &id in base_count.keys().chain(their_count.keys()) {
        let b = base_count.get(&id).copied().unwrap_or(0);
        let t = their_count.get(&id).copied().unwrap_or(0);
        if b != t && b.max(t) > 1 {
            runs.entry(id).or_default();
        }
    }

    let base_keys = keys(&base.rows);
    let index: HashMap<Key, usize> = base_keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    // A base row whose id is taken whole is accounted for by the run, not
    // removed.
    let mut seen: Vec<bool> = base.rows.iter().map(|r| runs.contains_key(&r.id)).collect();
    let mut diff = TableDiff::default();
    for (key, row) in keys(&theirs.rows).into_iter().zip(&theirs.rows) {
        if let Some(run) = runs.get_mut(&row.id) {
            run.push(row.clone());
            continue;
        }
        match index.get(&key) {
            Some(&i) => {
                seen[i] = true;
                let b = &base.rows[i];
                let words = changed_words(&b.data, &row.data);
                let name = (b.name != row.name).then(|| row.name.clone());
                if !words.is_empty() || name.is_some() {
                    diff.edits.push((i, words, name));
                }
            }
            None => diff.adds.push((key, row.clone())),
        }
    }
    diff.removes = seen
        .iter()
        .enumerate()
        .filter(|(_, s)| !**s)
        .map(|(i, _)| i)
        .collect();
    diff.runs = runs.into_iter().collect();
    Ok(diff)
}

/// The base, parsed once.
struct Base {
    reg: Regulation,
    /// `None` for a table this cannot parse; such a table is merged whole.
    parsed: Vec<Option<Param>>,
}

/// What one mod did to the whole container.
#[derive(Default)]
struct ModDiff {
    tables: BTreeMap<usize, TableDiff>,
    /// Tables the base's parser could not read, which this mod changed. They
    /// can only be taken whole.
    whole: Vec<(usize, Vec<u8>)>,
    changed: usize,
    added: usize,
    removed: usize,
}

/// Whether a stamp is a game version: eight digits, as Elden Ring's
/// (`11711000`) and Dark Souls III's `Data0.bdt` (`01350000`) are.
fn is_version_stamp(stamp: &str) -> bool {
    stamp.len() == 8 && stamp.bytes().all(|b| b.is_ascii_digit())
}

/// A binder entry's name without its directory; a table always has one.
fn short(f: &fs_bnd4::File) -> &str {
    f.short_name().unwrap_or("")
}

fn diff_regulation(base: &Base, theirs: &Regulation) -> std::result::Result<ModDiff, String> {
    let by_name: HashMap<&str, &fs_bnd4::File> =
        theirs.bnd.files.iter().map(|f| (short(f), f)).collect();
    let base_names: std::collections::HashSet<&str> =
        base.reg.bnd.files.iter().map(short).collect();
    if let Some(extra) = theirs
        .bnd
        .files
        .iter()
        .map(short)
        .find(|n| !base_names.contains(n))
    {
        return Err(format!(
            "it has a {} table this game does not",
            table_name(extra)
        ));
    }

    let mut out = ModDiff::default();
    for (i, f) in base.reg.bnd.files.iter().enumerate() {
        let file = short(f);
        let Some(tf) = by_name.get(file) else {
            return Err(format!(
                "it has no {} table, which this game needs",
                table_name(file)
            ));
        };
        // Most tables in most mods are untouched copies; comparing bytes first
        // skips parsing them at all.
        if tf.data == f.data {
            continue;
        }
        match &base.parsed[i] {
            Some(bp) => {
                let tp = Param::read(&tf.data).map_err(|e| {
                    format!("its {} table could not be read ({e})", table_name(file))
                })?;
                let d = diff_table(table_name(file), bp, &tp)?;
                out.changed += d.edits.len() + d.runs.len();
                out.added += d.adds.len();
                out.removed += d.removes.len();
                if !(d.edits.is_empty()
                    && d.adds.is_empty()
                    && d.removes.is_empty()
                    && d.runs.is_empty())
                {
                    out.tables.insert(i, d);
                }
            }
            None => out.whole.push((i, tf.data.clone())),
        }
    }
    Ok(out)
}

/// `AtkParam_Pc.param` → `AtkParam_Pc`.
fn table_name(file: &str) -> &str {
    file.strip_suffix(".param").unwrap_or(file)
}

/// A base row as it stands mid-merge, with who last set each part of it.
struct Slot {
    row: Row,
    word_owner: HashMap<usize, usize>,
    name_owner: Option<usize>,
}

impl Slot {
    fn fresh(row: &Row) -> Self {
        Slot {
            row: row.clone(),
            word_owner: HashMap::new(),
            name_owner: None,
        }
    }
    fn owners(&self) -> Vec<usize> {
        let mut o: Vec<usize> = self.word_owner.values().copied().collect();
        o.extend(self.name_owner);
        o.sort_unstable();
        o.dedup();
        o
    }
}

/// One table as it stands mid-merge.
struct Work {
    rows: Vec<Option<Slot>>,
    removed_by: Vec<Option<usize>>,
    adds: Vec<(Key, Row, usize)>,
    /// Repeated ids replaced whole, with the mod that replaced them.
    runs: BTreeMap<i32, (Vec<Row>, usize)>,
    /// Each base row's key, to name it in a conflict.
    keys: Vec<Key>,
}

impl Work {
    fn new(base: &Param) -> Self {
        Work {
            rows: base.rows.iter().map(|r| Some(Slot::fresh(r))).collect(),
            removed_by: vec![None; base.rows.len()],
            adds: Vec::new(),
            runs: BTreeMap::new(),
            keys: keys(&base.rows),
        }
    }
}

/// A later mod touching an id an earlier one replaced whole takes it back
/// from the run: load order decides, and the report says so.
fn drop_run(
    work: &mut Work,
    conflicts: &mut Conflicts,
    table: &str,
    id: i32,
    mi: usize,
    labels: &[&str],
) {
    if let Some((_, owner)) = work.runs.remove(&id) {
        if owner != mi {
            conflicts.note(
                table,
                (id, 0),
                ConflictKind::DuplicateRun,
                labels[mi],
                &[labels[owner]],
                None,
            );
        }
    }
}

/// Conflicts, one entry per table, row and kind.
#[derive(Default)]
struct Conflicts(BTreeMap<(String, Key, ConflictKind), RowConflict>);

impl Conflicts {
    fn note(
        &mut self,
        table: &str,
        key: Key,
        kind: ConflictKind,
        winner: &str,
        overridden: &[&str],
        offset: Option<u32>,
    ) {
        let c = self
            .0
            .entry((table.to_string(), key, kind))
            .or_insert_with(|| RowConflict {
                param: table.to_string(),
                row_id: key.0,
                occurrence: key.1,
                kind,
                winner: winner.to_string(),
                overridden: Vec::new(),
                offsets: Vec::new(),
            });
        c.winner = winner.to_string();
        for o in overridden {
            if *o != winner && !c.overridden.iter().any(|x| x == o) {
                c.overridden.push(o.to_string());
            }
        }
        c.overridden.retain(|o| o != winner);
        if let Some(off) = offset {
            if !c.offsets.contains(&off) {
                c.offsets.push(off);
            }
        }
    }
}

/// Merge `mods`, lowest priority first, onto `base`.
///
/// Fails only when the base itself cannot be read or the result cannot be
/// written; a mod that cannot be merged is refused in the report and the rest
/// go ahead without it.
pub fn merge(key: &[u8; 32], base: &[u8], mods: &[Contribution]) -> Result<MergeOutcome> {
    let reg = Regulation::read(key, base)?;
    let parsed = reg
        .bnd
        .files
        .iter()
        .map(|f| Param::read(&f.data).ok())
        .collect();
    let base = Base { reg, parsed };
    let base_version = base.reg.version();

    let read: Vec<Result<Regulation>> = mods
        .iter()
        .map(|m| Regulation::read(key, &m.bytes))
        .collect();
    let mut report = MergeReport {
        game_version: display_version(&base_version),
        ..Default::default()
    };

    // Diff every mod first, so a refusal is decided before anything is merged.
    struct Prepared {
        version: String,
        drifted: bool,
        diffed: std::result::Result<ModDiff, String>,
    }
    let prepared: Vec<Result<Prepared>> = read
        .into_iter()
        .map(|r| {
            r.map(|theirs| {
                let version = theirs.version();
                // Only a stamp that is a game version can say the mod was
                // made for another one. Dark Souls III's loose gameparam
                // carries what looks like a build date (`21L18F17`) instead;
                // comparing that with the game's would call every such mod
                // outdated, and refuse any that removes rows. Shape is still
                // checked either way.
                let drifted = is_version_stamp(&version)
                    && is_version_stamp(&base_version)
                    && version != base_version;
                let diffed = diff_regulation(&base, &theirs);
                Prepared {
                    version,
                    drifted,
                    diffed,
                }
            })
        })
        .collect();

    // The game's tables that could not be parsed are merged whole, which is
    // worth saying.
    for (i, f) in base.reg.bnd.files.iter().enumerate() {
        if base.parsed[i].is_none() {
            report.warnings.push(format!(
                "The game's {} table could not be read value by value, so if more than one \
                 mod changes it, the one highest in the load order is used whole.",
                table_name(short(f))
            ));
        }
    }

    let mut accepted: Vec<(usize, ModDiff)> = Vec::new();
    for (mi, (m, p)) in mods.iter().zip(prepared).enumerate() {
        let Prepared {
            version,
            drifted,
            diffed,
        } = match p {
            Ok(p) => p,
            Err(e) => {
                report.refused.push(RefusedMod {
                    label: m.label.clone(),
                    version: None,
                    reason: format!("Its gameplay file could not be read ({e})."),
                });
                continue;
            }
        };
        let shown = display_version(&version);
        let for_version = format!(
            "It was made for game version {shown}, and this game is {}",
            report.game_version
        );
        match diffed {
            Err(why) => {
                let reason = if drifted {
                    format!("{}: {why}. An update of the mod is needed.", for_version)
                } else {
                    format!("Its gameplay file does not match this game's: {why}.")
                };
                report.refused.push(RefusedMod {
                    label: m.label.clone(),
                    version: Some(shown),
                    reason,
                });
            }
            Ok(d) if drifted && d.removed > 0 => {
                report.refused.push(RefusedMod {
                    label: m.label.clone(),
                    version: Some(shown),
                    reason: format!(
                        "{}, and {} rows this game has are missing from its file. A file \
                         from an older version looks like that, and so does a mod that \
                         deletes rows; the two cannot be told apart, and merging would \
                         delete those rows from this game either way. An update of the mod \
                         is needed.",
                        for_version, d.removed
                    ),
                });
            }
            Ok(d) => {
                if drifted {
                    report.warnings.push(format!(
                        "{} was made for game version {shown}, and this game is {}. Every \
                         value in its file that differs from this game's is treated as the \
                         mod's own change, so where the update changed a value, {}'s older \
                         one is used unless a mod higher in the load order sets it too.",
                        m.label, report.game_version, m.label
                    ));
                } else if version != base_version && !is_version_stamp(&version) {
                    // A stamp that is not a version cannot say the mod is
                    // outdated, and cannot say it is not: values an update
                    // changed would pass every shape check.
                    report.warnings.push(format!(
                        "{}'s gameplay file does not say which game version it was made for. \
                         Every value in it that differs from this game's is treated as the \
                         mod's own change, so if it was made before an update that changed \
                         values, its older ones are used unless a mod higher in the load order \
                         sets them too.",
                        m.label
                    ));
                }
                report.merged.push(MergedMod {
                    label: m.label.clone(),
                    version: shown,
                    rows_changed: d.changed,
                    rows_added: d.added,
                    rows_removed: d.removed,
                });
                accepted.push((mi, d));
            }
        }
    }

    if accepted.is_empty() {
        return Ok(MergeOutcome {
            bytes: None,
            report,
        });
    }

    let labels: Vec<&str> = mods.iter().map(|m| m.label.as_str()).collect();
    let label = |mi: usize| labels[mi];
    let mut works: BTreeMap<usize, Work> = BTreeMap::new();
    let mut whole: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    let mut conflicts = Conflicts::default();

    for (mi, diff) in &accepted {
        let mi = *mi;
        for (&t, d) in &diff.tables {
            let bp = base.parsed[t].as_ref().expect("diffed tables were parsed");
            let tname = table_name(short(&base.reg.bnd.files[t])).to_string();
            let work = works.entry(t).or_insert_with(|| Work::new(bp));

            for (i, words, name) in &d.edits {
                let i = *i;
                let key = work.keys[i];
                drop_run(work, &mut conflicts, &tname, key.0, mi, &labels);
                if work.rows[i].is_none() {
                    let remover = work.removed_by[i].expect("a removed row records who removed it");
                    conflicts.note(
                        &tname,
                        key,
                        ConflictKind::RemovedAndEdited,
                        label(mi),
                        &[label(remover)],
                        None,
                    );
                    work.rows[i] = Some(Slot::fresh(&bp.rows[i]));
                    work.removed_by[i] = None;
                }
                let slot = work.rows[i].as_mut().expect("restored above");
                for (w, bytes) in words {
                    let at = w * WORD;
                    let current = &slot.row.data[at..at + bytes.len()];
                    if let Some(&owner) = slot.word_owner.get(w) {
                        if current != bytes.as_slice() {
                            conflicts.note(
                                &tname,
                                key,
                                ConflictKind::Values,
                                label(mi),
                                &[label(owner)],
                                Some(at as u32),
                            );
                        }
                    }
                    slot.row.data[at..at + bytes.len()].copy_from_slice(bytes);
                    slot.word_owner.insert(*w, mi);
                }
                if let Some(name) = name {
                    if let Some(owner) = slot.name_owner {
                        if &slot.row.name != name {
                            conflicts.note(
                                &tname,
                                key,
                                ConflictKind::Name,
                                label(mi),
                                &[label(owner)],
                                None,
                            );
                        }
                    }
                    slot.row.name = name.clone();
                    slot.name_owner = Some(mi);
                }
            }

            for &i in &d.removes {
                let key = work.keys[i];
                drop_run(work, &mut conflicts, &tname, key.0, mi, &labels);
                if let Some(slot) = &work.rows[i] {
                    let owners = slot.owners();
                    if !owners.is_empty() {
                        let names: Vec<&str> = owners.iter().map(|&o| label(o)).collect();
                        conflicts.note(
                            &tname,
                            key,
                            ConflictKind::RemovedAndEdited,
                            label(mi),
                            &names,
                            None,
                        );
                    }
                    work.rows[i] = None;
                    work.removed_by[i] = Some(mi);
                }
            }

            for (k, row) in &d.adds {
                drop_run(work, &mut conflicts, &tname, k.0, mi, &labels);
                match work.adds.iter().position(|(ak, _, _)| ak == k) {
                    Some(pos) => {
                        let (_, existing, owner) = &work.adds[pos];
                        if existing != row {
                            conflicts.note(
                                &tname,
                                *k,
                                ConflictKind::AddedTwice,
                                label(mi),
                                &[label(*owner)],
                                None,
                            );
                        }
                        work.adds[pos] = (*k, row.clone(), mi);
                    }
                    None => work.adds.push((*k, row.clone(), mi)),
                }
            }

            // An id this mod replaced whole overrides whatever earlier mods did
            // to any row carrying it — edits, removals, additions — and says so.
            for (id, rows) in &d.runs {
                let id = *id;
                let mut overridden: Vec<usize> = Vec::new();
                if let Some((prev, owner)) = work.runs.get(&id) {
                    if prev != rows {
                        overridden.push(*owner);
                    }
                }
                for (i, k) in work.keys.iter().enumerate() {
                    if k.0 != id {
                        continue;
                    }
                    match &work.rows[i] {
                        Some(slot) => overridden.extend(slot.owners()),
                        None => overridden.extend(work.removed_by[i]),
                    }
                    // Back to the game's own row. The run replaces all of this
                    // id; a later mod taking the id back must start from the
                    // game's rows, not revive what this run overrode.
                    work.rows[i] = Some(Slot::fresh(&bp.rows[i]));
                    work.removed_by[i] = None;
                }
                overridden.extend(
                    work.adds
                        .iter()
                        .filter(|(k, _, _)| k.0 == id)
                        .map(|(_, _, owner)| *owner),
                );
                work.adds.retain(|(k, _, _)| k.0 != id);
                overridden.retain(|&o| o != mi);
                overridden.sort_unstable();
                overridden.dedup();
                if !overridden.is_empty() {
                    let names: Vec<&str> = overridden.iter().map(|&o| label(o)).collect();
                    conflicts.note(
                        &tname,
                        (id, 0),
                        ConflictKind::DuplicateRun,
                        label(mi),
                        &names,
                        None,
                    );
                }
                work.runs.insert(id, (rows.clone(), mi));
            }
        }

        for (t, bytes) in &diff.whole {
            let tname = table_name(short(&base.reg.bnd.files[*t]));
            if let Some((prev, prev_bytes)) = whole.get(t) {
                if prev_bytes != bytes {
                    report.warnings.push(format!(
                        "{} and {} both change the {tname} table, which could not be merged \
                         value by value; {}'s is used whole.",
                        label(*prev),
                        label(mi),
                        label(mi)
                    ));
                }
            }
            whole.insert(*t, (mi, bytes.clone()));
        }
    }

    // Assemble. Only tables some mod changed are rewritten; every other one is
    // the game's own bytes, untouched.
    let mut merged: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    let mut expected_rows: Vec<(String, usize)> = Vec::new();
    for (t, work) in works {
        let bp = base.parsed[t].as_ref().expect("merged tables were parsed");
        let kept: Vec<Row> = work.rows.into_iter().flatten().map(|s| s.row).collect();
        // A replaced id's rows go where the game's first remaining row with
        // that id was; one no remaining row carries goes in by id, below.
        let replaced: std::collections::HashSet<i32> = work.runs.keys().copied().collect();
        let mut runs = work.runs;
        let mut rows: Vec<Row> = Vec::with_capacity(kept.len());
        for r in kept {
            if replaced.contains(&r.id) {
                if let Some((run, _)) = runs.remove(&r.id) {
                    rows.extend(run);
                }
                continue;
            }
            rows.push(r);
        }
        let mut adds = work.adds;
        for (id, (run, owner)) in runs {
            for (n, row) in run.into_iter().enumerate() {
                adds.push(((id, n as u32), row, owner));
            }
        }
        adds.sort_by_key(|(k, _, _)| *k);
        for (_, row, _) in adds {
            // After every row with an id no greater than this one: sorted order
            // where the game's table is sorted, and a stable place where it is
            // not.
            let at = rows
                .iter()
                .position(|r| r.id > row.id)
                .unwrap_or(rows.len());
            rows.insert(at, row);
        }
        expected_rows.push((short(&base.reg.bnd.files[t]).to_string(), rows.len()));
        let mut table = bp.clone();
        table.rows = rows;
        merged.insert(t, table.write()?);
    }
    for (t, (_, bytes)) in whole {
        merged.insert(t, bytes);
    }
    let mut out = base.reg.clone();
    for (t, data) in merged {
        out.bnd.files[t].data = data;
    }
    let bytes = out.write()?;

    // Read back what is about to be deployed. A merge that wrote something the
    // reader cannot parse, or that lost rows on the way, must fail here rather
    // than at the game's title screen.
    let check = Regulation::read(key, &bytes)?;
    for (name, rows) in expected_rows {
        let file = check
            .bnd
            .files
            .iter()
            .find(|f| short(f) == name)
            .ok_or_else(|| {
                Error::Format(format!(
                    "the merged {} table is missing from the result",
                    table_name(&name)
                ))
            })?;
        let got = Param::read(&file.data)?;
        if got.rows.len() != rows {
            return Err(Error::Format(format!(
                "the merged {} table read back with {} rows instead of {rows}",
                table_name(&name),
                got.rows.len()
            )));
        }
    }

    report.conflicts = conflicts.0.into_values().collect();
    Ok(MergeOutcome {
        bytes: Some(bytes),
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::Row;
    use fs_bnd4::{Bnd4, File, Header};

    const KEY: [u8; 32] = [0x5A; 32];

    fn row<const N: usize>(id: i32, data: [u8; N]) -> Row {
        Row {
            id,
            data: data.to_vec(),
            name: Some(Vec::new()),
        }
    }

    fn table<const N: usize>(type_name: &str, row_size: usize, rows: &[(i32, [u8; N])]) -> Param {
        Param::synthesize(
            type_name,
            row_size,
            rows.iter().map(|&(id, d)| row(id, d)).collect(),
        )
    }

    /// A whole encrypted file: every layer is the real writer's.
    fn regulation(version: &str, tables: Vec<(&str, Param)>) -> Vec<u8> {
        let mut stamp = [0u8; 8];
        stamp[..version.len()].copy_from_slice(version.as_bytes());
        let files = tables
            .into_iter()
            .enumerate()
            .map(|(i, (name, p))| File {
                flags: 0x40,
                id: Some(i as i32),
                name: Some(format!(r"N:\GR\data\Param\param\GameParam\{name}.param")),
                data: p.write().expect("a test table writes"),
            })
            .collect();
        let bnd = Bnd4 {
            header: Header {
                unk04: 0,
                unk05: 0,
                unk0a: 1,
                version: stamp,
                raw_format: 0x74,
                hash_table: true,
                unicode: true,
            },
            files,
        };
        Regulation::from_parts(KEY, bnd)
            .write()
            .expect("a test file writes")
    }

    fn contribution(label: &str, bytes: Vec<u8>) -> Contribution {
        Contribution {
            label: label.into(),
            bytes,
        }
    }

    fn read_table(bytes: &[u8], name: &str) -> Param {
        Regulation::read(&KEY, bytes)
            .expect("the merged file reads")
            .table(name)
            .expect("the table parses")
    }

    fn base() -> Vec<u8> {
        regulation(
            "11711000",
            vec![
                (
                    "AtkParam",
                    table("ATK_ST", 8, &[(1, [1; 8]), (2, [2; 8]), (3, [3; 8])]),
                ),
                (
                    "ShopParam",
                    table("SHOP_ST", 8, &[(10, [0; 8]), (20, [0; 8])]),
                ),
            ],
        )
    }

    /// A mod: the base with some rows of `AtkParam` replaced.
    fn edits(version: &str, atk: &[(i32, [u8; 8])]) -> Vec<u8> {
        regulation(
            version,
            vec![
                ("AtkParam", table("ATK_ST", 8, atk)),
                (
                    "ShopParam",
                    table("SHOP_ST", 8, &[(10, [0; 8]), (20, [0; 8])]),
                ),
            ],
        )
    }

    fn run(mods: Vec<Contribution>) -> MergeOutcome {
        merge(&KEY, &base(), &mods).expect("the merge runs")
    }

    /// A single-row table whose strings offset points inside its row reads a
    /// row shorter than the game's. The reader takes that offset as the row's
    /// end, so this comparison is the guard: such a file is refused rather than
    /// merged as a truncated row.
    #[test]
    fn a_single_row_table_read_short_by_a_misleading_offset_is_refused() {
        let base = regulation(
            "11711000",
            vec![("ONE", table("ONE_ST", 12, &[(0, [7u8; 12])]))],
        );

        let mut bytes = table("ONE_ST", 12, &[(0, [9u8; 12])]).write().unwrap();
        let data_at = i64::from_le_bytes(bytes[0x48..0x50].try_into().unwrap()) as usize;
        bytes[0x00..0x04].copy_from_slice(&((data_at + 8) as u32).to_le_bytes());
        let short = Param::read(&bytes).unwrap();
        assert_eq!(short.row_size, 8, "the misleading offset shortens the row");
        let modded = regulation("11711000", vec![("ONE", short)]);

        let out = merge(&KEY, &base, &[contribution("Misleading", modded)]).unwrap();
        assert!(out.report.merged.is_empty(), "{:?}", out.report);
        let refused = out.report.refused.first().expect("the mod is refused");
        assert!(
            refused
                .reason
                .contains("rows are 8 bytes where this game's are 12"),
            "{}",
            refused.reason
        );
    }

    fn dup_base() -> Vec<u8> {
        regulation(
            "11711000",
            vec![(
                "Dup",
                table("DUP_ST", 4, &[(7, [1; 4]), (7, [2; 4]), (8, [3; 4])]),
            )],
        )
    }

    fn dup(rows: &[(i32, [u8; 4])]) -> Vec<u8> {
        regulation("11711000", vec![("Dup", table("DUP_ST", 4, rows))])
    }

    fn dup_rows(out: &MergeOutcome) -> Vec<(i32, Vec<u8>)> {
        read_table(out.bytes.as_ref().unwrap(), "Dup")
            .rows
            .into_iter()
            .map(|r| (r.id, r.data))
            .collect()
    }

    /// One mod inserts a row in front of a repeated id; another edits one of
    /// that id's rows. Paired by occurrence, the edit would land on the
    /// inserted row and make a row neither mod wrote. The id is a unit
    /// instead: load order decides, whole, and the report says so.
    #[test]
    fn inserting_into_a_repeated_id_is_taken_whole_rather_than_misaligned() {
        let inserter = dup(&[(7, [9; 4]), (7, [1; 4]), (7, [2; 4]), (8, [3; 4])]);
        let editor = dup(&[(7, [5; 4]), (7, [2; 4]), (8, [3; 4])]);

        // Editor last: the game's two rows, with the editor's change.
        let out = merge(
            &KEY,
            &dup_base(),
            &[
                contribution("Inserter", inserter.clone()),
                contribution("Editor", editor.clone()),
            ],
        )
        .unwrap();
        assert_eq!(
            dup_rows(&out),
            vec![(7, vec![5; 4]), (7, vec![2; 4]), (8, vec![3; 4])]
        );
        assert_eq!(out.report.conflicts[0].kind, ConflictKind::DuplicateRun);
        assert_eq!(out.report.conflicts[0].winner, "Editor");

        // Inserter last: its three rows exactly, with no trace of the edit.
        let out = merge(
            &KEY,
            &dup_base(),
            &[
                contribution("Editor", editor),
                contribution("Inserter", inserter),
            ],
        )
        .unwrap();
        assert_eq!(
            dup_rows(&out),
            vec![
                (7, vec![9; 4]),
                (7, vec![1; 4]),
                (7, vec![2; 4]),
                (8, vec![3; 4])
            ]
        );
        assert_eq!(out.report.conflicts[0].winner, "Inserter");
        assert_eq!(
            out.report.conflicts[0].overridden,
            vec!["Editor".to_string()]
        );
    }

    /// Codex's second case: A edits a repeated id's row, B replaces that id
    /// whole, C takes it back by editing another of its rows. B overrode A, so
    /// A's edit must not come back when C drops B's replacement.
    #[test]
    fn an_edit_a_replacement_overrode_does_not_come_back_with_a_later_mod() {
        let a = dup(&[(7, [5; 4]), (7, [2; 4]), (8, [3; 4])]);
        let b = dup(&[(7, [9; 4]), (7, [1; 4]), (7, [2; 4]), (8, [3; 4])]);
        let c = dup(&[(7, [1; 4]), (7, [6; 4]), (8, [3; 4])]);
        let out = merge(
            &KEY,
            &dup_base(),
            &[
                contribution("A", a),
                contribution("B", b),
                contribution("C", c),
            ],
        )
        .unwrap();
        assert_eq!(
            dup_rows(&out),
            vec![(7, vec![1; 4]), (7, vec![6; 4]), (8, vec![3; 4])],
            "the game's rows with C's edit, and nothing of A's"
        );
        assert!(
            out.report
                .conflicts
                .iter()
                .all(|c| c.kind == ConflictKind::DuplicateRun)
        );
    }

    /// A mod that only inserts into a repeated id, with nobody else touching
    /// it, gets exactly its rows and no conflict.
    #[test]
    fn a_lone_insertion_into_a_repeated_id_is_kept_as_written() {
        let inserter = dup(&[(7, [1; 4]), (7, [9; 4]), (7, [2; 4]), (8, [3; 4])]);
        let out = merge(&KEY, &dup_base(), &[contribution("Inserter", inserter)]).unwrap();
        assert_eq!(
            dup_rows(&out),
            vec![
                (7, vec![1; 4]),
                (7, vec![9; 4]),
                (7, vec![2; 4]),
                (8, vec![3; 4])
            ]
        );
        assert!(out.report.conflicts.is_empty());
    }

    #[test]
    fn a_conflict_on_the_second_of_a_repeated_id_names_that_occurrence() {
        let a = dup(&[(7, [1; 4]), (7, [5; 4]), (8, [3; 4])]);
        let b = dup(&[(7, [1; 4]), (7, [6; 4]), (8, [3; 4])]);
        let out = merge(
            &KEY,
            &dup_base(),
            &[contribution("A", a), contribution("B", b)],
        )
        .unwrap();
        assert_eq!(out.report.conflicts.len(), 1);
        assert_eq!(out.report.conflicts[0].row_id, 7);
        assert_eq!(out.report.conflicts[0].occurrence, 1);
    }

    #[test]
    fn changes_to_different_rows_are_both_kept_with_no_conflict() {
        let a = edits("11711000", &[(1, [9; 8]), (2, [2; 8]), (3, [3; 8])]);
        let b = edits("11711000", &[(1, [1; 8]), (2, [2; 8]), (3, [7; 8])]);
        let out = run(vec![contribution("A", a), contribution("B", b)]);

        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        assert_eq!(atk.rows[0].data, vec![9; 8]);
        assert_eq!(atk.rows[2].data, vec![7; 8]);
        assert!(
            out.report.conflicts.is_empty(),
            "{:?}",
            out.report.conflicts
        );
        assert_eq!(out.report.merged.len(), 2);
    }

    #[test]
    fn different_words_of_one_row_combine() {
        let a = edits(
            "11711000",
            &[(1, [5, 5, 5, 5, 1, 1, 1, 1]), (2, [2; 8]), (3, [3; 8])],
        );
        let b = edits(
            "11711000",
            &[(1, [1, 1, 1, 1, 6, 6, 6, 6]), (2, [2; 8]), (3, [3; 8])],
        );
        let out = run(vec![contribution("A", a), contribution("B", b)]);

        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        assert_eq!(atk.rows[0].data, vec![5, 5, 5, 5, 6, 6, 6, 6]);
        assert!(out.report.conflicts.is_empty());
    }

    /// The case merging exists to report: both set one value. The later mod
    /// wins, and the report names both and where.
    #[test]
    fn the_same_value_set_twice_goes_to_the_later_mod_and_is_reported() {
        let a = edits(
            "11711000",
            &[(1, [5, 5, 5, 5, 1, 1, 1, 1]), (2, [2; 8]), (3, [3; 8])],
        );
        let b = edits(
            "11711000",
            &[(1, [6, 6, 6, 6, 1, 1, 1, 1]), (2, [2; 8]), (3, [3; 8])],
        );
        let out = run(vec![contribution("A", a), contribution("B", b)]);

        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        assert_eq!(atk.rows[0].data, vec![6, 6, 6, 6, 1, 1, 1, 1]);
        assert_eq!(
            out.report.conflicts,
            vec![RowConflict {
                param: "AtkParam".into(),
                row_id: 1,
                occurrence: 0,
                kind: ConflictKind::Values,
                winner: "B".into(),
                overridden: vec!["A".into()],
                offsets: vec![0],
            }]
        );
    }

    #[test]
    fn agreeing_on_a_value_is_not_a_conflict() {
        let a = edits("11711000", &[(1, [5; 8]), (2, [2; 8]), (3, [3; 8])]);
        let out = run(vec![contribution("A", a.clone()), contribution("B", a)]);
        assert!(out.report.conflicts.is_empty());
    }

    /// Two small fields sharing a word cannot be told apart without field
    /// definitions. Combining them could make a value neither mod wrote, so it
    /// is a conflict — stricter than a per-field merge, never looser.
    #[test]
    fn two_small_fields_in_one_word_are_a_conflict_not_a_blend() {
        let a = edits(
            "11711000",
            &[(1, [9, 1, 1, 1, 1, 1, 1, 1]), (2, [2; 8]), (3, [3; 8])],
        );
        let b = edits(
            "11711000",
            &[(1, [1, 8, 1, 1, 1, 1, 1, 1]), (2, [2; 8]), (3, [3; 8])],
        );
        let out = run(vec![contribution("A", a), contribution("B", b)]);

        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        assert_eq!(
            atk.rows[0].data,
            vec![1, 8, 1, 1, 1, 1, 1, 1],
            "B's word, whole"
        );
        assert_eq!(out.report.conflicts.len(), 1);
    }

    #[test]
    fn added_rows_land_in_id_order_and_a_disagreement_is_reported() {
        let a = edits(
            "11711000",
            &[(1, [1; 8]), (2, [2; 8]), (3, [3; 8]), (5, [50; 8])],
        );
        let b = edits(
            "11711000",
            &[
                (1, [1; 8]),
                (2, [2; 8]),
                (3, [3; 8]),
                (4, [40; 8]),
                (5, [51; 8]),
            ],
        );
        let out = run(vec![contribution("A", a), contribution("B", b)]);

        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        let ids: Vec<i32> = atk.rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
        assert_eq!(atk.rows[4].data, vec![51; 8]);
        assert_eq!(out.report.conflicts[0].kind, ConflictKind::AddedTwice);
    }

    #[test]
    fn a_row_one_mod_removes_and_another_edits_is_decided_by_order_and_reported() {
        let removes = edits("11711000", &[(1, [1; 8]), (3, [3; 8])]);
        let changes = edits("11711000", &[(1, [1; 8]), (2, [9; 8]), (3, [3; 8])]);

        // Edited last: the row survives with the edit.
        let out = run(vec![
            contribution("Remover", removes.clone()),
            contribution("Editor", changes.clone()),
        ]);
        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        assert_eq!(atk.rows.len(), 3);
        assert_eq!(atk.rows[1].data, vec![9; 8]);
        assert_eq!(out.report.conflicts[0].kind, ConflictKind::RemovedAndEdited);
        assert_eq!(out.report.conflicts[0].winner, "Editor");

        // Removed last: the row goes.
        let out = run(vec![
            contribution("Editor", changes),
            contribution("Remover", removes),
        ]);
        let atk = read_table(out.bytes.as_ref().unwrap(), "AtkParam");
        assert_eq!(
            atk.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(out.report.conflicts[0].winner, "Remover");
    }

    /// The game's own `RandomAppearParam` repeats ids. Keyed by id alone, an
    /// edit to the second row with an id would land on the first.
    #[test]
    fn a_repeated_id_is_told_apart_by_its_occurrence() {
        let base = regulation(
            "11711000",
            vec![("Dup", table("DUP_ST", 4, &[(7, [1; 4]), (7, [2; 4])]))],
        );
        let theirs = regulation(
            "11711000",
            vec![("Dup", table("DUP_ST", 4, &[(7, [1; 4]), (7, [9; 4])]))],
        );
        let out = merge(&KEY, &base, &[contribution("A", theirs)]).unwrap();
        let dup = read_table(out.bytes.as_ref().unwrap(), "Dup");
        assert_eq!(dup.rows[0].data, vec![1; 4]);
        assert_eq!(dup.rows[1].data, vec![9; 4]);
    }

    /// A table no mod touched must reach the game as the game's own bytes.
    #[test]
    fn an_untouched_table_is_the_games_own_bytes() {
        let a = edits("11711000", &[(1, [9; 8]), (2, [2; 8]), (3, [3; 8])]);
        let out = run(vec![contribution("A", a)]);
        let merged = Regulation::read(&KEY, out.bytes.as_ref().unwrap()).unwrap();
        let original = Regulation::read(&KEY, &base()).unwrap();
        assert_eq!(merged.bnd.files[1].data, original.bnd.files[1].data);
    }

    #[test]
    fn a_mod_from_another_version_with_a_different_shape_is_refused_by_name() {
        let old = regulation(
            "11001000",
            vec![
                ("AtkParam", table("ATK_ST", 4, &[(1, [1; 4])])),
                (
                    "ShopParam",
                    table("SHOP_ST", 8, &[(10, [0; 8]), (20, [0; 8])]),
                ),
            ],
        );
        let good = edits("11711000", &[(1, [9; 8]), (2, [2; 8]), (3, [3; 8])]);
        let out = run(vec![contribution("Old", old), contribution("Good", good)]);

        assert_eq!(out.report.refused.len(), 1);
        let why = &out.report.refused[0];
        assert_eq!(why.label, "Old");
        assert!(why.reason.contains("1.10.0"), "{}", why.reason);
        assert!(why.reason.contains("1.17.1"), "{}", why.reason);
        assert_eq!(out.report.merged.len(), 1, "the other mod still merges");
    }

    #[test]
    fn a_table_missing_from_a_mod_refuses_it() {
        let partial = regulation(
            "11711000",
            vec![(
                "AtkParam",
                table("ATK_ST", 8, &[(1, [1; 8]), (2, [2; 8]), (3, [3; 8])]),
            )],
        );
        let out = run(vec![contribution("Partial", partial)]);
        assert!(out.bytes.is_none(), "nothing merged, so nothing to deploy");
        assert!(out.report.refused[0].reason.contains("ShopParam"));
    }

    /// Rows missing from an older file could be the update's doing or the
    /// mod's; nothing can tell which. Merging would delete them from the
    /// current game either way, so the mod is refused and the reason says the
    /// cause is ambiguous rather than claiming to know it.
    #[test]
    fn an_older_file_missing_rows_is_refused_rather_than_deleting_them() {
        let old = edits("11701000", &[(1, [9; 8]), (2, [2; 8])]);
        let out = run(vec![contribution("Old", old)]);
        assert!(out.bytes.is_none());
        let reason = &out.report.refused[0].reason;
        assert!(reason.contains("cannot be told apart"), "{reason}");
        assert!(reason.contains("1.17.0"), "{reason}");
    }

    #[test]
    fn an_older_file_of_the_same_shape_merges_with_a_warning() {
        let old = edits("11701000", &[(1, [9; 8]), (2, [2; 8]), (3, [3; 8])]);
        let out = run(vec![contribution("Old", old)]);
        assert!(out.bytes.is_some());
        assert_eq!(out.report.warnings.len(), 1);
        assert!(out.report.warnings[0].contains("1.17.0"));
    }

    #[test]
    fn an_unreadable_mod_is_refused_and_the_rest_merge() {
        let good = edits("11711000", &[(1, [9; 8]), (2, [2; 8]), (3, [3; 8])]);
        let out = run(vec![
            contribution("Garbage", vec![0; 100]),
            contribution("Good", good),
        ]);
        assert_eq!(out.report.refused[0].label, "Garbage");
        assert!(out.bytes.is_some());
    }
}

//! `apoc-fs names`: our own list of archive paths, built from the player's
//! game and nothing else.
//!
//! The archives file entries under a hash of the path, so a guess at a path is
//! either confirmed by a matching entry or thrown away. Nothing here is a list
//! of names, and the list made is game data, so it is never committed.
//! Candidates come from what the game's files say about themselves:
//!
//! 1. Text in `eldenring.exe`: literal paths and the folder aliases they use.
//! 2. What binders hold: a binder's members give its stem and folder.
//! 3. Text in every other file, as paths.
//! 4. Map ids, found by sweeping every id and in text, with the files the
//!    exe's path formats say a map has.
//! 5. Other ids swept over their whole range, in the same formats.
//! 6. Every word of text tried in every folder and extension named so far.
//!
//! With 64-bit hashes against 120,000 entries, a wrong path matching by chance
//! is about one in 10^14 a try, so even the trillions tried here are unlikely
//! to hold a false name.

use std::collections::{BTreeMap, HashSet};
use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fs_bytes::Result;
use fs_dvdbnd::{Dvd, hash_path};
use rayon::prelude::*;

/// The multiplier of the archive's path hash (`fs_dvdbnd::name`).
const HASH_FACTOR: u64 = 0x85;

/// Paths tried against the hashes wanted, keeping the ones that hit.
pub struct Namer {
    wanted: HashSet<u64>,
    named: HashMap<u64, String>,
    source: &'static str,
    by_source: BTreeMap<&'static str, usize>,
}

impl Namer {
    pub fn new(wanted: impl IntoIterator<Item = u64>) -> Self {
        Namer {
            wanted: wanted.into_iter().collect(),
            named: HashMap::new(),
            source: "",
            by_source: BTreeMap::new(),
        }
    }

    /// Credit what is named from here on to this source.
    fn source(&mut self, source: &'static str) {
        self.source = source;
    }

    /// Try one path; true when it names an entry not named before. Paths are
    /// kept in lower case, since the hash cannot tell case apart.
    pub fn attempt(&mut self, path: &str) -> bool {
        let h = hash_path(path);
        if !self.wanted.contains(&h) || self.named.contains_key(&h) {
            return false;
        }
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let path = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        self.named.insert(h, path);
        *self.by_source.entry(self.source).or_default() += 1;
        true
    }

    /// Try a path as stored and compressed.
    fn attempt_both(&mut self, path: &str) {
        self.attempt(path);
        self.attempt(&format!("{path}.dcx"));
    }
}

fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'/' | b'\\' | b':' | b'-' | b'%')
}

/// Runs of name characters in `b`, one byte per character or UTF-16.
pub fn text_runs(b: &[u8], mut found: impl FnMut(String)) {
    let mut i = 0;
    while i < b.len() {
        if !is_name_byte(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && is_name_byte(b[i]) {
            i += 1;
        }
        if (5..200).contains(&(i - start)) {
            found(String::from_utf8_lossy(&b[start..i]).into_owned());
        }
    }
    for odd in 0..2 {
        let mut i = odd;
        while i + 1 < b.len() {
            if !(is_name_byte(b[i]) && b[i + 1] == 0) {
                i += 2;
                continue;
            }
            let start = i;
            while i + 1 < b.len() && is_name_byte(b[i]) && b[i + 1] == 0 {
                i += 2;
            }
            if (3..200).contains(&((i - start) / 2)) {
                found(b[start..i].iter().step_by(2).map(|&c| c as char).collect());
            }
        }
    }
}

fn magic(b: &[u8]) -> String {
    b.iter()
        .take(4)
        .map(|&c| if c.is_ascii_graphic() { c as char } else { '?' })
        .collect()
}

#[derive(Default)]
struct Facts {
    members: Vec<String>,
    strings: HashSet<String>,
    inner: String,
}

/// What one stored file says about itself, through compression and binders.
fn walk(b: &[u8], f: &mut Facts, depth: usize) {
    let plain;
    let b = if fs_dcx::is_dcx(b) {
        match fs_dcx::decompress(b) {
            Ok((_, p)) => {
                plain = p;
                &plain[..]
            }
            Err(_) => return,
        }
    } else {
        b
    };
    if depth == 0 {
        f.inner = magic(b);
    }
    if fs_bnd4::is_bnd4(b)
        && depth < 4
        && let Ok(bnd) = fs_bnd4::Bnd4::read(b)
    {
        for file in &bnd.files {
            if let Some(n) = &file.name {
                f.members.push(n.clone());
            }
            walk(&file.data, f, depth + 1);
        }
        return;
    }
    if b.starts_with(b"BDF4") || b.starts_with(b"AKPK") || b.starts_with(b"DDS ") {
        return;
    }
    if b.starts_with(b"TPF\0") {
        // The names sit before the first texture; the textures are noise.
        if b.len() >= 0x14 {
            let first = u32::from_le_bytes(b[0x10..0x14].try_into().unwrap()) as usize;
            text_runs(&b[0x10..first.clamp(0x10, b.len())], |s| {
                f.strings.insert(s);
            });
        }
        return;
    }
    text_runs(b, |s| {
        f.strings.insert(s);
    });
}

/// What one pass over every entry learned. It takes minutes, so it is kept
/// between runs in a cache folder; delete that after a game patch.
#[derive(Default)]
pub struct Corpus {
    /// Per entry: hash, archive, outer kind, inner kind.
    pub entries: Vec<(u64, String, String, String)>,
    /// Binder members, by the hash of the entry holding them.
    pub members: Vec<(u64, String)>,
    /// Text that might be a name.
    pub strings: Vec<String>,
}

fn gather(game: &Path, cache: &Path, jobs: usize) -> Result<()> {
    std::fs::create_dir_all(cache)?;
    let dvd = Dvd::open_elden_ring(game)?;
    let refs: Vec<_> = dvd.entries().map(|(r, _)| r).collect();
    let writer = |n: &str| -> Result<_> {
        Ok(Mutex::new(std::io::BufWriter::new(std::fs::File::create(
            cache.join(format!("{n}.partial")),
        )?)))
    };
    let table = writer("entries.tsv")?;
    let members = writer("members.tsv")?;
    let strings = writer("strings.tsv")?;
    let seen = Mutex::new(HashSet::<String>::new());
    let large = Mutex::new(());
    pool(jobs).install(|| {
        refs.par_iter().try_for_each(|&r| -> Result<()> {
            let entry = dvd.entry(r);
            // The largest files (1.4 GB) are read one at a time.
            let _guard =
                (entry.padded_size as usize > 64 * 1024 * 1024).then(|| large.lock().unwrap());
            let stored = dvd.read(r)?;
            let mut f = Facts::default();
            walk(&stored, &mut f, 0);
            writeln!(
                table.lock().unwrap(),
                "{:016x}\t{}\t{}\t{}",
                entry.hash,
                dvd.archives[r.archive].name,
                magic(&stored),
                f.inner
            )?;
            {
                let mut m = members.lock().unwrap();
                for n in &f.members {
                    writeln!(m, "{:016x}\t{n}", entry.hash)?;
                }
            }
            let mut seen = seen.lock().unwrap();
            let mut out = strings.lock().unwrap();
            for s in f.strings.into_iter().filter(|s| plausible(s)) {
                if !seen.contains(&s) {
                    writeln!(out, "{s}")?;
                    seen.insert(s);
                }
            }
            Ok(())
        })
    })?;
    for w in [table, members, strings] {
        w.into_inner().unwrap().flush()?;
    }
    // Only a finished pass counts as a cache.
    for n in ["entries.tsv", "members.tsv", "strings.tsv"] {
        std::fs::rename(cache.join(format!("{n}.partial")), cache.join(n))?;
    }
    Ok(())
}

/// Whether text could be a path or a name in one. Weeds out most of what
/// random bytes spell.
pub fn plausible(s: &str) -> bool {
    let b = s.as_bytes();
    let digit = b.iter().any(u8::is_ascii_digit);
    let lower = b.iter().any(u8::is_ascii_lowercase);
    let upper = b.iter().filter(|c| c.is_ascii_uppercase()).count();
    let sep = b.iter().any(|c| matches!(c, b'_' | b'.' | b'/' | b'\\'));
    (b.len() < 80) && (sep || (digit && upper + 2 >= b.len() / 2) || (lower && upper <= 2))
}

fn load(cache: &Path) -> Result<Corpus> {
    let mut c = Corpus::default();
    let lines = |n: &str| -> Result<_> {
        Ok(std::io::BufReader::new(std::fs::File::open(cache.join(n))?).split(b'\n'))
    };
    for l in lines("entries.tsv")? {
        let l = l?;
        let l = String::from_utf8_lossy(&l);
        let mut p = l.split('\t');
        if let (Some(h), Some(a), Some(o), Some(i)) = (p.next(), p.next(), p.next(), p.next())
            && let Ok(h) = u64::from_str_radix(h, 16)
        {
            c.entries.push((h, a.into(), o.into(), i.into()));
        }
    }
    for l in lines("members.tsv")? {
        let l = l?;
        let l = String::from_utf8_lossy(&l);
        if let Some((h, n)) = l.split_once('\t')
            && let Ok(h) = u64::from_str_radix(h, 16)
        {
            c.members.push((h, n.to_string()));
        }
    }
    for l in lines("strings.tsv")? {
        c.strings.push(String::from_utf8_lossy(&l?).into_owned());
    }
    Ok(c)
}

/// Text in the exe that could be a path.
fn exe_strings(game: &Path) -> Result<Vec<String>> {
    let b = std::fs::read(game.join("eldenring.exe"))?;
    let mut set = BTreeSet::new();
    text_runs(&b, |s| {
        set.insert(s);
    });
    Ok(set.into_iter().collect())
}

/// Binder extensions, from the exe's path formats and resource names.
const BINDER_EXTENSIONS: &[&str] = &[
    "geombnd",
    "geomhkxbnd",
    "mapbnd",
    "hkxbnd",
    "nvmhktbnd",
    "tpfbnd",
    "ivinfobnd",
    "partsbnd",
    "chrbnd",
    "anibnd",
    "behbnd",
    "texbnd",
    "luabnd",
    "talkesdbnd",
    "ffxbnd",
    "cutscenebnd",
    "relationinfobnd",
    "mapinfotexbnd",
    "msgbnd",
    "fgbnd",
    "tpf",
    "matbinbnd",
    "shaderbnd",
    "shaderbdlebnd",
    "parambnd",
    "paramdefbnd",
    "aipbnd",
    "rumblebnd",
    "movtae",
    "entryfilelistbnd",
    "moaeibnd",
    "sblytbnd",
    "mtmskbnd",
];

/// Which folder an alias in the exe's paths stands for, where it is not the
/// alias's own name. The exe lists its aliases' roots (`data2:/map/mapstudio`),
/// and each pairing here was confirmed by the hashes it hits.
const ALIAS_FOLDERS: &[(&str, &str)] = &[
    ("chranibnd", "chr"),
    ("chrbehbnd", "chr"),
    ("chrbnd", "chr"),
    ("chrtexbnd", "chr"),
    ("mapstudio", "map/mapstudio"),
    ("entryfilelist", "map/entryfilelist"),
    ("sfxbnd", "sfx"),
    ("patch_sfxbnd", "sfx"),
    ("wwise_moaeibnd", "sound"),
    ("wwise_mobnkinfo", "sound"),
    ("talkscript", "script/talk"),
    ("aiscript", "script"),
    ("luascriptpatch", "script"),
    ("actscript", "action/script"),
    ("gparam", "param/drawparam"),
    ("regulation", ""),
    ("mapinfotex", "other/mapinfotex"),
    ("menutpfbnd", "menu"),
];

/// Text as a path, under the folders its alias could stand for.
fn literal_paths(n: &mut Namer, strings: &[String]) {
    for s in strings {
        let s = s.replace('\\', "/");
        if let Some((_, r)) = s.split_once("INTERROOT_win64/") {
            n.attempt_both(&format!("/{r}"));
        }
        let (alias, rest) = s.split_once(":/").unwrap_or(("", s.as_str()));
        if rest.contains('%') || rest.is_empty() {
            continue;
        }
        let rest = rest.trim_start_matches('/');
        let mapped = ALIAS_FOLDERS
            .iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(alias))
            .map_or("", |(_, f)| f);
        for root in ["", alias, mapped, "map", "other", "sfx", "msg", "menu"] {
            if root.is_empty() {
                n.attempt_both(&format!("/{rest}"));
            } else {
                n.attempt_both(&format!("/{root}/{rest}"));
            }
        }
    }
}

/// `_h` or `_l`, as the high and low versions of a stem have.
fn without_level(s: &str) -> &str {
    ["_h", "_l", "_H", "_L"]
        .iter()
        .find_map(|x| s.strip_suffix(x))
        .unwrap_or(s)
}

/// Every folder and stem a binder's member suggests, tried with each binder
/// extension.
fn from_member(n: &mut Namer, member: &str) {
    let path = member.replace('\\', "/");
    let Some((_, rel)) = path.split_once("INTERROOT_win64/") else {
        return;
    };
    let parts: Vec<&str> = rel.split('/').collect();
    let mut stems: HashSet<&str> = parts
        .iter()
        .map(|p| p.split('.').next().unwrap_or(p))
        .collect();
    stems.insert(without_level(
        parts[parts.len() - 1].split('.').next().unwrap_or(""),
    ));
    let mut dirs: HashSet<String> = (0..parts.len()).map(|i| parts[..i].join("/")).collect();
    // Maps sit in a folder for their area; a member may name another map's.
    for m in map_ids(std::iter::once(rel)) {
        dirs.insert(format!("map/m{}/{m}", &m[1..3]));
    }
    dirs.extend(["parts", "chr", "script", "map"].map(String::from));
    for d in &dirs {
        for s in &stems {
            for suffix in ["", "_h", "_l"] {
                for e in BINDER_EXTENSIONS {
                    n.attempt_both(&format!("/{d}/{s}{suffix}.{e}"));
                }
            }
        }
    }
    match parts[0] {
        // `cutscene:/%s_%02d.tpfbnd`
        "cutscene" if parts.len() > 2 => {
            for i in 0..100 {
                n.attempt_both(&format!("/cutscene/{}_{i:02}.tpfbnd", parts[1]));
            }
        }
        // `msg:/%s/item_dlc02.msgbnd` and its like.
        "msg" if parts.len() > 2 => {
            for base in ["item", "menu", "ngword", "sellregion"] {
                for dlc in ["", "_dlc01", "_dlc02"] {
                    n.attempt_both(&format!("/msg/{}/{base}{dlc}.msgbnd", parts[1]));
                }
            }
        }
        _ => {}
    }
}

/// The map ids (`mAA_BB_CC_DD`) in some text, in lower case.
pub fn map_ids<'a>(texts: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for t in texts {
        for w in t.as_bytes().windows(12) {
            let shape = w[0].eq_ignore_ascii_case(&b'm')
                && [1, 2, 4, 5, 7, 8, 10, 11]
                    .iter()
                    .all(|&k| w[k].is_ascii_digit())
                && [3, 6, 9].iter().all(|&k| w[k] == b'_');
            if shape {
                found.insert(String::from_utf8_lossy(w).to_lowercase());
            }
        }
    }
    found
}

/// The files the exe's path formats say a map has.
fn from_map_id(n: &mut Namer, m: &str) {
    let area = &m[1..3];
    let id = &m[1..];
    let root = format!("/map/m{area}/{m}");
    for f in [
        format!("/map/mapstudio/{m}.msb"),
        format!("/event/{m}.emevd"),
        format!("/map/onav/{m}.onav"),
        format!("/script/talk/{m}.talkesdbnd"),
        format!("/other/mapinfotex/{m}.mapinfotexbnd"),
        format!("/sfx/sfxbnd_{m}.ffxbnd"),
        format!("{root}/{m}.nva"),
        format!("{root}/{m}.mfr"),
        format!("{root}/{m}.mpw"),
        format!("{root}/{m}.pmdcl"),
        format!("{root}/{m}.nvmhktbnd"),
        format!("{root}/{m}.relationinfobnd"),
    ] {
        n.attempt_both(&f);
    }
    for kind in ["h", "f", "l", "n"] {
        n.attempt(&format!("{root}/{kind}{id}.hkxbhd"));
        n.attempt(&format!("{root}/{kind}{id}.hkxbdt"));
    }
    for q in ["high", "middle", "low"] {
        n.attempt_both(&format!("{root}/{m}_{q}.ivinfobnd"));
        for i in 0..10 {
            for j in 0..100 {
                n.attempt_both(&format!("{root}/{m}_Envmap_{i:02}_{q}_{j:02}.tpfbnd"));
            }
        }
    }
    for i in 0..10_000 {
        n.attempt_both(&format!("{root}/{m}_{i:04}.btl"));
        n.attempt_both(&format!("{root}/{m}_{i:04}.fvb"));
    }
    for lod in 0..6 {
        n.attempt_both(&format!("/map/breakgeom/lod{lod}/{m}_lod{lod}.breakgeom"));
    }
    let common = format!("/map/m{area}/common");
    n.attempt_both(&format!("{common}/m{area}_CGrading.tpf"));
    n.attempt_both(&format!("{common}/m{area}_9999.tpf"));
    n.attempt(&format!("{common}/GI_Env_m{area}.tpfbhd"));
    n.attempt(&format!("{common}/GI_Env_m{area}.tpfbdt"));
    n.attempt_both(&format!("/sfx/sfxbnd_m{area}.ffxbnd"));
    n.attempt_both(&format!("/sfx/sfxbnd_m{area}.ffxbnd.devpatch"));
}

/// One byte of a template: a fixed byte, or a digit of the id being swept.
#[derive(Clone, Copy)]
enum Slot {
    Fixed(u8),
    Digit(usize),
}

/// A path with an id to sweep: `#` is a new digit, `@N` repeats digit `N`.
fn parse_template(t: &str) -> (Vec<Slot>, usize) {
    let mut slots = Vec::new();
    let mut digits = 0;
    let mut it = t.bytes();
    while let Some(c) = it.next() {
        match c {
            b'#' => {
                slots.push(Slot::Digit(digits));
                digits += 1;
            }
            b'@' => {
                let d = it.next().expect("a digit's number follows @");
                slots.push(Slot::Digit((d - b'0') as usize));
            }
            c => slots.push(Slot::Fixed(c.to_ascii_lowercase())),
        }
    }
    (slots, digits)
}

/// A bit per hash prefix, so that nearly every wrong guess costs one load.
struct Filter {
    bits: Vec<u64>,
}

impl Filter {
    const BITS: u32 = 30;

    fn new(hashes: impl Iterator<Item = u64>) -> Self {
        let mut bits = vec![0u64; 1 << (Self::BITS - 6)];
        for h in hashes {
            let k = h >> (64 - Self::BITS);
            bits[(k >> 6) as usize] |= 1 << (k & 63);
        }
        Filter { bits }
    }

    #[inline]
    fn may_hold(&self, h: u64) -> bool {
        let k = h >> (64 - Self::BITS);
        self.bits[(k >> 6) as usize] >> (k & 63) & 1 == 1
    }
}

#[inline]
fn hash_more(h: u64, c: u8) -> u64 {
    h.wrapping_mul(HASH_FACTOR).wrapping_add(u64::from(c))
}

fn pool(jobs: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .expect("a thread pool")
}

/// Every value of a template whose hash is wanted. The digits are split so
/// that only the tail of the path is rehashed for most values.
fn sweep(
    wanted: &HashSet<u64>,
    filter: &Filter,
    template: &str,
    threads: &rayon::ThreadPool,
) -> Vec<String> {
    let (slots, digits) = parse_template(template);
    assert!(digits > 0 && digits <= 18, "too many digits: {template}");
    let byte = |s: &Slot, ds: &[u8]| match *s {
        Slot::Fixed(c) => c,
        Slot::Digit(i) => ds[i],
    };
    let fill = |ds: &mut [u8], mut v: u64| {
        for d in ds.iter_mut().rev() {
            *d = (v % 10) as u8 + b'0';
            v /= 10;
        }
    };
    // Digits past `outer` change for every value; the path up to them is
    // hashed once per `outer` value.
    let outer = digits.saturating_sub(4);
    let first_inner = slots
        .iter()
        .position(|s| matches!(s, Slot::Digit(i) if *i >= outer))
        .unwrap_or(slots.len());
    let inner_count = 10u64.pow((digits - outer) as u32);
    let chunks = 10u64.pow(outer.min(4) as u32);
    let per_chunk = 10u64.pow((outer - outer.min(4)) as u32);
    let hits = Mutex::new(Vec::new());
    threads.install(|| {
        (0..chunks).into_par_iter().for_each(|chunk| {
            let mut ds = vec![b'0'; digits];
            for o in 0..per_chunk {
                let outer_value = chunk * per_chunk + o;
                fill(&mut ds[..outer], outer_value);
                let start = slots[..first_inner]
                    .iter()
                    .fold(0, |h, s| hash_more(h, byte(s, &ds)));
                for i in 0..inner_count {
                    fill(&mut ds[outer..], i);
                    let h = slots[first_inner..]
                        .iter()
                        .fold(start, |h, s| hash_more(h, byte(s, &ds)));
                    if filter.may_hold(h) && wanted.contains(&h) {
                        let path = slots.iter().map(|s| byte(s, &ds) as char).collect();
                        hits.lock().unwrap().push(path);
                    }
                }
            }
        })
    });
    hits.into_inner().unwrap()
}

/// Sweeps take minutes at their largest, so their hits are kept in the cache.
fn sweep_all(n: &mut Namer, templates: &[String], cache: &Path, jobs: usize) -> Result<()> {
    let file = cache.join("sweeps.tsv");
    let mut done: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(&file) {
        for l in text.lines() {
            if let Some((t, p)) = l.split_once('\t') {
                let hits = done.entry(t.to_string()).or_default();
                if !p.is_empty() {
                    hits.push(p.to_string());
                }
            }
        }
    }
    let filter = Filter::new(n.wanted.iter().copied());
    let threads = pool(jobs);
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?;
    for t in templates {
        let hits = match done.remove(t) {
            Some(hits) => hits,
            None => {
                let hits = sweep(&n.wanted, &filter, t, &threads);
                for p in &hits {
                    writeln!(log, "{t}\t{p}")?;
                }
                writeln!(log, "{t}\t")?;
                hits
            }
        };
        for p in &hits {
            n.attempt(p);
        }
    }
    Ok(())
}

/// The id sweeps. Each format is the exe's (`%s:/aet/aet%s/%s.%s`, `e%06d/i%010d`).
fn id_templates() -> Vec<String> {
    let mut t: Vec<String> = [
        "/asset/aet/aet###/aet@0@1@2_###.tpf.dcx",
        "/asset/aet/aet###/aet@0@1@2_###_l.tpf.dcx",
        "/asset/aet/aet###/aet@0@1@2_###_billboards.tpf.dcx",
        "/asset/aet/aet###/aet@0@1@2_###_billboards_l.tpf.dcx",
        "/asset/aeg/aeg###/aeg@0@1@2_###.geombnd.dcx",
        "/asset/aeg/aeg###/aeg@0@1@2_###_l.geomhkxbnd.dcx",
        "/asset/aeg/aeg###/aeg@0@1@2_###_h.geomhkxbnd.dcx",
        "/chr/c####.chrbnd.dcx",
        "/chr/c####.anibnd.dcx",
        "/chr/c####.behbnd.dcx",
        "/chr/c####_h.texbnd.dcx",
        "/chr/c####_l.texbnd.dcx",
        "/chr/c0000_c####.anibnd.dcx",
        "/action/script/c####.hks",
        "/sfx/sfxbnd_c####.ffxbnd.dcx",
        "/sfx/sfxbnd_c####.ffxbnd.devpatch.dcx",
        "/sfx/sfxbnd_m##.ffxbnd.dcx",
        "/sfx/sfxbnd_mimic#########.ffxbnd.dcx",
        "/map/entryfilelist/e######/e@0@1@2@3@4@5####.entryfilelist",
        "/map/entryfilelist/e######/i@0@1@2@3@4@5####.entryfilelist",
        "/param/drawparam/s##_##_####.gparam.dcx",
        "/param/drawparam/m##_##_####.gparam.dcx",
        "/param/drawparam/s##_##_####_weatheroverride_##.gparam.dcx",
        "/param/drawparam/m##_##_####_weatheroverride_##.gparam.dcx",
        "/movie/########.bk2",
    ]
    .map(String::from)
    .to_vec();
    t.push("/map/mapstudio/m##_##_##_##.msb.dcx".into());
    t.push("/event/m##_##_##_##.emevd.dcx".into());
    t.push("/map/onav/m##_##_##_##.onav.dcx".into());
    t
}

/// Each word of text with a path's folder and extension: `/dir/` + word + `.ext`.
/// The hash of a joined path comes from the parts' hashes, so this needs no
/// joining until a hit.
fn words_in_folders(n: &mut Namer, words: &[String], places: &[(String, String)], jobs: usize) {
    let filter = Filter::new(n.wanted.iter().copied());
    let wanted = &n.wanted;
    let parts: Vec<(u64, u64)> = places
        .iter()
        .map(|(d, e)| (hash_path(&format!("{d}/")), pow_factor(e.len() as u32 + 1)))
        .collect();
    let tails: Vec<u64> = places
        .iter()
        .map(|(_, e)| {
            format!(".{e}")
                .bytes()
                .fold(0, |h, c| hash_more(h, c.to_ascii_lowercase()))
        })
        .collect();
    let hits = Mutex::new(Vec::new());
    pool(jobs).install(|| {
        words.par_iter().for_each(|w| {
            let scale = pow_factor(w.len() as u32);
            let body = w
                .bytes()
                .fold(0, |h, c| hash_more(h, c.to_ascii_lowercase()));
            for (i, &(head, tail_scale)) in parts.iter().enumerate() {
                let h = head
                    .wrapping_mul(scale)
                    .wrapping_add(body)
                    .wrapping_mul(tail_scale)
                    .wrapping_add(tails[i]);
                if filter.may_hold(h) && wanted.contains(&h) {
                    hits.lock()
                        .unwrap()
                        .push(format!("{}/{w}.{}", places[i].0, places[i].1));
                }
            }
        })
    });
    for p in hits.into_inner().unwrap() {
        n.attempt(&p);
    }
}

fn pow_factor(e: u32) -> u64 {
    HASH_FACTOR.wrapping_pow(e)
}

/// The folders and extensions of paths named so far, without those tied to
/// one map or one asset, along with the places the exe names but no file
/// named so far has shown.
fn places_known(n: &Namer) -> Vec<(String, String)> {
    let mut places = BTreeSet::new();
    for p in n.named.values() {
        if let Some((dir, file)) = p.rsplit_once('/')
            && let Some((_, ext)) = file.split_once('.')
            && !dir.contains("/m6")
            && map_ids(std::iter::once(dir)).is_empty()
            && !dir.contains("/aeg/")
            && !dir.contains("/aet/")
            && !dir.contains("/entryfilelist/")
        {
            places.insert((dir.to_string(), ext.to_string()));
        }
    }
    for (d, e) in [
        ("/sound", "pck"),
        ("/sound", "bnk"),
        ("/sd", "pck"),
        ("/sd", "bnk"),
        ("/menu/win", "gfx"),
        ("/menu", "gfx"),
        ("/menu/hi", "tpf.dcx"),
        ("/menu/low", "tpf.dcx"),
        ("/menu/hi", "sblytbnd.dcx"),
        ("/menu/low", "sblytbnd.dcx"),
        ("/menu/hi", "tpfbhd"),
        ("/menu/hi", "tpfbdt"),
        ("/menu/low", "tpfbhd"),
        ("/menu/low", "tpfbdt"),
        ("/menu", "tpfbhd"),
        ("/menu", "tpfbdt"),
        ("/param/drawparam", "gparam.dcx"),
        ("/shader", "shaderbnd.dcx"),
        ("/shader", "shaderbdlebnd.dcx"),
        ("/shader", "shaderbdlebnd.devpatch.dcx"),
        ("/font", "gfx"),
        ("/movie", "bk2"),
        ("/other", "tpf.dcx"),
        ("/other", "tpf"),
        ("/other", "dds"),
        ("/expression", "dcx"),
        ("/material", "matbinbnd.dcx"),
        ("/material", "matbinbnd.devpatch.dcx"),
        ("/sfx", "ffxbnd.dcx"),
        ("/sfx", "ffxbnd.devpatch.dcx"),
        ("/action", "hks"),
        ("/action/script", "hks"),
        ("/action", "txt"),
        ("/action/script", "txt"),
        ("/action", "dcx"),
        ("/map/nvc", "nvc.dcx"),
        ("/map/breakobj", "breakobj.dcx"),
    ] {
        places.insert((d.to_string(), e.to_string()));
    }
    places.into_iter().collect()
}

/// The words in a text: whole, its last path part, and that without what
/// follows the first dot.
fn words_from(strings: &[&str]) -> Vec<String> {
    let mut words = HashSet::new();
    for s in strings {
        let last = s.rsplit(['/', '\\']).next().unwrap_or(s);
        let stem = last.split('.').next().unwrap_or(last);
        for w in [stem, last] {
            let ok = (2..=60).contains(&w.len())
                && w.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'));
            if ok {
                words.insert(w.to_ascii_lowercase());
            }
        }
        // `_02_044_PC_TextSelect_fla.MainTimeline` is the movie `02_044_PC_TextSelect`.
        if let Some((movie, _)) = stem.split_once("_fla")
            && let Some(movie) = movie.rsplit('%').next()
        {
            words.insert(movie.trim_start_matches('_').to_ascii_lowercase());
        }
    }
    let mut words: Vec<String> = words.into_iter().collect();
    words.sort();
    words
}

/// Everything the passes above name, from what was read of the game.
pub fn name_all(c: &Corpus, exe: &[String], cache: &Path, jobs: usize) -> Result<Namer> {
    let mut n = Namer::new(c.entries.iter().map(|e| e.0));

    n.source("exe");
    literal_paths(&mut n, exe);

    n.source("binder members");
    let mut seen = HashSet::new();
    for (_, m) in &c.members {
        if seen.insert(m.as_str()) {
            from_member(&mut n, m);
        }
    }

    n.source("text in files");
    literal_paths(&mut n, &c.strings);

    n.source("id sweeps");
    sweep_all(&mut n, &id_templates(), cache, jobs)?;

    n.source("map ids");
    let mut maps = map_ids(c.strings.iter().map(String::as_str));
    maps.extend(map_ids(c.members.iter().map(|m| m.1.as_str())));
    maps.extend(map_ids(n.named.values().map(String::as_str)));
    for m in &maps {
        from_map_id(&mut n, m);
    }

    n.source("words in known places");
    let mut texts: Vec<&str> = exe.iter().map(String::as_str).collect();
    texts.extend(c.strings.iter().map(String::as_str));
    texts.extend(c.members.iter().map(|m| m.1.as_str()));
    let words = words_from(&texts);
    // Twice, since the first round shows new folders.
    for _ in 0..2 {
        let places = places_known(&n);
        words_in_folders(&mut n, &words, &places, jobs);
    }
    Ok(n)
}

pub fn run(game: PathBuf, out: PathBuf, cache: PathBuf, jobs: usize) -> Result<()> {
    if !cache.join("strings.tsv").exists() {
        eprintln!("reading every entry once, which takes minutes");
        gather(&game, &cache, jobs)?;
    }
    let corpus = load(&cache)?;
    let n = name_all(&corpus, &exe_strings(&game)?, &cache, jobs)?;

    let mut names: Vec<&String> = n.named.values().collect();
    names.sort();
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut w = std::io::BufWriter::new(std::fs::File::create(&out)?);
    for p in names {
        writeln!(w, "{p}")?;
    }
    w.flush()?;
    let unnamed = out.with_extension("unnamed.tsv");
    report(&n, &corpus, &unnamed)?;
    println!(
        "list: {}\nstill unnamed: {}",
        out.display(),
        unnamed.display()
    );
    Ok(())
}

/// The counts, and the entries still without a name.
fn report(n: &Namer, c: &Corpus, unnamed: &Path) -> Result<()> {
    let total = c.entries.len();
    let named = c
        .entries
        .iter()
        .filter(|e| n.named.contains_key(&e.0))
        .count();
    println!(
        "named {named} of {total} entries ({} distinct paths), {} left",
        n.named.len(),
        total - named
    );
    println!("by source: {:?}", n.by_source);

    let first_member: HashMap<u64, &str> = c
        .members
        .iter()
        .rev()
        .map(|(h, m)| (*h, m.as_str()))
        .collect();
    let mut left: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
    let mut w = std::io::BufWriter::new(std::fs::File::create(unnamed)?);
    for (h, a, o, i) in &c.entries {
        if !n.named.contains_key(h) {
            *left.entry((a, o, i)).or_default() += 1;
            let member = first_member.get(h).copied().unwrap_or("");
            writeln!(w, "{h:016x}\t{a}\t{o}\t{i}\t{member}")?;
        }
    }
    w.flush()?;
    println!("unnamed by archive and kind (outer, then inside its compression):");
    for (k, v) in &left {
        println!("  {v:6} {k:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanting(paths: &[&str]) -> Namer {
        Namer::new(paths.iter().map(|p| hash_path(p)))
    }

    #[test]
    fn only_a_matching_hash_names_an_entry() {
        let mut n = wanting(&["/event/m10_00_00_00.emevd.dcx"]);
        assert!(!n.attempt("/event/m11_00_00_00.emevd.dcx"));
        assert!(n.attempt(r"EVENT\M10_00_00_00.emevd.dcx"));
        assert!(!n.attempt("/event/m10_00_00_00.emevd.dcx"), "named once");
        let names: Vec<_> = n.named.values().collect();
        assert_eq!(names, ["/event/m10_00_00_00.emevd.dcx"]);
    }

    #[test]
    fn text_is_found_in_both_encodings() {
        let mut bytes = b"\xff\xfe/map/thing.bin\x00\x01ab\xff\xff".to_vec();
        for c in "c1234_x".bytes() {
            bytes.extend([c, 0]);
        }
        let mut found = Vec::new();
        text_runs(&bytes, |s| found.push(s));
        assert!(found.contains(&"/map/thing.bin".to_string()));
        assert!(found.contains(&"c1234_x".to_string()));
        assert!(!found.contains(&"ab".to_string()), "too short to be a name");
    }

    #[test]
    fn map_ids_are_found_inside_longer_text() {
        let ids = map_ids(["x\\M60_43_36_00\\n60_43_36_00_1", "m6_1_2_3 m1"].into_iter());
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), ["m60_43_36_00"]);
    }

    #[test]
    fn a_sweep_finds_ids_and_ties_repeated_digits() {
        let wanted: HashSet<u64> = ["/a/e01/e01234.x", "/a/e99/e99000.x", "/a/e01/e02000.x"]
            .iter()
            .map(|p| hash_path(p))
            .collect();
        let filter = Filter::new(wanted.iter().copied());
        let t = "/a/e##/e@0@1###.x";
        let mut hits = sweep(&wanted, &filter, t, &pool(2));
        hits.sort();
        // The third does not repeat its first two digits, so it is not this
        // template's.
        assert_eq!(hits, ["/a/e01/e01234.x", "/a/e99/e99000.x"]);
    }

    #[test]
    fn a_sweep_of_few_digits_has_no_outer_part() {
        let wanted: HashSet<u64> = [hash_path("/chr/c0042.chrbnd.dcx")].into();
        let filter = Filter::new(wanted.iter().copied());
        let hits = sweep(&wanted, &filter, "/chr/c####.chrbnd.dcx", &pool(1));
        assert_eq!(hits, ["/chr/c0042.chrbnd.dcx"]);
    }

    #[test]
    fn a_word_joined_to_a_place_hashes_as_the_whole_path_does() {
        let mut n = wanting(&["/menu/win/02_044_pc_textselect.gfx", "/other/a.tpf.dcx"]);
        let words = ["02_044_PC_TextSelect".to_string(), "a".to_string()];
        let places = [
            ("/menu/win".to_string(), "gfx".to_string()),
            ("/other".to_string(), "tpf.dcx".to_string()),
        ];
        words_in_folders(&mut n, &words, &places, 1);
        assert_eq!(n.named.len(), 2);
    }

    #[test]
    fn words_come_from_paths_and_movie_classes() {
        let w = words_from(&["N:\\x\\AEG001_000.flver", "_02_044_PC_TextSelect_fla.Main"]);
        assert!(w.contains(&"aeg001_000".to_string()));
        assert!(w.contains(&"02_044_pc_textselect".to_string()));
    }

    #[test]
    fn a_binder_member_names_its_binder() {
        let mut n = wanting(&[
            "/asset/aeg/aeg001/aeg001_000.geombnd.dcx",
            "/asset/aeg/aeg001/aeg001_000_l.geomhkxbnd.dcx",
            "/map/m60/m60_44_54_10/m60_44_54_10_445401.mapbnd.dcx",
        ]);
        let root = r"N:\GR\data\INTERROOT_win64";
        from_member(
            &mut n,
            &format!(r"{root}\asset\aeg\AEG001\AEG001_000\hkx_L\AEG001_000_L.hkx"),
        );
        from_member(
            &mut n,
            &format!(
                r"{root}\map\m60_44_54_00\m60_44_54_10\m60_44_54_10_445401\Model\m60_44_54_10_445401.flver"
            ),
        );
        assert_eq!(n.named.len(), 3);
        assert!(
            n.named
                .values()
                .any(|p| p == "/map/m60/m60_44_54_10/m60_44_54_10_445401.mapbnd.dcx"),
            "a member can sit under another map's folder"
        );
    }

    #[test]
    fn an_alias_in_the_exe_stands_for_its_folder() {
        let mut n = wanting(&["/chr/c0000_a00_hi.anibnd.dcx", "/event/common.emevd.dcx"]);
        let exe = [
            "chranibnd:/c0000_a00_hi.anibnd".to_string(),
            "event:/common.emevd".to_string(),
            "event:/%s.emevd".to_string(),
        ];
        literal_paths(&mut n, &exe);
        assert_eq!(n.named.len(), 2);
    }

    #[test]
    fn a_map_id_gives_the_files_a_map_has() {
        let m = "/map/m60/m60_43_36_00/";
        let paths: Vec<String> = [
            format!("{m}h60_43_36_00.hkxbhd"),
            format!("{m}m60_43_36_00_Envmap_00_high_03.tpfbnd.dcx"),
            format!("{m}m60_43_36_00_0100.btl.dcx"),
            "/map/breakgeom/lod0/m60_43_36_00_lod0.breakgeom.dcx".to_string(),
            "/map/mapstudio/m60_43_36_00.msb.dcx".to_string(),
        ]
        .into();
        let mut n = Namer::new(paths.iter().map(|p| hash_path(p)));
        from_map_id(&mut n, "m60_43_36_00");
        assert_eq!(n.named.len(), paths.len());
    }

    /// A stored file, compressed, holding a binder, holding text: `walk` reads
    /// through them all.
    #[test]
    fn walking_a_file_reaches_members_and_their_text() {
        let member = fs_bnd4::File {
            flags: 0x40,
            id: Some(0),
            name: Some(r"N:\GR\data\INTERROOT_win64\chr\c1234\c1234.flver".to_string()),
            data: b"\0\0inner_text_here\0\0".to_vec(),
        };
        let bnd = fs_bnd4::Bnd4 {
            header: fs_bnd4::Header {
                unk04: 0,
                unk05: 0,
                unk0a: 1,
                version: *b"07D7R6\0\0",
                raw_format: 0x74,
                hash_table: true,
                unicode: true,
            },
            files: vec![member],
        };
        let bytes = bnd.write().unwrap();
        let stored =
            fs_dcx::compress(&fs_dcx::Header::new(fs_dcx::Algorithm::Zstd), &bytes).unwrap();
        let mut f = Facts::default();
        walk(&stored, &mut f, 0);
        assert_eq!(f.inner, "BND4");
        assert!(f.members[0].ends_with("c1234.flver"));
        assert!(f.strings.contains("inner_text_here"));
    }

    /// The whole of naming over a made-up game: the cache's corpus in, a list
    /// out, and only what hashes to an entry on it.
    #[test]
    fn naming_a_made_up_game_finds_each_kind_of_source() {
        let real = [
            "/event/common.emevd.dcx",
            "/chr/c1234.chrbnd.dcx",
            "/map/mapstudio/m10_00_00_00.msb.dcx",
            "/map/breakgeom/lod0/m10_00_00_00_lod0.breakgeom.dcx",
            "/menu/win/02_044_pc_textselect.gfx",
            "/nowhere/unfindable.bin",
        ];
        let corpus = Corpus {
            entries: real
                .iter()
                .map(|p| (hash_path(p), "Data0".into(), "DCX".into(), "x".into()))
                .collect(),
            members: vec![(
                hash_path("/chr/c1234.chrbnd.dcx"),
                r"N:\GR\data\INTERROOT_win64\chr\c1234\c1234.flver".to_string(),
            )],
            strings: vec![
                "_02_044_PC_TextSelect_fla.MainTimeline".to_string(),
                "m10_00_00_00".to_string(),
            ],
        };
        let exe = ["event:/common.emevd".to_string()];
        let dir = std::env::temp_dir().join(format!("apoc-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The slow sweeps have nothing to find here: a finished cache skips them.
        let slow: String = id_templates()
            .iter()
            .filter(|t| t.bytes().filter(|&c| c == b'#').count() > 4)
            .map(|t| format!("{t}\t\n"))
            .collect();
        std::fs::write(dir.join("sweeps.tsv"), slow).unwrap();
        let n = name_all(&corpus, &exe, &dir, 2).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        for p in &real[..5] {
            assert!(n.named.contains_key(&hash_path(p)), "{p} was not named");
        }
        assert_eq!(n.named.len(), 5, "the unfindable stays unnamed");
    }

    /// With a stock copy, the exe's own text names files that are really in
    /// the archives.
    #[test]
    fn the_real_exe_names_real_entries() {
        let Some(game) = std::env::var_os("APOC_ER_STOCK") else {
            eprintln!("skipped: APOC_ER_STOCK is not set");
            return;
        };
        let game = PathBuf::from(game);
        let dvd = Dvd::open_elden_ring(&game).unwrap();
        let mut n = Namer::new(dvd.entries().map(|(_, e)| e.hash));
        literal_paths(&mut n, &exe_strings(&game).unwrap());
        assert!(n.named.contains_key(&hash_path("/event/common.emevd.dcx")));
        assert!(n.named.len() > 20);
    }
}

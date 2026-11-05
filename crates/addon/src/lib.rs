//! The add-on's module. It runs in the mod manager's frame, where there are no
//! files: it says what it needs, the page fetches that from the mod manager, and
//! it is asked again (`ui/core.js`, `until`).
//!
//! One call in, one answer out: a request as JSON plus some bytes, and JSON plus
//! some bytes back. `{ op: "put", path }` hands over a file or a piece of an archive.
//! `{ op: "run", ... }` answers `{ need: [...] }` until it has all it needs, then
//! `{ slots, spoiler, files: [{ path, size }] }` with the files' bytes after the JSON.
//!
//! A piece is named `Data0.bdt#offset+length`. What another mod holds is `mod:<path>`, and
//! that mod's tables `mod#<n>`, the n-th mod that has any, lowest priority first.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fs_bytes::{Result, format_err};
use fs_dvdbnd::{Archive, ArchiveKey, Bdt, Dvd, ELDEN_RING_ARCHIVES};
use fs_param::Regulation;
use gamedata::model::Model;
use gamedata::{GameFiles, Sources, elden_ring};
use rando::{EnemyOptions, ItemOptions, Job, LANGUAGES, Sink};
use serde::{Deserialize, Serialize};

/// The pieces of archives handed over so far, and those asked for and not there.
#[derive(Debug, Default)]
struct Pieces {
    held: HashMap<String, Vec<u8>>,
    missing: Vec<String>,
}

type Shared = Arc<Mutex<Pieces>>;

fn piece(archive: &str, at: u64, length: usize) -> String {
    format!("{archive}.bdt#{at}+{length}")
}

/// An archive's data, as far as it has been handed over.
#[derive(Debug)]
struct FedBdt {
    name: String,
    pieces: Shared,
}

impl Bdt for FedBdt {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
        let name = piece(&self.name, offset, buf.len());
        let mut pieces = self.pieces.lock().unwrap();
        match pieces.held.get(&name) {
            Some(bytes) if bytes.len() == buf.len() => {
                buf.copy_from_slice(bytes);
                Ok(())
            }
            _ => {
                pieces.missing.push(name);
                Err(std::io::ErrorKind::NotFound.into())
            }
        }
    }
}

/// What is kept between seeds on the same mods, so a second one reads nothing again.
struct Loaded {
    stack: Stack,
    sources: Sources,
    model: Model,
    /// The spoiler log's opening, saying what the seed stands on.
    merged: String,
}

#[derive(Default)]
struct State {
    whole: HashMap<String, Vec<u8>>,
    pieces: Shared,
    files: Option<GameFiles>,
    asked_all: bool,
    loaded: Option<Loaded>,
    /// The unmodded names of bosses, read once when mods first come into it.
    names: Option<rando::BossNames>,
}

/// A file another mod puts in the game.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct StackFile {
    path: String,
    size: u64,
}

/// What the other mods hold, as the mod manager will place it, the seed apart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct Stack {
    #[serde(default)]
    files: Vec<StackFile>,
    /// The mods with tables, lowest priority first.
    #[serde(default)]
    regulations: Vec<String>,
    /// Their names, for the page. Not part of what a seed is built from.
    #[serde(default)]
    mods: Vec<String>,
}

impl Stack {
    fn same_files(&self, other: &Stack) -> bool {
        self.files == other.files && self.regulations == other.regulations
    }

    fn names(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|f| format!("mod:{}", f.path))
            .chain((0..self.regulations.len()).map(|i| format!("mod#{i}")))
            .collect()
    }
}

/// One seed, as the page's form gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    seed: u64,
    /// As `apoc-rando --shuffle`: `all=50,keys=100`.
    #[serde(default)]
    shuffle: String,
    #[serde(default)]
    bias: String,
    #[serde(default)]
    dlc_separate: bool,
    /// As `apoc-rando --enemies`.
    #[serde(default)]
    enemies: String,
    /// As `apoc-rando --no-enemy-scaling`.
    #[serde(default)]
    no_scaling: bool,
    /// The page places the native module; this says so in the spoiler log.
    #[serde(default)]
    auto_upgrade: bool,
    /// As `apoc-rando --leyndell-runes`, 0 to 4.
    #[serde(default = "game_runes")]
    leyndell_runes: u8,
    /// As `apoc-rando --keep-whetblades`.
    #[serde(default)]
    keep_whetblades: bool,
    /// As `apoc-rando --keep-maps`.
    #[serde(default)]
    keep_maps: bool,
    /// As `apoc-rando --whole-map`.
    #[serde(default)]
    whole_map: bool,
    #[serde(default)]
    stack: Stack,
}

/// What Leyndell's gate asks in the game itself.
fn game_runes() -> u8 {
    rando::DEFAULT_LEYNDELL_RUNES
}

#[derive(Debug, Serialize)]
struct FileOut {
    path: String,
    size: usize,
}

#[derive(Default)]
struct Written(Mutex<Vec<(String, Vec<u8>)>>);

impl Sink for Written {
    fn put(&self, path: &str, bytes: Vec<u8>) -> Result<()> {
        self.0.lock().unwrap().push((path.to_string(), bytes));
        Ok(())
    }
}

/// The header files and tables, by the names the game folder has for them.
fn first_files() -> Vec<String> {
    let mut names = vec!["regulation.bin".to_string(), "eldenring.exe".to_string()];
    names.extend(ELDEN_RING_ARCHIVES.iter().map(|a| format!("{a}.bhd")));
    names
}

/// Every piece a seed is known to read.
fn known_pieces(files: &GameFiles) -> Vec<String> {
    let mut paths = Sources::paths(files);
    paths.extend(rando::far::paths(files));
    for language in LANGUAGES {
        for binder in ["item_dlc02", "menu_dlc02"] {
            paths.push(format!("/msg/{language}/{binder}.msgbnd.dcx"));
        }
    }
    let mut pieces: Vec<String> = paths
        .iter()
        .filter_map(|p| files.place(p))
        .map(|(archive, at, length)| piece(archive, at, length as usize))
        .collect();
    pieces.sort();
    pieces.dedup();
    pieces
}

fn open_files(state: &State) -> Result<GameFiles> {
    let keys = ArchiveKey::all_in_exe(&state.whole["eldenring.exe"])?;
    let archives = ELDEN_RING_ARCHIVES
        .iter()
        .map(|name| {
            let bdt = FedBdt {
                name: name.to_string(),
                pieces: state.pieces.clone(),
            };
            // The mod manager refuses a read past the end, so the length need not be known.
            Archive::from_header(
                name,
                &state.whole[&format!("{name}.bhd")],
                &keys,
                Box::new(bdt),
                u64::MAX,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(GameFiles::over(Dvd::from_archives(archives)))
}

fn need(paths: Vec<String>) -> Answer {
    Answer {
        json: serde_json::json!({ "need": paths }),
        bytes: Vec::new(),
    }
}

struct Answer {
    json: serde_json::Value,
    bytes: Vec<u8>,
}

/// What failed because a piece was not there yet, if that is why.
fn asked_for_more(pieces: &Shared) -> Option<Answer> {
    let mut pieces = pieces.lock().unwrap();
    if pieces.missing.is_empty() {
        return None;
    }
    let mut missing = std::mem::take(&mut pieces.missing);
    missing.sort();
    missing.dedup();
    Some(need(missing))
}

fn stock_tables(state: &State) -> Result<([u8; 32], &[u8])> {
    let base = &state.whole["regulation.bin"];
    Ok((
        fs_param::crypto::find_key(&state.whole["eldenring.exe"], base)?,
        base,
    ))
}

/// The other mods' tables, lowest priority first.
fn contributions(state: &State, stack: &Stack) -> Vec<rando::Contribution> {
    stack
        .regulations
        .iter()
        .enumerate()
        .map(|(i, label)| rando::Contribution {
            label: label.clone(),
            bytes: state.whole[&format!("mod#{i}")].clone(),
        })
        .collect()
}

/// Read the game as the other mods leave it. Some answer if pieces are still wanted.
fn load(state: &mut State, stack: &Stack) -> Result<Option<Answer>> {
    let (key, base) = stock_tables(state)?;
    let regulation = Regulation::read(&key, base)?;
    rando::known_patch(&regulation.version())?;
    let files = state.files.as_mut().expect("opened above");
    files.hold(std::iter::empty());
    let stacked = !stack.files.is_empty() || !stack.regulations.is_empty();
    // The unmodded names first, and let go of before the mods' copy is read: both at once
    // would come near what a module's memory can hold.
    if stacked && state.names.is_none() {
        let stock = match Sources::read(regulation.clone(), files) {
            Ok(stock) => stock,
            Err(e) => return asked_for_more(&state.pieces).map(Some).ok_or(e),
        };
        state.names = Some(rando::boss_names(elden_ring::extract(&stock)?));
    }
    let held: Vec<(String, Vec<u8>)> = stack
        .files
        .iter()
        .map(|f| {
            (
                f.path.clone(),
                state.whole[&format!("mod:{}", f.path)].clone(),
            )
        })
        .collect();
    state.files.as_mut().expect("opened above").hold(held);
    let (regulation, merged) = if stack.regulations.is_empty() {
        (regulation, String::new())
    } else {
        rando::merged_tables(
            &key,
            &state.whole["regulation.bin"],
            &contributions(state, stack),
        )?
    };
    let files = state.files.as_ref().expect("opened above");
    let sources = match Sources::read(regulation, files) {
        Ok(sources) => sources,
        Err(e) => return asked_for_more(&state.pieces).map(Some).ok_or(e),
    };
    let mut model = elden_ring::extract(&sources)?;
    if let Some(names) = state.names.as_ref().filter(|_| stacked) {
        rando::keep_boss_names(&mut model, names);
    }
    state.loaded = Some(Loaded {
        stack: stack.clone(),
        sources,
        model,
        merged,
    });
    Ok(None)
}

fn run(state: &mut State, request: &Run) -> Result<Answer> {
    // The settings first, so one that is refused is said before any file is asked for.
    let mut options = ItemOptions::default();
    options.set(false, &request.shuffle).map_err(format_err)?;
    options.set(true, &request.bias).map_err(format_err)?;
    options.dlc_separate = request.dlc_separate;
    options.auto_upgrade = request.auto_upgrade;
    options.keep_whetblades = request.keep_whetblades;
    options.keep_maps = request.keep_maps;
    options.whole_map = request.whole_map;
    options
        .set_leyndell_runes(request.leyndell_runes)
        .map_err(format_err)?;
    let mut foes = EnemyOptions::default();
    foes.set(&request.enemies).map_err(format_err)?;
    foes.dlc_separate = true;
    foes.scale = !request.no_scaling;
    let absent: Vec<String> = first_files()
        .into_iter()
        .filter(|f| !state.whole.contains_key(f))
        .collect();
    if !absent.is_empty() {
        return Ok(need(absent));
    }
    if state.files.is_none() {
        state.files = Some(open_files(state)?);
    }
    if !state.asked_all {
        state.asked_all = true;
        return Ok(need(known_pieces(
            state.files.as_ref().expect("opened above"),
        )));
    }
    let absent: Vec<String> = request
        .stack
        .names()
        .into_iter()
        .filter(|n| !state.whole.contains_key(n))
        .collect();
    if !absent.is_empty() {
        return Ok(need(absent));
    }
    if state
        .loaded
        .as_ref()
        .is_none_or(|l| !l.stack.same_files(&request.stack))
    {
        state.loaded = None;
        if let Some(early) = load(state, &request.stack)? {
            return Ok(early);
        }
    }
    let written = Written::default();
    let loaded = state.loaded.as_ref().expect("loaded above");
    let files = state.files.as_ref().expect("opened above");
    let built = rando::build(
        files,
        &loaded.sources,
        &loaded.model,
        Job {
            seed: request.seed,
            options: &options,
            enemies: &foes,
        },
        &written,
        &|_| {},
    );
    let built = match built {
        Ok(built) => built,
        Err(e) => return asked_for_more(&state.pieces).ok_or(e),
    };
    let mut out = written.0.into_inner().unwrap();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    if !loaded.stack.regulations.is_empty() {
        let seed = out
            .iter()
            .find(|(path, _)| path == "regulation.bin")
            .ok_or_else(|| format_err("the seed wrote no tables"))?;
        let (key, base) = stock_tables(state)?;
        rando::survives_merge(&key, base, &seed.1)?;
    }
    let files: Vec<FileOut> = out
        .iter()
        .map(|(path, bytes)| FileOut {
            path: path.clone(),
            size: bytes.len(),
        })
        .collect();
    Ok(Answer {
        json: serde_json::json!({
            "slots": built.slots,
            "spoiler": format!("{}{}", loaded.merged, built.spoiler),
            "files": files,
        }),
        bytes: out.into_iter().flat_map(|(_, bytes)| bytes).collect(),
    })
}

fn answer(state: &mut State, request: &[u8], data: &[u8]) -> Result<Answer> {
    let value: serde_json::Value =
        serde_json::from_slice(request).map_err(|e| format_err(e.to_string()))?;
    match value["op"].as_str() {
        Some("put") => {
            let path = value["path"].as_str().unwrap_or_default().to_string();
            if value["absent"].as_bool().unwrap_or(false) {
                return Err(format_err(format!("the game has no {path}")));
            }
            if path.contains('#') && !path.starts_with("mod") {
                let mut pieces = state.pieces.lock().unwrap();
                pieces.held.insert(path, data.to_vec());
            } else {
                state.whole.insert(path, data.to_vec());
            }
            Ok(Answer {
                json: serde_json::json!({}),
                bytes: Vec::new(),
            })
        }
        // The page's first call, which only shows the module has started.
        Some("files") => Ok(Answer {
            json: serde_json::json!({}),
            bytes: Vec::new(),
        }),
        Some("run") => {
            let request: Run =
                serde_json::from_value(value).map_err(|e| format_err(e.to_string()))?;
            run(state, &request)
        }
        other => Err(format_err(format!("no request {other:?}"))),
    }
}

fn encode(answer: Answer) -> Vec<u8> {
    let json = serde_json::to_vec(&answer.json).unwrap_or_default();
    let mut out = Vec::with_capacity(4 + json.len() + answer.bytes.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&answer.bytes);
    out
}

/// One call. A failure is an answer with `error`, for the page to show.
pub fn call_bytes(state: &mut StateBox, request: &[u8], data: &[u8]) -> Vec<u8> {
    encode(
        answer(&mut state.0, request, data).unwrap_or_else(|e| Answer {
            json: serde_json::json!({ "error": e.to_string() }),
            bytes: Vec::new(),
        }),
    )
}

/// The module's state, opaque to what calls it.
#[derive(Default)]
pub struct StateBox(State);

#[cfg(target_family = "wasm")]
mod exports {
    use super::StateBox;
    use std::cell::RefCell;

    thread_local! {
        static STATE: RefCell<StateBox> = RefCell::default();
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn alloc(length: usize) -> *mut u8 {
        let mut bytes = Vec::<u8>::with_capacity(length.max(1));
        let at = bytes.as_mut_ptr();
        std::mem::forget(bytes);
        at
    }

    /// # Safety
    /// `at` and `length` are what `alloc` was given and gave.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn dealloc(at: *mut u8, length: usize) {
        unsafe { drop(Vec::from_raw_parts(at, 0, length.max(1))) }
    }

    /// # Safety
    /// Both pairs are from `alloc`, with that many bytes written.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn call(
        request: *const u8,
        request_length: usize,
        data: *const u8,
        data_length: usize,
    ) -> u64 {
        let request = unsafe { std::slice::from_raw_parts(request, request_length) };
        let data = unsafe { std::slice::from_raw_parts(data, data_length) };
        let answer = STATE.with(|s| super::call_bytes(&mut s.borrow_mut(), request, data));
        // `dealloc` is told the length, so the capacity must equal it.
        let mut boxed = answer.into_boxed_slice();
        let (at, length) = (boxed.as_mut_ptr() as u64, boxed.len() as u64);
        std::mem::forget(boxed);
        (at << 32) | length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(answer: Vec<u8>) -> serde_json::Value {
        let n = u32::from_le_bytes(answer[..4].try_into().unwrap()) as usize;
        serde_json::from_slice(&answer[4..4 + n]).unwrap()
    }

    #[test]
    fn a_run_first_asks_for_the_tables_and_headers() {
        let mut state = StateBox::default();
        let answer = call_bytes(&mut state, br#"{"op":"run","seed":1}"#, b"");
        let need = decode(answer)["need"].clone();
        assert_eq!(need[0], "regulation.bin");
        assert_eq!(need[1], "eldenring.exe");
        assert_eq!(need[2], "Data0.bhd");
        assert_eq!(need.as_array().unwrap().len(), 7);
    }

    #[test]
    fn what_was_handed_over_is_not_asked_for_again() {
        let mut state = StateBox::default();
        call_bytes(&mut state, br#"{"op":"put","path":"regulation.bin"}"#, b"x");
        let answer = call_bytes(&mut state, br#"{"op":"run","seed":1}"#, b"");
        assert_eq!(decode(answer)["need"][0], "eldenring.exe");
    }

    #[test]
    fn a_file_the_game_lacks_is_said() {
        let mut state = StateBox::default();
        let answer = call_bytes(
            &mut state,
            br#"{"op":"put","path":"Data0.bhd","absent":true}"#,
            b"",
        );
        let text = decode(answer)["error"].as_str().unwrap().to_string();
        assert!(text.contains("Data0.bhd"), "{text}");
    }

    #[test]
    fn a_bad_setting_is_said() {
        let bad = br#"{"op":"run","seed":"one"}"#;
        let mut state = StateBox::default();
        assert!(decode(call_bytes(&mut state, bad, b""))["error"].is_string());
    }

    #[test]
    fn a_bias_for_a_kind_of_slot_is_refused_before_anything_is_asked() {
        let mut state = StateBox::default();
        let answer = decode(call_bytes(
            &mut state,
            br#"{"op":"run","seed":1,"bias":"keys=10,merchants=80"}"#,
            b"",
        ));
        let text = answer["error"].as_str().unwrap();
        assert!(text.contains("lean does not apply to merchants"), "{text}");
        // `all=` passes by what does not lean, and the run goes on to ask for files.
        let answer = decode(call_bytes(
            &mut state,
            br#"{"op":"run","seed":1,"bias":"all=50"}"#,
            b"",
        ));
        assert!(answer["need"].is_array(), "{answer}");
    }

    #[test]
    fn a_piece_reads_only_at_its_own_place() {
        let pieces = Shared::default();
        pieces
            .lock()
            .unwrap()
            .held
            .insert(piece("Data0", 16, 2), vec![1, 2]);
        let bdt = FedBdt {
            name: "Data0".into(),
            pieces: pieces.clone(),
        };
        let mut buf = [0; 2];
        bdt.read_at(&mut buf, 16).unwrap();
        assert_eq!(buf, [1, 2]);
        assert!(bdt.read_at(&mut buf, 18).is_err());
        assert_eq!(pieces.lock().unwrap().missing, ["Data0.bdt#18+2"]);
    }

    #[test]
    fn a_mods_tables_are_kept_whole_and_not_taken_for_a_piece() {
        let mut state = StateBox::default();
        call_bytes(&mut state, br#"{"op":"put","path":"mod#0"}"#, b"x");
        assert_eq!(state.0.whole["mod#0"], b"x");
        assert!(state.0.pieces.lock().unwrap().held.is_empty());
    }

    #[test]
    fn a_stack_names_its_files_and_each_mods_tables() {
        let stack = Stack {
            files: vec![StackFile {
                path: "map/mapstudio/m10.msb.dcx".into(),
                size: 1,
            }],
            regulations: vec!["A".into(), "B".into()],
            mods: vec![],
        };
        assert_eq!(
            stack.names(),
            ["mod:map/mapstudio/m10.msb.dcx", "mod#0", "mod#1"]
        );
    }
}

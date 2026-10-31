//! The game's main archives: `Data0`–`Data3` and `DLC`, each a `.bhd` header
//! and a `.bdt` of file data.
//!
//! Observed on Elden Ring 1.17.2 (exe 2.7.1.0, Steam build 25080141):
//!
//! 1. Each `.bhd` is RSA-encrypted with a 2048-bit public key, raw (no
//!    padding): every 256-byte block decrypts to a zero byte followed by 255
//!    bytes of plaintext ([`key`]).
//! 2. The game carries those public keys as PEM text in `eldenring.exe`, five of
//!    them, one per archive. They are read from the player's own exe, so no key
//!    lives in this repository. Which key opens which header is found by trying
//!    each one ([`key::ArchiveKey::opens`]), not by assuming an order.
//! 3. The plaintext is a `BHD5` table of buckets of 40-byte file entries
//!    ([`bhd`]), keyed by a 64-bit hash of the file's path ([`name`]).
//! 4. A file's bytes sit at its offset in the `.bdt`, padded, with listed ranges
//!    AES-128-ECB-encrypted under a per-file key ([`archive`]). The DLC archive
//!    encrypts nothing.

pub mod archive;
pub mod bhd;
pub mod key;
pub mod name;

pub use archive::{Archive, Bdt, Dvd, EntryRef};
pub use bhd::{Bhd, Entry};
pub use key::ArchiveKey;
pub use name::hash_path;

/// The archives of Elden Ring with Shadow of the Erdtree, by file stem.
pub const ELDEN_RING_ARCHIVES: [&str; 5] = ["Data0", "Data1", "Data2", "Data3", "DLC"];

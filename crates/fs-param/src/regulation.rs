//! `regulation.bin`: AES-256-CBC around a zstd DCX around a BND4 of param
//! tables, one file per table.

use std::path::Path;

use fs_bnd4::Bnd4;
use fs_bytes::{Error, Result, format_err};

use crate::crypto;
use crate::param::Param;

/// A decrypted, decompressed `regulation.bin`.
#[derive(Debug, Clone)]
pub struct Regulation {
    key: [u8; 32],
    dcx: fs_dcx::Header,
    pub bnd: Bnd4,
}

impl Regulation {
    /// Open the file with a known key.
    pub fn read(key: &[u8; 32], file: &[u8]) -> Result<Self> {
        let packed = crypto::decrypt(key, file)?;
        let (dcx, data) = fs_dcx::decompress(&packed)?;
        let bnd = Bnd4::read(&data)?;
        Ok(Regulation {
            key: *key,
            dcx,
            bnd,
        })
    }

    /// Open `regulation.bin` with the key found in `eldenring.exe`, both from
    /// one game folder.
    pub fn open(game_dir: &Path) -> Result<Self> {
        let read = |name: &str| {
            let path = game_dir.join(name);
            std::fs::read(&path).map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("{}: {e}", path.display()),
                ))
            })
        };
        let file = read("regulation.bin")?;
        let key = crypto::find_key(&read("eldenring.exe")?, &file)?;
        Self::read(&key, &file)
    }

    /// The version stamp, e.g. `11720000`.
    pub fn version(&self) -> String {
        self.bnd.header.version_str()
    }

    /// The tables, by file name without extension, e.g. `ItemLotParam_map`.
    pub fn table_names(&self) -> impl Iterator<Item = &str> {
        self.bnd.files.iter().filter_map(table_name)
    }

    /// Parse one table.
    pub fn table(&self, name: &str) -> Result<Param> {
        let f = self
            .bnd
            .files
            .iter()
            .find(|f| table_name(f) == Some(name))
            .ok_or_else(|| format_err(format!("regulation.bin has no table {name}")))?;
        Param::read(&f.data).map_err(|e| format_err(format!("{name}: {e}")))
    }

    /// Replace one table's bytes.
    pub fn set_table(&mut self, name: &str, param: &Param) -> Result<()> {
        let bytes = param.write()?;
        let f = self
            .bnd
            .files
            .iter_mut()
            .find(|f| table_name(f) == Some(name))
            .ok_or_else(|| format_err(format!("regulation.bin has no table {name}")))?;
        f.data = bytes;
        Ok(())
    }

    /// A file made here, for tests.
    #[cfg(test)]
    pub(crate) fn from_parts(key: [u8; 32], bnd: Bnd4) -> Self {
        Regulation {
            key,
            dcx: fs_dcx::Header::new(fs_dcx::Algorithm::Zstd),
            bnd,
        }
    }

    /// Write the file back: BND4, zstd DCX, encrypted.
    pub fn write(&self) -> Result<Vec<u8>> {
        let packed = fs_dcx::compress(&self.dcx, &self.bnd.write()?)?;
        Ok(crypto::encrypt(&self.key, &packed))
    }
}

fn table_name(f: &fs_bnd4::File) -> Option<&str> {
    let short = f.short_name()?;
    Some(short.strip_suffix(".param").unwrap_or(short))
}

/// A version as people read it: `11711000` is 1.17.1. Anything not in that
/// shape is shown as stored.
pub fn display_version(stamp: &str) -> String {
    let d: Vec<u32> = stamp.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() == 8 && stamp.len() == 8 && d[0] != 0 {
        format!("{}.{}.{}", d[0], d[1] * 10 + d[2], d[3])
    } else {
        stamp.to_string()
    }
}

//! The encryption around `regulation.bin`: AES-256-CBC, the IV in the first 16
//! bytes (zero in the game's file).
//!
//! The key is not stored here. The game keeps it as 32 plain bytes in
//! `eldenring.exe`, and [`find_key`] finds it by trying each aligned window
//! until one decrypts the file's first block to a DCX header. Behaviour lifted
//! from `apoc-fsparams` (`crypto.rs`), with CBC written out over `aes` so no
//! other crate is needed.

use aes::Aes256;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use fs_bytes::{Error, Result};
use rayon::prelude::*;

pub const IV_LEN: usize = 16;
const BLOCK: usize = 16;
/// The key's alignment in the exe. It sits on a 16-byte boundary in build
/// 25080141; 4 is searched so a patch that moves it a little still finds it.
const KEY_SEARCH_STEP: usize = 4;

fn cipher(key: &[u8; 32]) -> Aes256 {
    Aes256::new(GenericArray::from_slice(key))
}

/// Decrypt. The tail of a body that is not whole blocks is padded with zeros
/// first; whatever it decrypts to lies past the DCX data and is never read.
pub fn decrypt(key: &[u8; 32], file: &[u8]) -> Result<Vec<u8>> {
    if file.len() <= IV_LEN {
        return Err(Error::Truncated("encrypted file"));
    }
    let (iv, body) = file.split_at(IV_LEN);
    let mut out = body.to_vec();
    out.resize(body.len().div_ceil(BLOCK) * BLOCK, 0);
    let c = cipher(key);
    let mut prev: [u8; BLOCK] = iv.try_into().unwrap();
    for block in out.chunks_exact_mut(BLOCK) {
        let ct: [u8; BLOCK] = block.try_into().unwrap();
        c.decrypt_block(GenericArray::from_mut_slice(block));
        block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
        prev = ct;
    }
    Ok(out)
}

/// Encrypt under a zero IV with PKCS#7 padding, as the game's file is.
pub fn encrypt(key: &[u8; 32], plain: &[u8]) -> Vec<u8> {
    let pad = BLOCK - plain.len() % BLOCK;
    let mut body = plain.to_vec();
    body.extend(std::iter::repeat_n(pad as u8, pad));
    let c = cipher(key);
    let mut prev = [0u8; BLOCK];
    for block in body.chunks_exact_mut(BLOCK) {
        block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
        c.encrypt_block(GenericArray::from_mut_slice(block));
        prev = block.try_into().unwrap();
    }
    let mut out = vec![0u8; IV_LEN];
    out.extend(body);
    out
}

/// Whether `key` decrypts the file's first block to the start of a DCX header.
fn opens(key: &[u8; 32], file: &[u8]) -> bool {
    let Some(first) = file.get(IV_LEN..IV_LEN + BLOCK) else {
        return false;
    };
    let mut block = GenericArray::clone_from_slice(first);
    cipher(key).decrypt_block(&mut block);
    block
        .iter_mut()
        .zip(&file[..IV_LEN])
        .for_each(|(b, p)| *b ^= p);
    block.starts_with(b"DCX\0\x00\x01\x10\x00")
}

/// The key that opens `regulation`, found in `exe`.
pub fn find_key(exe: &[u8], regulation: &[u8]) -> Result<[u8; 32]> {
    (0..exe.len().saturating_sub(31))
        .into_par_iter()
        .step_by(KEY_SEARCH_STEP)
        .find_first(|&at| opens(exe[at..at + 32].try_into().unwrap(), regulation))
        .map(|at| exe[at..at + 32].try_into().unwrap())
        .ok_or_else(|| Error::Crypto("no 32 bytes of the exe open regulation.bin".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dcx_like() -> Vec<u8> {
        let mut plain = b"DCX\0\x00\x01\x10\x00".to_vec();
        plain.extend((0..1000u32).map(|i| (i * 31) as u8));
        plain
    }

    #[test]
    fn what_is_encrypted_decrypts_back_followed_only_by_padding() {
        let key = [7u8; 32];
        let plain = dcx_like();
        let file = encrypt(&key, &plain);
        assert_eq!(
            &file[..IV_LEN],
            &[0u8; IV_LEN],
            "the game's own files use a zero IV"
        );
        let back = decrypt(&key, &file).unwrap();
        assert_eq!(&back[..plain.len()], &plain[..]);
        assert!(back.len() - plain.len() <= 16);
        assert!(decrypt(&key, &[0; 10]).is_err());
    }

    #[test]
    fn the_key_is_found_wherever_it_sits_in_the_exe() {
        let key: [u8; 32] = std::array::from_fn(|i| (i as u8).wrapping_mul(73).wrapping_add(5));
        let file = encrypt(&key, &dcx_like());
        let mut exe: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
        exe[1232..1264].copy_from_slice(&key);
        assert_eq!(find_key(&exe, &file).unwrap(), key);
        assert!(find_key(&exe[..1000], &file).is_err());
    }
}

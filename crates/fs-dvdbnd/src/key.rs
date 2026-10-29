//! The archive keys: RSA public keys read from the game's exe, and the raw RSA
//! that opens a `.bhd` with one.

use base64::Engine;
use fs_bytes::{Error, Result, format_err};
use num_bigint::BigUint;
use rayon::prelude::*;

const PEM_BEGIN: &[u8] = b"-----BEGIN RSA PUBLIC KEY-----";
const PEM_END: &[u8] = b"-----END RSA PUBLIC KEY-----";
/// The longest PEM body accepted. The game's run to about 400 bytes.
const MAX_PEM: usize = 4096;
/// The magic a decrypted header starts with, after the block's zero byte.
const BHD5: &[u8] = b"BHD5";

/// One archive's public key.
#[derive(Clone, PartialEq, Eq)]
pub struct ArchiveKey {
    n: BigUint,
    e: BigUint,
    /// The modulus length in bytes: the size of one encrypted block.
    block: usize,
}

impl std::fmt::Debug for ArchiveKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ArchiveKey({} bits)", self.n.bits())
    }
}

impl ArchiveKey {
    /// A key from a PKCS#1 `RSA PUBLIC KEY` PEM block.
    pub fn from_pem(pem: &[u8]) -> Result<Self> {
        let start =
            find(pem, PEM_BEGIN).ok_or_else(|| format_err("no RSA public key in the PEM"))?;
        let body_at = start + PEM_BEGIN.len();
        let end = find(&pem[body_at..], PEM_END)
            .ok_or_else(|| format_err("the RSA public key has no end line"))?;
        let body: Vec<u8> = pem[body_at..body_at + end]
            .iter()
            .copied()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(&body)
            .map_err(|e| format_err(format!("the RSA public key is not base64: {e}")))?;
        Self::from_der(&der)
    }

    /// A key from DER: `SEQUENCE { INTEGER n, INTEGER e }`.
    pub fn from_der(der: &[u8]) -> Result<Self> {
        let (tag, seq, _) = der_item(der)?;
        if tag != 0x30 {
            return Err(format_err("the RSA public key is not a DER sequence"));
        }
        let (tag, n, rest) = der_item(seq)?;
        let (tag2, e, _) = der_item(rest)?;
        if tag != 0x02 || tag2 != 0x02 {
            return Err(format_err("the RSA public key does not hold two integers"));
        }
        let n = BigUint::from_bytes_be(n);
        let e = BigUint::from_bytes_be(e);
        let block = n.bits().div_ceil(8) as usize;
        if !(16..=1024).contains(&block) || e == BigUint::ZERO {
            return Err(format_err("the RSA public key has an implausible size"));
        }
        Ok(ArchiveKey { n, e, block })
    }

    /// Every RSA public key in the exe, in the order they appear.
    pub fn all_in_exe(exe: &[u8]) -> Result<Vec<Self>> {
        let mut keys = Vec::new();
        let mut at = 0;
        while let Some(i) = find(&exe[at..], PEM_BEGIN) {
            let start = at + i;
            let window = &exe[start..exe.len().min(start + MAX_PEM)];
            let end = find(window, PEM_END)
                .ok_or_else(|| format_err("an RSA public key in the exe has no end line"))?;
            keys.push(Self::from_pem(&window[..end + PEM_END.len()])?);
            at = start + end + PEM_END.len();
        }
        if keys.is_empty() {
            return Err(format_err("the exe holds no RSA public keys"));
        }
        Ok(keys)
    }

    fn decrypt_block(&self, block: &[u8]) -> Result<Vec<u8>> {
        let c = BigUint::from_bytes_be(block);
        if c >= self.n {
            return Err(Error::Crypto(
                "a block is not below the key's modulus".into(),
            ));
        }
        let m = c.modpow(&self.e, &self.n).to_bytes_be();
        // `to_bytes_be` drops leading zeros. The block always has at least one,
        // and a plaintext block is exactly one byte shorter than the key.
        let plain = self.block - 1;
        if m.len() > plain {
            return Err(Error::Crypto(
                "a block decrypts to more than the key allows".into(),
            ));
        }
        let mut out = vec![0u8; plain - m.len()];
        out.extend_from_slice(&m);
        Ok(out)
    }

    /// Whether this key opens the header, judged from its first block alone.
    pub fn opens(&self, encrypted: &[u8]) -> bool {
        encrypted
            .get(..self.block)
            .and_then(|b| self.decrypt_block(b).ok())
            .is_some_and(|m| m.starts_with(BHD5))
    }

    /// Decrypt a whole `.bhd`.
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>> {
        if encrypted.is_empty() || !encrypted.len().is_multiple_of(self.block) {
            return Err(Error::Crypto(format!(
                "the header is {} bytes, not a whole number of {}-byte blocks",
                encrypted.len(),
                self.block
            )));
        }
        let blocks: Vec<Vec<u8>> = encrypted
            .par_chunks(self.block)
            .map(|b| self.decrypt_block(b))
            .collect::<Result<_>>()?;
        Ok(blocks.concat())
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// One DER item: its tag, its contents, and what follows it.
fn der_item(b: &[u8]) -> Result<(u8, &[u8], &[u8])> {
    let short = || format_err("the RSA public key's DER is cut short");
    let tag = *b.first().ok_or_else(short)?;
    let first = *b.get(1).ok_or_else(short)?;
    let (len, at) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 {
            return Err(format_err(
                "the RSA public key's DER has an unusable length",
            ));
        }
        let len = b
            .get(2..2 + n)
            .ok_or_else(short)?
            .iter()
            .fold(0usize, |acc, &x| acc << 8 | usize::from(x));
        (len, 2 + n)
    };
    let body = b.get(at..at + len).ok_or_else(short)?;
    Ok((tag, body, &b[at + len..]))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small key for tests, with its private exponent, so a test can encrypt
    /// what the archive reader then decrypts. Not from any game.
    ///
    /// p = 2^127 - 1 and q = 2^89 - 1 are Mersenne primes, so n is 216 bits and
    /// a block is 27 bytes.
    pub(crate) struct TestKey {
        pub public: ArchiveKey,
        d: BigUint,
    }

    impl TestKey {
        pub fn new() -> Self {
            let one = BigUint::from(1u32);
            let p = (BigUint::from(1u32) << 127u32) - &one;
            let q = (BigUint::from(1u32) << 89u32) - &one;
            let n = &p * &q;
            let phi = (&p - &one) * (&q - &one);
            let e = BigUint::from(65537u32);
            let d = e.modinv(&phi).expect("e is coprime with phi");
            let block = n.bits().div_ceil(8) as usize;
            TestKey {
                public: ArchiveKey { n, e, block },
                d,
            }
        }

        /// Encrypt with the private exponent, the way the game's headers are.
        pub fn encrypt(&self, plain: &[u8]) -> Vec<u8> {
            let k = &self.public;
            let mut out = Vec::new();
            for chunk in plain.chunks(k.block - 1) {
                let mut padded = chunk.to_vec();
                padded.resize(k.block - 1, 0);
                let c = BigUint::from_bytes_be(&padded).modpow(&self.d, &k.n);
                let bytes = c.to_bytes_be();
                out.extend(std::iter::repeat_n(0, k.block - bytes.len()));
                out.extend_from_slice(&bytes);
            }
            out
        }

        /// The public key as a PEM block.
        pub fn pem(&self) -> Vec<u8> {
            let k = &self.public;
            let int = |v: &BigUint| {
                let mut b = v.to_bytes_be();
                if b[0] & 0x80 != 0 {
                    b.insert(0, 0);
                }
                let mut out = vec![0x02, b.len() as u8];
                out.extend(b);
                out
            };
            let mut body = int(&k.n);
            body.extend(int(&k.e));
            let mut der = vec![0x30, body.len() as u8];
            der.extend(body);
            let b64 = base64::engine::general_purpose::STANDARD.encode(der);
            format!("-----BEGIN RSA PUBLIC KEY-----\n{b64}\n-----END RSA PUBLIC KEY-----\n")
                .into_bytes()
        }
    }

    #[test]
    fn a_header_encrypted_with_the_private_key_decrypts_with_the_public_one() {
        let k = TestKey::new();
        let plain: Vec<u8> = b"BHD5"
            .iter()
            .copied()
            .chain((0..200u8).map(|i| i ^ 0x5a))
            .collect();
        let enc = k.encrypt(&plain);
        assert!(k.public.opens(&enc));
        let back = k.public.decrypt(&enc).unwrap();
        assert_eq!(&back[..plain.len()], &plain[..]);
        assert!(
            back[plain.len()..].iter().all(|&b| b == 0),
            "only padding after"
        );
    }

    #[test]
    fn keys_are_found_in_an_exe_in_the_order_they_appear() {
        let k = TestKey::new();
        let mut exe = b"MZ\x90\0 junk".to_vec();
        exe.extend(k.pem());
        exe.extend_from_slice(b"\0\0more junk\0");
        exe.extend(k.pem());
        let keys = ArchiveKey::all_in_exe(&exe).unwrap();
        assert_eq!(keys, vec![k.public.clone(), k.public.clone()]);
        assert!(ArchiveKey::all_in_exe(b"MZ nothing here").is_err());
    }

    #[test]
    fn a_key_that_does_not_fit_does_not_claim_the_header() {
        let k = TestKey::new();
        let enc = k.encrypt(b"BHD5 and the rest");
        let mut other = k.public.clone();
        other.e = BigUint::from(3u32);
        assert!(!other.opens(&enc));
        assert!(!k.public.opens(&enc[..5]), "too short to hold a block");
    }

    #[test]
    fn a_header_that_is_not_whole_blocks_is_refused() {
        let k = TestKey::new();
        let enc = k.encrypt(b"BHD5");
        assert!(k.public.decrypt(&enc[..enc.len() - 1]).is_err());
        assert!(k.public.decrypt(&[]).is_err());
    }

    #[test]
    fn broken_der_is_an_error_and_not_a_panic() {
        for der in [
            &[][..],
            &[0x30],
            &[0x30, 0x84, 0xff, 0xff, 0xff, 0xff],
            &[0x30, 0x02, 0x02, 0x05],
        ] {
            assert!(ArchiveKey::from_der(der).is_err(), "{der:?}");
        }
    }
}

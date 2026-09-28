//! One-time codes (RFC 6238), and the `otpauth://` links that carry their
//! secrets.
//!
//! # HMAC by hand
//!
//! The `hmac` crate in this tree (0.13, through `zip`) is built on `digest`
//! 0.11, and the app's `sha2` is 0.10 — the two do not meet. Moving `sha2` for
//! the whole app to serve one feature is churn in code that is not this one.
//! HMAC itself is two hash calls and a padded key (RFC 2104), so it is written
//! out here and held to the published vectors of RFC 2202 (SHA-1) and RFC 4231
//! (SHA-256, SHA-512), and the TOTP on top of it to RFC 6238's own table.
//!
//! # Where the secret lives
//!
//! In the item, as a `SecretString`, like a password. The screen never
//! receives it: it receives the current code, which is worth thirty seconds.

use serde::{Deserialize, Serialize};
use sha1::Digest as _;

use super::item::SecretString;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Algorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    fn block_len(self) -> usize {
        match self {
            Algorithm::Sha1 | Algorithm::Sha256 => 64,
            Algorithm::Sha512 => 128,
        }
    }

    fn hash(self, parts: &[&[u8]]) -> Vec<u8> {
        match self {
            Algorithm::Sha1 => {
                let mut h = sha1::Sha1::new();
                parts.iter().for_each(|p| h.update(p));
                h.finalize().to_vec()
            }
            Algorithm::Sha256 => {
                use sha2::Digest;
                let mut h = sha2::Sha256::new();
                parts.iter().for_each(|p| h.update(p));
                h.finalize().to_vec()
            }
            Algorithm::Sha512 => {
                use sha2::Digest;
                let mut h = sha2::Sha512::new();
                parts.iter().for_each(|p| h.update(p));
                h.finalize().to_vec()
            }
        }
    }
}

/// HMAC (RFC 2104): `H((K ⊕ opad) ‖ H((K ⊕ ipad) ‖ message))`, with a key
/// longer than a block hashed first and a shorter one padded with zeros.
pub fn hmac(algorithm: Algorithm, key: &[u8], message: &[u8]) -> Vec<u8> {
    let block = algorithm.block_len();
    let mut k = zeroize::Zeroizing::new(if key.len() > block { algorithm.hash(&[key]) } else { key.to_vec() });
    k.resize(block, 0);
    let ipad: zeroize::Zeroizing<Vec<u8>> = zeroize::Zeroizing::new(k.iter().map(|b| b ^ 0x36).collect());
    let opad: zeroize::Zeroizing<Vec<u8>> = zeroize::Zeroizing::new(k.iter().map(|b| b ^ 0x5c).collect());
    let inner = algorithm.hash(&[&ipad, message]);
    algorithm.hash(&[&opad, &inner])
}

/// What an item keeps to make codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totp {
    /// Base32, upper case, no padding — the form people are given.
    pub secret: SecretString,
    #[serde(default)]
    pub algorithm: Algorithm,
    #[serde(default = "six")]
    pub digits: u8,
    #[serde(default = "thirty")]
    pub period: u32,
}

fn six() -> u8 {
    6
}

fn thirty() -> u32 {
    30
}

/// What the screen is told about an item's codes: never the secret.
#[derive(Debug, Clone, Serialize)]
pub struct TotpView {
    pub algorithm: Algorithm,
    pub digits: u8,
    pub period: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Code {
    pub code: String,
    /// Seconds until this code is replaced.
    pub remaining: u32,
    pub period: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TotpError {
    #[error("that is neither an otpauth:// link nor a base32 secret")]
    Unreadable,
    #[error("the link is for a counter-based code (HOTP), which Safe does not make")]
    Hotp,
}

impl Totp {
    pub fn view(&self) -> TotpView {
        TotpView { algorithm: self.algorithm, digits: self.digits, period: self.period }
    }

    /// The code for Unix time `now`.
    pub fn code_at(&self, now: u64) -> Code {
        let key = zeroize::Zeroizing::new(base32_decode(self.secret.expose()).unwrap_or_default());
        let counter = now / u64::from(self.period);
        let mac = hmac(self.algorithm, &key, &counter.to_be_bytes());
        // Dynamic truncation, RFC 4226 §5.3.
        let offset = (mac[mac.len() - 1] & 0x0f) as usize;
        let binary = u32::from_be_bytes([mac[offset] & 0x7f, mac[offset + 1], mac[offset + 2], mac[offset + 3]]);
        let modulus = 10u64.pow(u32::from(self.digits));
        let code = format!("{:0width$}", u64::from(binary) % modulus, width = usize::from(self.digits));
        Code { code, remaining: self.period - (now % u64::from(self.period)) as u32, period: self.period }
    }

    /// Read what a person pasted: an `otpauth://totp/…` link from a QR code,
    /// or the bare base32 secret a site shows under "can't scan it?".
    pub fn parse(input: &str) -> Result<Totp, TotpError> {
        let input = input.trim();
        if input.to_ascii_lowercase().starts_with("otpauth://") {
            return Self::parse_uri(input);
        }
        let secret = normalise_secret(input).ok_or(TotpError::Unreadable)?;
        Ok(Totp { secret: SecretString::new(secret), algorithm: Algorithm::Sha1, digits: 6, period: 30 })
    }

    fn parse_uri(input: &str) -> Result<Totp, TotpError> {
        let url = url::Url::parse(input).map_err(|_| TotpError::Unreadable)?;
        match url.host_str().map(str::to_ascii_lowercase).as_deref() {
            Some("totp") => {}
            Some("hotp") => return Err(TotpError::Hotp),
            _ => return Err(TotpError::Unreadable),
        }
        let mut totp = Totp { secret: SecretString::default(), algorithm: Algorithm::Sha1, digits: 6, period: 30 };
        for (key, value) in url.query_pairs() {
            match key.to_ascii_lowercase().as_str() {
                "secret" => totp.secret = SecretString::new(normalise_secret(&value).ok_or(TotpError::Unreadable)?),
                "algorithm" => {
                    totp.algorithm = match value.to_ascii_uppercase().as_str() {
                        "SHA1" => Algorithm::Sha1,
                        "SHA256" => Algorithm::Sha256,
                        "SHA512" => Algorithm::Sha512,
                        _ => return Err(TotpError::Unreadable),
                    }
                }
                "digits" => totp.digits = value.parse().ok().filter(|d| (6..=8).contains(d)).ok_or(TotpError::Unreadable)?,
                "period" => totp.period = value.parse().ok().filter(|p| (1..=300).contains(p)).ok_or(TotpError::Unreadable)?,
                _ => {}
            }
        }
        if totp.secret.is_empty() {
            return Err(TotpError::Unreadable);
        }
        Ok(totp)
    }
}

/// A base32 secret as people type it — spaces, dashes, lower case, padding —
/// in the canonical form, or `None` if it is not base32 at all.
fn normalise_secret(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect::<String>()
        .trim_end_matches('=')
        .to_string();
    let decoded = base32_decode(&cleaned)?;
    // Under 80 bits is not a secret anybody issues; it is a typo.
    (decoded.len() >= 10).then_some(cleaned)
}

/// RFC 4648 base32, upper case, no padding.
fn base32_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = Vec::with_capacity(input.len() * 5 / 8);
    let (mut buffer, mut bits) = (0u64, 0u32);
    for c in input.bytes() {
        let value = ALPHABET.iter().position(|&a| a == c)? as u64;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        hex::decode(s).unwrap()
    }

    /// RFC 2202, test cases 1, 2 and 6 (SHA-1).
    #[test]
    fn hmac_sha1_matches_rfc_2202() {
        assert_eq!(hmac(Algorithm::Sha1, &[0x0b; 20], b"Hi There"), hex("b617318655057264e28bc0b6fb378c8ef146be00"));
        assert_eq!(hmac(Algorithm::Sha1, b"Jefe", b"what do ya want for nothing?"), hex("effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"));
        assert_eq!(
            hmac(Algorithm::Sha1, &[0xaa; 80], b"Test Using Larger Than Block-Size Key - Hash Key First"),
            hex("aa4ae5e15272d00e95705637ce8a3b55ed402112")
        );
    }

    /// RFC 4231, test cases 1, 2 and 6 (SHA-256 and SHA-512) — case 6 is the
    /// key longer than a block.
    #[test]
    fn hmac_sha2_matches_rfc_4231() {
        assert_eq!(
            hmac(Algorithm::Sha256, &[0x0b; 20], b"Hi There"),
            hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
        );
        assert_eq!(
            hmac(Algorithm::Sha512, b"Jefe", b"what do ya want for nothing?"),
            hex("164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737")
        );
        assert_eq!(
            hmac(Algorithm::Sha256, &[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First"),
            hex("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54")
        );
    }

    /// RFC 6238, Appendix B: every row of the table, eight digits, all three
    /// algorithms with their own seeds.
    #[test]
    fn totp_matches_rfc_6238() {
        let seeds = [
            (Algorithm::Sha1, &b"12345678901234567890"[..]),
            (Algorithm::Sha256, &b"12345678901234567890123456789012"[..]),
            (Algorithm::Sha512, &b"1234567890123456789012345678901234567890123456789012345678901234"[..]),
        ];
        let table: [(u64, [&str; 3]); 6] = [
            (59, ["94287082", "46119246", "90693936"]),
            (1111111109, ["07081804", "68084774", "25091201"]),
            (1111111111, ["14050471", "67062674", "99943326"]),
            (1234567890, ["89005924", "91819424", "93441116"]),
            (2000000000, ["69279037", "90698825", "38618901"]),
            (20000000000, ["65353130", "77737706", "47863826"]),
        ];
        for (time, expected) in table {
            for (i, (algorithm, seed)) in seeds.iter().enumerate() {
                let totp = Totp { secret: SecretString::new(base32_encode(seed)), algorithm: *algorithm, digits: 8, period: 30 };
                assert_eq!(totp.code_at(time).code, expected[i], "{algorithm:?} at {time}");
            }
        }
    }

    fn base32_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let (mut out, mut buffer, mut bits) = (String::new(), 0u64, 0u32);
        for &b in bytes {
            buffer = (buffer << 8) | u64::from(b);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
            }
        }
        if bits > 0 {
            out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
        }
        out
    }

    #[test]
    fn remaining_counts_down_to_the_next_code() {
        let totp = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(totp.code_at(60).remaining, 30);
        assert_eq!(totp.code_at(89).remaining, 1);
        assert_eq!(totp.code_at(60).code.len(), 6);
    }

    #[test]
    fn links_and_bare_secrets_are_both_read() {
        let t = Totp::parse("otpauth://totp/GitHub:anh?secret=jbswy3dpehpk3pxp&issuer=GitHub&algorithm=SHA256&digits=8&period=60").unwrap();
        assert_eq!((t.algorithm, t.digits, t.period), (Algorithm::Sha256, 8, 60));
        assert_eq!(t.secret.expose(), "JBSWY3DPEHPK3PXP");

        let bare = Totp::parse("jbsw y3dp-ehpk 3pxp====").unwrap();
        assert_eq!(bare.secret.expose(), "JBSWY3DPEHPK3PXP");
        assert_eq!((bare.algorithm, bare.digits, bare.period), (Algorithm::Sha1, 6, 30));
    }

    #[test]
    fn nonsense_is_refused() {
        assert_eq!(Totp::parse("hello world!").unwrap_err(), TotpError::Unreadable);
        assert_eq!(Totp::parse("ABC").unwrap_err(), TotpError::Unreadable, "too short to be a real secret");
        assert_eq!(Totp::parse("otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP&counter=1").unwrap_err(), TotpError::Hotp);
        assert_eq!(Totp::parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=12").unwrap_err(), TotpError::Unreadable);
        assert_eq!(Totp::parse("otpauth://totp/x?issuer=nothing").unwrap_err(), TotpError::Unreadable);
    }
}

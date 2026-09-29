//! What Safe's SSH agent speaks: OpenSSH private keys, and the agent protocol.
//!
//! Pure — no socket here, no Safe. `ssh_agent` puts a socket in front of
//! [`handle`]; the keys come from the open Safe and never leave the process.
//!
//! # What is supported
//!
//! Ed25519, the key `ssh-keygen` has made by default for years. An RSA or
//! ECDSA key is skipped and said so; a passphrase-protected one is refused with
//! what to do — the Safe already encrypts it, and a second passphrase would
//! have to be typed where the agent cannot ask for it.
//!
//! The protocol is draft-miller-ssh-agent: list identities, sign. Adding,
//! removing and locking keys through the socket are refused — the Safe is where
//! keys are added, and its own lock is the lock.

use serde::Serialize;

pub const MAGIC: &[u8] = b"openssh-key-v1\0";

const FAILURE: u8 = 5;
const REQUEST_IDENTITIES: u8 = 11;
const IDENTITIES_ANSWER: u8 = 12;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;

/// The largest message the agent reads. A sign request carries a session
/// hash and a username; a megabyte is far beyond any real one.
pub const MAX_MESSAGE: usize = 1 << 20;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("not an OpenSSH private key")]
    NotOpenSsh,
    #[error("this key has a passphrase; remove it with `ssh-keygen -p` before keeping it in Safe, which already encrypts it")]
    Encrypted,
    #[error("only Ed25519 keys are supported by Safe's agent so far (this is {0})")]
    Unsupported(String),
    #[error("the key is damaged")]
    Damaged,
}

/// An Ed25519 key, as the agent needs it. The seed is wiped when dropped.
pub struct Ed25519Key {
    pub public: [u8; 32],
    seed: zeroize::Zeroizing<[u8; 32]>,
    pub comment: String,
}

impl Ed25519Key {
    /// The public key as the SSH wire format writes it.
    pub fn blob(&self) -> Vec<u8> {
        let mut b = Vec::new();
        put_string(&mut b, b"ssh-ed25519");
        put_string(&mut b, &self.public);
        b
    }

    /// `ssh-ed25519 AAAA… comment`, the line in `authorized_keys`.
    pub fn authorized_line(&self) -> String {
        use base64::Engine;
        format!("ssh-ed25519 {} {}", base64::engine::general_purpose::STANDARD.encode(self.blob()), self.comment).trim_end().to_string()
    }

    /// SHA256:… as `ssh-keygen -l` prints it.
    pub fn fingerprint(&self) -> String {
        use base64::Engine;
        use sha2::Digest;
        let digest = sha2::Sha256::digest(self.blob());
        format!("SHA256:{}", base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest))
    }

    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer;
        let key = ed25519_dalek::SigningKey::from_bytes(&self.seed);
        let signature = key.sign(data).to_bytes();
        let mut b = Vec::new();
        put_string(&mut b, b"ssh-ed25519");
        put_string(&mut b, &signature);
        b
    }
}

// ─── the wire ────────────────────────────────────────────

fn put_u32(b: &mut Vec<u8>, n: u32) {
    b.extend_from_slice(&n.to_be_bytes());
}

fn put_string(b: &mut Vec<u8>, s: &[u8]) {
    put_u32(b, s.len() as u32);
    b.extend_from_slice(s);
}

/// Reads SSH wire values from a buffer, refusing to run past its end.
struct Reader<'a> {
    b: &'a [u8],
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.b.split_first_chunk::<4>()?;
        self.b = rest;
        Some(u32::from_be_bytes(*head))
    }

    fn u8(&mut self) -> Option<u8> {
        let (head, rest) = self.b.split_first()?;
        self.b = rest;
        Some(*head)
    }

    fn string(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        if n > self.b.len() {
            return None;
        }
        let (s, rest) = self.b.split_at(n);
        self.b = rest;
        Some(s)
    }
}

// ─── private keys ────────────────────────────────────────

/// Whether `text` holds an OpenSSH private key at all — what the agent looks
/// for in an item's concealed fields.
pub fn looks_like_key(text: &str) -> bool {
    text.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
}

/// Read an OpenSSH private key (`-----BEGIN OPENSSH PRIVATE KEY-----`).
pub fn parse(pem: &str) -> Result<Ed25519Key, KeyError> {
    use base64::Engine;
    const BEGIN: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
    const END: &str = "-----END OPENSSH PRIVATE KEY-----";
    // Between the markers, whatever the line breaks: a key pasted into a
    // one-line field arrives with them removed, and one copied from a web page
    // may have spaces or `\r` in their place.
    let start = pem.find(BEGIN).ok_or(KeyError::NotOpenSsh)? + BEGIN.len();
    let end = pem[start..].find(END).ok_or(KeyError::NotOpenSsh)? + start;
    let body = zeroize::Zeroizing::new(pem[start..end].chars().filter(|c| !c.is_whitespace()).collect::<String>());
    let raw = zeroize::Zeroizing::new(
        base64::engine::general_purpose::STANDARD.decode(body.as_bytes()).map_err(|_| KeyError::NotOpenSsh)?,
    );
    let rest = raw.strip_prefix(MAGIC).ok_or(KeyError::NotOpenSsh)?;
    let mut r = Reader { b: rest };
    let cipher = r.string().ok_or(KeyError::Damaged)?;
    let _kdf = r.string().ok_or(KeyError::Damaged)?;
    let _kdf_options = r.string().ok_or(KeyError::Damaged)?;
    if cipher != b"none" {
        return Err(KeyError::Encrypted);
    }
    if r.u32() != Some(1) {
        return Err(KeyError::Damaged);
    }
    let _public_blob = r.string().ok_or(KeyError::Damaged)?;
    let private = r.string().ok_or(KeyError::Damaged)?;

    let mut p = Reader { b: private };
    let (c1, c2) = (p.u32().ok_or(KeyError::Damaged)?, p.u32().ok_or(KeyError::Damaged)?);
    if c1 != c2 {
        // The check integers differ only when the section was decrypted with
        // the wrong key — impossible for "none" unless the file is damaged.
        return Err(KeyError::Damaged);
    }
    let kind = p.string().ok_or(KeyError::Damaged)?;
    if kind != b"ssh-ed25519" {
        return Err(KeyError::Unsupported(String::from_utf8_lossy(kind).into_owned()));
    }
    let public: [u8; 32] = p.string().and_then(|s| s.try_into().ok()).ok_or(KeyError::Damaged)?;
    let both = p.string().filter(|s| s.len() == 64).ok_or(KeyError::Damaged)?;
    let seed: [u8; 32] = both[..32].try_into().expect("32 of 64");
    if both[32..] != public {
        return Err(KeyError::Damaged);
    }
    let comment = p.string().map(|c| String::from_utf8_lossy(c).into_owned()).unwrap_or_default();

    // The public half must be the seed's: a key whose halves disagree would
    // be offered under one identity and sign as another.
    let derived = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    if derived != public {
        return Err(KeyError::Damaged);
    }
    Ok(Ed25519Key { public, seed: zeroize::Zeroizing::new(seed), comment })
}

// ─── the protocol ────────────────────────────────────────

/// What a sign request is for, as far as it can be read: the user the
/// signature logs in as, when it is an SSH login. Shown on the approval.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SignRequest {
    pub key: String,
    pub fingerprint: String,
    /// The user in an SSH `publickey` login, when the data is one.
    pub login_as: Option<String>,
}

/// What [`handle`] needs from the agent around it.
pub trait Agent {
    /// The keys the open Safe holds, with the item title each came from.
    fn keys(&self) -> Vec<(String, Ed25519Key)>;
    /// Whether this signature may be made. Asking the user happens here.
    fn allow(&self, request: &SignRequest) -> bool;
}

fn failure() -> Vec<u8> {
    vec![FAILURE]
}

/// The user of an SSH userauth `publickey` request (RFC 4252 §7), if `data`
/// is one: session id, 50, user, service, "publickey", …
fn login_user(data: &[u8]) -> Option<String> {
    let mut r = Reader { b: data };
    r.string()?;
    if r.u8()? != 50 {
        return None;
    }
    let user = r.string()?;
    let _service = r.string()?;
    (r.string()? == b"publickey").then(|| String::from_utf8_lossy(user).into_owned())
}

/// Answer one agent message (its body, without the length prefix).
pub fn handle(message: &[u8], agent: &dyn Agent) -> Vec<u8> {
    let mut r = Reader { b: message };
    match r.u8() {
        Some(REQUEST_IDENTITIES) => {
            let keys = agent.keys();
            let mut out = vec![IDENTITIES_ANSWER];
            put_u32(&mut out, keys.len() as u32);
            for (title, key) in &keys {
                put_string(&mut out, &key.blob());
                put_string(&mut out, title.as_bytes());
            }
            out
        }
        Some(SIGN_REQUEST) => {
            let (Some(blob), Some(data)) = (r.string(), r.string()) else { return failure() };
            let keys = agent.keys();
            let Some((title, key)) = keys.iter().find(|(_, k)| k.blob() == blob) else { return failure() };
            let request = SignRequest { key: title.clone(), fingerprint: key.fingerprint(), login_as: login_user(data) };
            if !agent.allow(&request) {
                return failure();
            }
            let mut out = vec![SIGN_RESPONSE];
            put_string(&mut out, &key.sign(data));
            out
        }
        _ => failure(),
    }
}

/// Frame a reply: its length, then it.
pub fn frame(reply: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(reply.len() + 4);
    put_u32(&mut out, reply.len() as u32);
    out.extend_from_slice(reply);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made with `ssh-keygen -t ed25519 -N "" -C test@synabit`.
    const KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACA0wxhEVEUn1FV2yOPksuhXGAuiEvQpiVXGrL1+IsrGJQAAAJDuAwU+7gMF
PgAAAAtzc2gtZWQyNTUxOQAAACA0wxhEVEUn1FV2yOPksuhXGAuiEvQpiVXGrL1+IsrGJQ
AAAEAmPc8onOl4uUnq0JFiRDdfZua6PUHPGvbwj0R6KxHs4jTDGERURSfUVXbI4+Sy6FcY
C6IS9CmJVcasvX4iysYlAAAADHRlc3RAc3luYWJpdAE=
-----END OPENSSH PRIVATE KEY-----
";

    struct Fixed(bool);
    impl Agent for Fixed {
        fn keys(&self) -> Vec<(String, Ed25519Key)> {
            vec![("GitHub deploy".into(), parse(KEY).unwrap())]
        }
        fn allow(&self, _: &SignRequest) -> bool {
            self.0
        }
    }

    #[test]
    fn an_ed25519_key_is_read() {
        let key = parse(KEY).unwrap();
        assert_eq!(key.comment, "test@synabit");
        assert!(key.authorized_line().starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA"));
    }

    /// What a one-line password field does to a pasted key: the line breaks
    /// are gone. And what a web page or Windows may do: `\r\n`, or spaces.
    #[test]
    fn a_key_whose_line_breaks_were_lost_or_changed_is_still_read() {
        let expected = parse(KEY).unwrap().authorized_line();
        for mangled in [KEY.replace('\n', ""), KEY.replace('\n', "\r\n"), KEY.replace('\n', " ")] {
            assert_eq!(parse(&mangled).map(|k| k.authorized_line()).ok().as_deref(), Some(expected.as_str()), "{mangled:?}");
        }
    }

    #[test]
    fn what_is_not_a_usable_key_says_why() {
        assert_eq!(parse("hello").err(), Some(KeyError::NotOpenSsh));
        // One character of the private section changed.
        let lines: Vec<&str> = KEY.lines().collect();
        let mut body = lines[3].to_string();
        let c = body.remove(10);
        body.insert(10, if c == 'A' { 'B' } else { 'A' });
        let damaged = [lines[..3].join("\n"), body, lines[4..].join("\n")].join("\n");
        assert!(parse(&damaged).is_err(), "a changed private section was accepted");
    }

    #[test]
    fn identities_are_listed_and_a_signature_verifies() {
        let answer = handle(&[REQUEST_IDENTITIES], &Fixed(true));
        assert_eq!(answer[0], IDENTITIES_ANSWER);
        let mut r = Reader { b: &answer[1..] };
        assert_eq!(r.u32(), Some(1));
        let blob = r.string().unwrap().to_vec();
        assert_eq!(r.string(), Some(&b"GitHub deploy"[..]));

        let mut req = vec![SIGN_REQUEST];
        put_string(&mut req, &blob);
        put_string(&mut req, b"data to sign");
        put_u32(&mut req, 0);
        let signed = handle(&req, &Fixed(true));
        assert_eq!(signed[0], SIGN_RESPONSE);
        let mut r = Reader { b: &signed[1..] };
        let sig_blob = r.string().unwrap();
        let mut s = Reader { b: sig_blob };
        assert_eq!(s.string(), Some(&b"ssh-ed25519"[..]));
        let sig: [u8; 64] = s.string().unwrap().try_into().unwrap();
        use ed25519_dalek::Verifier;
        let public = ed25519_dalek::VerifyingKey::from_bytes(&parse(KEY).unwrap().public).unwrap();
        assert!(public.verify(b"data to sign", &ed25519_dalek::Signature::from_bytes(&sig)).is_ok());
    }

    #[test]
    fn a_refused_signature_and_anything_else_is_a_failure() {
        let blob = parse(KEY).unwrap().blob();
        let mut req = vec![SIGN_REQUEST];
        put_string(&mut req, &blob);
        put_string(&mut req, b"x");
        assert_eq!(handle(&req, &Fixed(false)), vec![FAILURE]);
        // Adding a key through the socket, and junk.
        assert_eq!(handle(&[17, 0, 0], &Fixed(true)), vec![FAILURE]);
        assert_eq!(handle(&[], &Fixed(true)), vec![FAILURE]);
        assert_eq!(handle(&[SIGN_REQUEST, 0, 0, 0, 200], &Fixed(true)), vec![FAILURE]);
    }

    #[test]
    fn an_ssh_login_names_its_user() {
        let mut data = Vec::new();
        put_string(&mut data, b"session-id");
        data.push(50);
        put_string(&mut data, b"git");
        put_string(&mut data, b"ssh-connection");
        put_string(&mut data, b"publickey");
        assert_eq!(login_user(&data).as_deref(), Some("git"));
        assert_eq!(login_user(b"not a login"), None);
    }
}

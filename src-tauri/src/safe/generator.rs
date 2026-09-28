//! New passwords and passphrases, and a rough idea of how strong one is.
//!
//! Every choice is uniform. A character is drawn by rejection sampling rather
//! than `byte % len`, which would make the first `256 % len` characters of the
//! alphabet more likely than the rest; a password with "at least one of each"
//! is drawn again until it has one, rather than having a character forced into
//! a position, which would make that position predictable.
//!
//! Passphrases use the BIP39 English list already in the binary: 2,048 words,
//! 11 bits each, every word identifiable by its first four letters. The EFF
//! list the design names carries 12.9 bits a word, but it is 60 KB of new data
//! and a new licence, for what one extra word buys back.

use serde::{Deserialize, Serialize};

use super::crypto::{random_bytes, CryptoError};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?/~";
/// Characters people misread when typing a password off a screen.
const AMBIGUOUS: &str = "Il1O0o|`'\"";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Recipe {
    Random {
        length: usize,
        #[serde(default = "yes")]
        lower: bool,
        #[serde(default = "yes")]
        upper: bool,
        #[serde(default = "yes")]
        digits: bool,
        #[serde(default = "yes")]
        symbols: bool,
        #[serde(default)]
        avoid_ambiguous: bool,
    },
    Passphrase {
        words: usize,
        #[serde(default = "dash")]
        separator: String,
        #[serde(default)]
        capitalise: bool,
        /// One digit appended to one random word, for sites that insist.
        #[serde(default)]
        digit: bool,
    },
    Pin {
        length: usize,
    },
}

fn yes() -> bool {
    true
}

fn dash() -> String {
    "-".into()
}

impl Default for Recipe {
    fn default() -> Self {
        Recipe::Random { length: 20, lower: true, upper: true, digits: true, symbols: true, avoid_ambiguous: false }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GenerateError {
    #[error("choose between {min} and {max}")]
    OutOfRange { min: usize, max: usize },
    #[error("choose at least one kind of character")]
    NoAlphabet,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

/// A uniformly random index below `n`.
fn below(n: usize) -> Result<usize, CryptoError> {
    assert!(n > 0 && n <= u32::MAX as usize);
    let n = n as u64;
    // The largest multiple of n that fits in a u32; draws at or above it are
    // thrown away so that every residue is equally likely.
    let zone = (u64::from(u32::MAX) + 1) / n * n;
    loop {
        let draw = u64::from(u32::from_le_bytes(random_bytes()?));
        if draw < zone {
            return Ok((draw % n) as usize);
        }
    }
}

pub fn generate(recipe: &Recipe) -> Result<String, GenerateError> {
    match recipe {
        Recipe::Random { length, lower, upper, digits, symbols, avoid_ambiguous } => {
            if !(8..=128).contains(length) {
                return Err(GenerateError::OutOfRange { min: 8, max: 128 });
            }
            let classes: Vec<Vec<char>> = [(*lower, LOWER), (*upper, UPPER), (*digits, DIGITS), (*symbols, SYMBOLS)]
                .into_iter()
                .filter(|(on, _)| *on)
                .map(|(_, set)| set.chars().filter(|c| !(*avoid_ambiguous && AMBIGUOUS.contains(*c))).collect())
                .collect();
            if classes.is_empty() {
                return Err(GenerateError::NoAlphabet);
            }
            let alphabet: Vec<char> = classes.concat();
            loop {
                let candidate: Vec<char> =
                    (0..*length).map(|_| below(alphabet.len()).map(|i| alphabet[i])).collect::<Result<_, _>>()?;
                if classes.iter().all(|class| candidate.iter().any(|c| class.contains(c))) {
                    return Ok(candidate.into_iter().collect());
                }
            }
        }
        Recipe::Passphrase { words, separator, capitalise, digit } => {
            if !(3..=12).contains(words) {
                return Err(GenerateError::OutOfRange { min: 3, max: 12 });
            }
            let list = bip39::Language::English.word_list();
            let mut chosen: Vec<String> = (0..*words)
                .map(|_| {
                    below(list.len()).map(|i| {
                        let w = list[i];
                        if *capitalise {
                            let mut c = w.chars();
                            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
                        } else {
                            w.to_string()
                        }
                    })
                })
                .collect::<Result<_, _>>()?;
            if *digit {
                let at = below(chosen.len())?;
                let d = below(10)?;
                chosen[at].push_str(&d.to_string());
            }
            Ok(chosen.join(separator))
        }
        Recipe::Pin { length } => {
            if !(4..=12).contains(length) {
                return Err(GenerateError::OutOfRange { min: 4, max: 12 });
            }
            (0..*length).map(|_| below(10).map(|d| char::from(b'0' + d as u8))).collect::<Result<_, _>>().map_err(Into::into)
        }
    }
}

/// The entropy a recipe puts in what it generates, in bits. Exact for what
/// this module makes, which is why the generator shows it rather than
/// [`estimate_bits`].
pub fn recipe_bits(recipe: &Recipe) -> f64 {
    match recipe {
        Recipe::Random { length, lower, upper, digits, symbols, avoid_ambiguous } => {
            let size: usize = [(*lower, LOWER), (*upper, UPPER), (*digits, DIGITS), (*symbols, SYMBOLS)]
                .into_iter()
                .filter(|(on, _)| *on)
                .map(|(_, set)| set.chars().filter(|c| !(*avoid_ambiguous && AMBIGUOUS.contains(*c))).count())
                .sum();
            if size == 0 {
                0.0
            } else {
                *length as f64 * (size as f64).log2()
            }
        }
        Recipe::Passphrase { words, digit, .. } => {
            *words as f64 * 11.0 + if *digit { (10.0 * *words as f64).log2() } else { 0.0 }
        }
        Recipe::Pin { length } => *length as f64 * 10f64.log2(),
    }
}

/// A deliberately pessimistic guess at the strength of a password a person
/// chose.
///
/// Not zxcvbn — that is P4, behind a size review. This one only knows the
/// commonest ways people fool themselves: a repeated or sequential run counts
/// as one character, and the character pool is only what the password
/// actually uses. It will call `Tr0ub4dor&3` stronger than it is. It will not
/// call `aaaaaaaaaaaa` or `123456789012` strong, which is the mistake that
/// matters for a master password.
pub fn estimate_bits(password: &str) -> f64 {
    let chars: Vec<char> = password.chars().collect();
    if chars.is_empty() {
        return 0.0;
    }
    let mut pool = 0usize;
    if chars.iter().any(|c| c.is_ascii_lowercase()) {
        pool += 26;
    }
    if chars.iter().any(|c| c.is_ascii_uppercase()) {
        pool += 26;
    }
    if chars.iter().any(|c| c.is_ascii_digit()) {
        pool += 10;
    }
    if chars.iter().any(|c| c.is_ascii_punctuation() || *c == ' ') {
        pool += 33;
    }
    if chars.iter().any(|c| !c.is_ascii()) {
        pool += 100;
    }

    // Count characters that are not simply the previous one repeated, or one
    // step along from it.
    let mut effective = 1usize;
    for pair in chars.windows(2) {
        let step = pair[1] as i64 - pair[0] as i64;
        if !(-1..=1).contains(&step) {
            effective += 1;
        }
    }
    effective as f64 * (pool as f64).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_passwords_have_the_length_and_every_class_asked_for() {
        for _ in 0..200 {
            let p = generate(&Recipe::default()).unwrap();
            assert_eq!(p.chars().count(), 20);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
        }
    }

    #[test]
    fn ambiguous_characters_can_be_left_out() {
        let recipe =
            Recipe::Random { length: 128, lower: true, upper: true, digits: true, symbols: true, avoid_ambiguous: true };
        for _ in 0..50 {
            assert!(!generate(&recipe).unwrap().chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn nonsense_recipes_are_refused() {
        let none = Recipe::Random { length: 20, lower: false, upper: false, digits: false, symbols: false, avoid_ambiguous: false };
        assert_eq!(generate(&none), Err(GenerateError::NoAlphabet));
        assert!(generate(&Recipe::Random { length: 4, lower: true, upper: true, digits: true, symbols: true, avoid_ambiguous: false }).is_err());
        assert!(generate(&Recipe::Passphrase { words: 2, separator: "-".into(), capitalise: false, digit: false }).is_err());
    }

    #[test]
    fn passphrases_are_words_from_the_list() {
        let list = bip39::Language::English.word_list();
        let p = generate(&Recipe::Passphrase { words: 6, separator: " ".into(), capitalise: false, digit: false }).unwrap();
        let words: Vec<&str> = p.split(' ').collect();
        assert_eq!(words.len(), 6);
        assert!(words.iter().all(|w| list.contains(w)));
        assert!((recipe_bits(&Recipe::Passphrase { words: 6, separator: " ".into(), capitalise: false, digit: false }) - 66.0).abs() < 1e-9);
    }

    #[test]
    fn a_pin_is_digits() {
        let p = generate(&Recipe::Pin { length: 6 }).unwrap();
        assert_eq!(p.len(), 6);
        assert!(p.chars().all(|c| c.is_ascii_digit()));
    }

    /// Every character of the alphabet turns up about as often as every other.
    /// Loose bounds: this is a tripwire for `% len`-style bias, not a
    /// statistical test suite.
    #[test]
    fn draws_are_uniform() {
        let mut counts = [0usize; 7];
        for _ in 0..70_000 {
            counts[below(7).unwrap()] += 1;
        }
        for c in counts {
            assert!((9_000..11_000).contains(&c), "{counts:?}");
        }
    }

    #[test]
    fn the_estimate_is_not_fooled_by_runs() {
        assert!(estimate_bits("aaaaaaaaaaaa") < 10.0);
        assert!(estimate_bits("123456789012") < 20.0);
        assert!(estimate_bits("abcdefghijkl") < 10.0);
        assert!(estimate_bits("correct horse battery staple") > 100.0);
        assert!(estimate_bits(&generate(&Recipe::default()).unwrap()) > 100.0);
    }
}

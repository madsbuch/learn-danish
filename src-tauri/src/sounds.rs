//! Sound pairs: Danish sounds an English ear runs together — æ and e, ø and
//! y, y and i, å and o, u and o, a stød and none, a short vowel and a long
//! one, a soft d and none, r and h — heard in pairs of real words that
//! differ only there ("træ" / "tre"). The app says one; the learner picks
//! which it was; then the other is played, so the two are heard side by
//! side.
//!
//! Pairs come from two places:
//! - a word's own near neighbour, which the tutor gives when it makes the
//!   word (`words.near_da`, `near_en`: schema 10) — the lesson's own sounds;
//! - the built-in list below, the core contrasts, which ships with the app
//!   like the demo course. Its pairs have ids derived from their words
//!   (DATA.md), the same on every device; nothing is written for them.
//!
//! A words lesson ends with a short round of them when its words have the
//! sounds (see [`round`]); the quick game "Sound pairs" plays the built-in
//! list. Older words have no neighbour and are not filled in: their lessons
//! use the built-in pairs.

use rand::seq::{IndexedRandom, SliceRandom};
use serde::Serialize;

use crate::models::{Exercise, Pair};
use crate::session::WordRow;
use crate::singalong::bare;
#[allow(unused_imports)]
use crate::ids::*;

/// Sound pairs in a words lesson's round: a short one.
pub const ROUND: usize = 2;

/// One contrast: what to listen for, the letters that make it, and real word
/// pairs that differ only there.
struct Contrast {
    /// What the card says to listen for.
    label: &'static str,
    /// A word with any of these has the sound.
    letters: &'static [&'static str],
    /// (Danish, English) and (Danish, English): the pair.
    pairs: &'static [(&'static str, &'static str, &'static str, &'static str)],
}

/// Spelled with a double consonant: the vowel before it is short.
const DOUBLED: &[&str] = &["ss", "dd", "ll", "tt", "nn", "mm", "kk", "pp"];

// Order matters: a pair is named by the first contrast whose letters one
// word has and the other hasn't (the stød's "nd" before the soft d's "d").
const CONTRASTS: &[Contrast] = &[
    Contrast {
        label: "æ or e",
        letters: &["æ"],
        pairs: &[("træ", "tree", "tre", "three"), ("læ", "shelter", "le", "to laugh"), ("hæl", "heel", "hel", "whole")],
    },
    Contrast {
        label: "ø or y",
        letters: &["ø"],
        pairs: &[("lød", "sounded", "lyd", "sound"), ("bøde", "fine (to pay)", "byde", "to offer")],
    },
    Contrast {
        label: "y or i",
        letters: &["y"],
        pairs: &[("ny", "new", "ni", "nine"), ("by", "town", "bi", "bee")],
    },
    Contrast {
        label: "å or o",
        letters: &["å"],
        pairs: &[("tå", "toe", "to", "two"), ("rå", "raw", "ro", "calm")],
    },
    Contrast {
        label: "u or o",
        letters: &["u"],
        pairs: &[("bud", "offer", "bod", "stall"), ("rude", "window pane", "rode", "to rummage")],
    },
    Contrast {
        label: "stød or none",
        letters: &["nd"],
        pairs: &[("hund", "dog", "hun", "she"), ("mand", "man", "man", "one, you (people)")],
    },
    Contrast {
        label: "short or long vowel",
        letters: DOUBLED,
        pairs: &[("visse", "certain (ones)", "vise", "to show"), ("hedde", "to be called", "hede", "heath")],
    },
    Contrast {
        label: "soft d or none",
        letters: &["d"],
        pairs: &[("sød", "sweet", "sø", "lake"), ("lide", "to like", "lige", "straight, equal")],
    },
    Contrast {
        label: "r or h",
        letters: &["r"],
        pairs: &[("rat", "steering wheel", "hat", "hat"), ("rose", "rose", "hose", "stockings")],
    },
];

/// A pair of words to tell apart by ear.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoundPair {
    /// A built-in pair's derived id; None for a word's own neighbour.
    pub id: Option<SoundPairId>,
    /// What to listen for: "æ or e".
    pub label: String,
    pub a: String,
    pub a_en: String,
    pub b: String,
    pub b_en: String,
}

/// A built-in pair's id: from its words, so it is the same everywhere.
fn pair_id(a: &str, b: &str) -> SoundPairId {
    SoundPairId::derived("sound-pair", &format!("{a}/{b}"))
}

/// The built-in pairs, every one.
pub fn built_in() -> Vec<SoundPair> {
    CONTRASTS
        .iter()
        .flat_map(|c| {
            c.pairs.iter().map(|(a, a_en, b, b_en)| SoundPair {
                id: Some(pair_id(a, b)),
                label: c.label.into(),
                a: (*a).into(),
                a_en: (*a_en).into(),
                b: (*b).into(),
                b_en: (*b_en).into(),
            })
        })
        .collect()
}

/// What to listen for between two words: the first contrast whose letters
/// one word has and the other hasn't.
pub fn label_for(a: &str, b: &str) -> String {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    CONTRASTS
        .iter()
        .find(|c| c.letters.iter().any(|l| a.contains(l) != b.contains(l)))
        .map_or_else(|| "Listen closely".into(), |c| c.label.into())
}

/// The built-in contrasts a lesson's words have: one pair of each, in a
/// random order.
fn heard_in(words: &[&WordRow], rng: &mut impl rand::Rng) -> Vec<SoundPair> {
    let text: Vec<String> = words.iter().map(|w| bare(&w.da).to_lowercase()).collect();
    let mut found: Vec<SoundPair> = Vec::new();
    for contrast in CONTRASTS {
        if !contrast.letters.iter().any(|l| text.iter().any(|t| t.contains(l))) {
            continue;
        }
        let pairs: Vec<SoundPair> = built_in().into_iter().filter(|p| p.label == contrast.label).collect();
        if let Some(pair) = pairs.choose(rng) {
            found.push(pair.clone());
        }
    }
    found.shuffle(rng);
    found
}

/// The pairs for a words lesson: its words' own neighbours first, then the
/// built-in pairs for the sounds its words have; at most [`ROUND`].
pub fn for_words(words: &[&WordRow], rng: &mut impl rand::Rng) -> Vec<SoundPair> {
    let mut own: Vec<SoundPair> = words
        .iter()
        .filter_map(|w| {
            let near = w.near.as_ref()?;
            let (a, b) = (bare(&w.da), bare(&near.da));
            Some(SoundPair { id: None, label: label_for(a, b), a: a.into(), a_en: w.meaning.clone(), b: b.into(), b_en: near.en.clone() })
        })
        .collect();
    own.shuffle(rng);
    let mut pairs = own;
    for pair in heard_in(words, rng) {
        if !pairs.iter().any(|p| p.label == pair.label || p.a == pair.a) {
            pairs.push(pair);
        }
    }
    pairs.truncate(ROUND);
    pairs
}

/// One card: the app says one of the pair; which was it? The two words are
/// the options, in the pair's order (so the card reads the same each time);
/// `pairs` carries both with their meanings, for after.
pub fn exercise(pair: &SoundPair, lesson_id: LessonId, rng: &mut impl rand::Rng) -> Exercise {
    let (said, said_en) = if rng.random_bool(0.5) { (&pair.a, &pair.a_en) } else { (&pair.b, &pair.b_en) };
    Exercise {
        key: format!("p{lesson_id}-{}-{}-sound", pair.a, pair.b),
        kind: "soundPair".into(),
        item_kind: "set".into(),
        item_id: lesson_id.to_string(),
        is_new: false,
        prompt: pair.label.clone(),
        hint: String::new(),
        answer: said.clone(),
        options: vec![pair.a.clone(), pair.b.clone()],
        tokens: vec![],
        da: said.clone(),
        meaning: said_en.clone(),
        gaps: vec![],
        pairs: vec![
            Pair { da: pair.a.clone(), en: pair.a_en.clone(), image: None },
            Pair { da: pair.b.clone(), en: pair.b_en.clone(), image: None },
        ],
        image: None,
        option_images: vec![],
        context: vec![],
        endings: vec![],
        scene: None,
        story: None,
        reply: None,
        explanation: String::new(),
    }
}

/// A words lesson's round of sound pairs.
pub fn round(words: &[&WordRow], lesson_id: LessonId, rng: &mut impl rand::Rng) -> Vec<Exercise> {
    for_words(words, rng).iter().map(|pair| exercise(pair, lesson_id, rng)).collect()
}

/// What to do to make a sound: its spellings, and a word of advice for when
/// it was wanted but something else was heard.
struct Sound {
    spellings: &'static [&'static str],
    advice: &'static str,
}

/// Two sounds a learner's mouth runs together, as a speaking card hears
/// them: the same contrasts as the pairs above (and their labels, so a tip
/// can play the pair), plus a few that only matter when speaking.
struct Confusion {
    label: &'static str,
    a: Sound,
    b: Sound,
}

const OPEN_AE: &str = "open the mouth a little more for æ, like the e in “bed”, held long";
const CLOSE_E: &str = "close the mouth a little more for e: a pure “ay” with no glide, close to the ee in “see”";
const ROUND_OE: &str = "round the lips for ø: say a pure e, with no glide, and push the lips forward";
const ROUND_Y: &str = "for y, say the ee in “see” with the lips pushed forward and rounded tight";
const SPREAD_I: &str = "keep the lips spread for i, like the ee in “see”";
const OPEN_AA: &str = "open a little for å, like the aw in “law”, with no glide";
const CLOSE_O: &str = "close and round the lips for o: a pure o, between “go” and “too”, with no glide";
const TIGHT_U: &str = "round the lips tight for u, like the oo in “food”, pushed forward";
const SHORT_VOWEL: &str = "keep the vowel short and clipped: a double consonant after it says so";
const LONG_VOWEL: &str = "hold the vowel long: a single consonant after it lets it stretch";

/// The short vowel before each doubled consonant, and the long one before
/// the single: one confusion per consonant, so "visse" heard as "vise" is
/// named, but not as "vide".
macro_rules! length {
    ($double:literal, $single:literal) => {
        Confusion {
            label: "short or long vowel",
            a: Sound { spellings: &[$double], advice: SHORT_VOWEL },
            b: Sound { spellings: &[$single], advice: LONG_VOWEL },
        }
    };
}

const CONFUSIONS: &[Confusion] = &[
    Confusion { label: "æ or e", a: Sound { spellings: &["æ"], advice: OPEN_AE }, b: Sound { spellings: &["e"], advice: CLOSE_E } },
    Confusion { label: "ø or y", a: Sound { spellings: &["ø"], advice: ROUND_OE }, b: Sound { spellings: &["y"], advice: ROUND_Y } },
    Confusion { label: "y or i", a: Sound { spellings: &["y"], advice: ROUND_Y }, b: Sound { spellings: &["i"], advice: SPREAD_I } },
    Confusion { label: "ø or e", a: Sound { spellings: &["ø"], advice: ROUND_OE }, b: Sound { spellings: &["e"], advice: CLOSE_E } },
    Confusion { label: "å or o", a: Sound { spellings: &["å"], advice: OPEN_AA }, b: Sound { spellings: &["o"], advice: CLOSE_O } },
    Confusion { label: "u or o", a: Sound { spellings: &["u"], advice: TIGHT_U }, b: Sound { spellings: &["o"], advice: CLOSE_O } },
    Confusion {
        label: "stød or none",
        a: Sound { spellings: &["nd"], advice: "add the stød: a little catch, a creak in the throat, as the n ends" },
        b: Sound { spellings: &["n"], advice: "say it smoothly, with no catch in the throat" },
    },
    length!("ss", "s"),
    length!("dd", "d"),
    length!("ll", "l"),
    length!("tt", "t"),
    length!("nn", "n"),
    length!("mm", "m"),
    length!("kk", "k"),
    length!("pp", "p"),
    Confusion {
        label: "soft d or none",
        a: Sound { spellings: &["d"], advice: "say the soft d: the tongue tip loose behind the lower teeth, close to the th in “the”" },
        b: Sound { spellings: &["g", ""], advice: "no soft d here: let the vowel run on to the end" },
    },
    Confusion {
        label: "r or h",
        a: Sound { spellings: &["r"], advice: "make the r far back in the throat, a soft gargle: never rolled, never a plain h" },
        b: Sound { spellings: &["h"], advice: "try a plain breathy h, as in “hat”" },
    },
];

/// A word said that was heard as another, and the sound that likely did it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoundTip {
    /// The word as it should be: "træ".
    pub word: String,
    /// What was heard: "tre".
    pub heard: String,
    /// The contrast: "æ or e".
    pub label: String,
    /// What to try: "open the mouth a little more for æ…".
    pub advice: String,
    /// A built-in pair of the contrast, to hear side by side; None when the
    /// contrast has none.
    pub pair: Option<SoundPair>,
}

/// Whether `heard` is `word` with one sound of `want` said as `got`.
fn swapped(word: &str, heard: &str, want: &Sound, got: &Sound) -> bool {
    want.spellings.iter().any(|w| {
        word.match_indices(w).any(|(at, _)| {
            got.spellings.iter().any(|g| {
                let candidate = format!("{}{g}{}", &word[..at], &word[at + w.len()..]);
                candidate == heard
            })
        })
    })
}

/// Why `word` may have been heard as `heard`: the first known contrast that
/// turns the one into the other by a single sound, with the advice for the
/// sound that was wanted. None when the two differ otherwise (a different
/// word, an ending, more than one sound).
pub fn misheard(word: &str, heard: &str) -> Option<SoundTip> {
    let (w, h) = (word.to_lowercase(), heard.to_lowercase());
    if w == h || w.is_empty() || h.is_empty() {
        return None;
    }
    CONFUSIONS.iter().find_map(|c| {
        let want = if swapped(&w, &h, &c.a, &c.b) {
            &c.a
        } else if swapped(&w, &h, &c.b, &c.a) {
            &c.b
        } else {
            return None;
        };
        Some(SoundTip {
            word: word.into(),
            heard: heard.into(),
            label: c.label.into(),
            advice: want.advice.into(),
            pair: built_in().into_iter().find(|p| p.label == c.label),
        })
    })
}

/// Whether two spellings are one word: "aa" is the old spelling of å
/// ("Aarhus", "Århus"), and an ear may write ä for æ and ö for ø. Danish
/// spellings that sound alike but differ (hv and v, "hjem" and "jem") are
/// other words, so nothing else is forgiven.
pub fn sound_alike(a: &str, b: &str) -> bool {
    let fold = |s: &str| s.to_lowercase().replace("aa", "å").replace('ä', "æ").replace('ö', "ø");
    fold(a) == fold(b)
}

/// A word's near neighbour as the tutor gave it, cleaned: one real word,
/// not the word itself (letters folded). None to leave it out.
/// Its article or "at" is left off ("et træ" is "træ"): the pair is two words.
pub fn clean_near(da: &str, near: &str) -> Option<String> {
    let near = crate::text::clean(near);
    let near = bare(&near).to_string();
    let single = !near.is_empty() && !near.contains(char::is_whitespace);
    (single && near.to_lowercase() != bare(da).to_lowercase()).then_some(near)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn row(da: &str, near: Option<(&str, &str)>) -> WordRow {
        WordRow {
            id: WordId::new(),
            da: da.into(),
            meaning: format!("{da} in English"),
            pos: "n-common".into(),
            reps: 0,
            image: None,
            near: near.map(|(da, en)| crate::session::Near { da: da.into(), en: en.into() }),
        }
    }

    #[test]
    fn built_in_pairs_are_real_pairs_with_ids_the_same_everywhere() {
        let pairs = built_in();
        assert!(pairs.len() >= 15);
        for p in &pairs {
            assert_ne!(p.a.to_lowercase(), p.b.to_lowercase());
            assert!(!p.a_en.is_empty() && !p.b_en.is_empty());
            assert_eq!(p.id, Some(pair_id(&p.a, &p.b)));
            // Each pair is named by its own contrast.
            assert_eq!(label_for(&p.a, &p.b), p.label, "{} / {}", p.a, p.b);
        }
        let ids: std::collections::HashSet<_> = pairs.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), pairs.len(), "no two pairs share an id");
    }

    #[test]
    fn the_contrast_is_named() {
        assert_eq!(label_for("træ", "tre"), "æ or e");
        assert_eq!(label_for("ny", "ni"), "y or i");
        assert_eq!(label_for("hund", "hun"), "stød or none");
        assert_eq!(label_for("hedde", "hede"), "short or long vowel");
        assert_eq!(label_for("kat", "kat"), "Listen closely");
    }

    #[test]
    fn a_lesson_hears_its_own_pairs_first_then_its_sounds() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let words = [row("kaffe", None), row("by", Some(("bi", "bee"))), row("mælk", None)];
        let refs: Vec<&WordRow> = words.iter().collect();
        let pairs = for_words(&refs, &mut rng);
        assert_eq!(pairs[0].a, "by");
        assert_eq!(pairs[0].b, "bi");
        assert_eq!(pairs[0].id, None);
        assert!(pairs.len() <= ROUND);
        // Words without the sounds: no round at all.
        let plain = [row("kat", None), row("ost", None)];
        let refs: Vec<&WordRow> = plain.iter().collect();
        assert!(round(&refs, LessonId::new(), &mut rng).is_empty());
        // Words with å: a built-in å/o pair.
        let open = [row("kål", None)];
        let refs: Vec<&WordRow> = open.iter().collect();
        let pairs = for_words(&refs, &mut rng);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].label, "å or o");
        let card = exercise(&pairs[0], LessonId::new(), &mut rng);
        assert!(card.options.contains(&card.answer));
        assert_eq!(card.options, [pairs[0].a.clone(), pairs[0].b.clone()]);
    }

    #[test]
    fn a_near_word_is_one_other_word() {
        assert_eq!(clean_near("træ", " tre "), Some("tre".into()));
        assert_eq!(clean_near("træ", "Træ"), None);
        assert_eq!(clean_near("træ", "two words"), None);
        assert_eq!(clean_near("træ", ""), None);
        assert_eq!(clean_near("et træ", "tre"), Some("tre".into()));
        assert_eq!(clean_near("at le", "at læ"), Some("læ".into()));
        assert_eq!(clean_near("et træ", "træ"), None);
    }

    #[test]
    fn a_word_heard_as_another_names_the_sound() {
        let tip = misheard("træ", "tre").expect("æ heard as e");
        assert_eq!(tip.label, "æ or e");
        assert!(tip.advice.contains("open the mouth"), "{}", tip.advice);
        assert!(tip.pair.is_some(), "the contrast has a pair to hear");
        assert!(misheard("tre", "træ").unwrap().advice.contains("close the mouth"));
        assert_eq!(misheard("lyd", "lød").unwrap().label, "ø or y");
        assert_eq!(misheard("ny", "ni").unwrap().label, "y or i");
        assert_eq!(misheard("tå", "to").unwrap().label, "å or o");
        assert_eq!(misheard("bud", "bod").unwrap().label, "u or o");
        assert_eq!(misheard("hund", "hun").unwrap().label, "stød or none");
        let tip = misheard("visse", "vise").unwrap();
        assert_eq!(tip.label, "short or long vowel");
        assert!(tip.advice.contains("short"));
        assert!(misheard("vise", "visse").unwrap().advice.contains("long"));
        assert_eq!(misheard("sød", "sø").unwrap().label, "soft d or none");
        assert_eq!(misheard("lige", "lide").unwrap().label, "soft d or none");
        assert_eq!(misheard("rat", "hat").unwrap().label, "r or h");
        // Not a sound: another word, another ending, or the same word.
        assert_eq!(misheard("kaffe", "te"), None);
        assert_eq!(misheard("kaffen", "kaffe"), None);
        assert_eq!(misheard("hus", "Hus"), None);
    }

    #[test]
    fn spellings_of_one_word_sound_alike() {
        assert!(sound_alike("Århus", "Aarhus"));
        assert!(sound_alike("smør", "smör"));
        assert!(!sound_alike("hvil", "vil"));
        assert!(!sound_alike("træ", "tre"));
    }
}

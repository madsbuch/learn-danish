//! Sound pairs: Danish sounds an English ear runs together — æ and e, ø and
//! y, y and i, å and o, u and o, a stød and none, a short vowel and a long
//! one, a soft d and none, r and h — heard in pairs of real words that
//! differ only there ("træ" / "tre"). The app says one; the learner picks
//! which it was; then the other is played, so the two are heard side by
//! side.
//!
//! Pairs come from two places:
//! - a word's own near neighbour, which the tutor gives when it makes the
//!   word (`words.near_da`, `near_en`: schema 10);
//! - the built-in list below, the core contrasts, which ships with the app
//!   like the demo course. Its pairs have ids derived from their words
//!   (DATA.md), the same on every device; nothing is written for them.
//!
//! A words lesson ends with a short round of them, and a sentences lesson
//! has one before its gaps, but only of pairs with the lesson's own words
//! (see [`for_words`], [`for_sentences`]): a pair that merely shares a sound
//! with them is not played. The quick game "Sound pairs" plays the whole
//! built-in list.

use std::collections::HashSet;

use rand::seq::SliceRandom;
use serde::Serialize;

use crate::models::{Exercise, Pair};
use crate::session::{split_punctuation, SentenceRow, WordRow};
use crate::singalong::bare;
#[allow(unused_imports)]
use crate::ids::*;

/// Sound pairs in a lesson's round: a short one.
// Spec: SPK-9, SPK-11
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

/// A word as pairs are matched on: without its article ("en", "et",
/// "at"), lower case, without punctuation, its Danish letters kept ("tå" is
/// not "to").
fn key(word: &str) -> String {
    crate::tiles::fold(bare(word))
}

/// The built-in pairs with one of these words (as [`key`]s).
fn built_in_with(keys: &HashSet<String>) -> Vec<SoundPair> {
    built_in().into_iter().filter(|p| keys.contains(&key(&p.a)) || keys.contains(&key(&p.b))).collect()
}

/// A word and its own near neighbour, as a pair; None without one.
fn own_pair(w: &WordRow) -> Option<SoundPair> {
    let near = w.near.as_ref()?;
    let (a, b) = (bare(&w.da), bare(&near.da));
    Some(SoundPair { id: None, label: label_for(a, b), a: a.into(), a_en: w.meaning.clone(), b: b.into(), b_en: near.en.clone() })
}

/// A round from the pairs found: own neighbours first, then built-in pairs,
/// each in a random order, the same two words once; at most [`ROUND`].
fn pick(mut own: Vec<SoundPair>, mut built: Vec<SoundPair>, rng: &mut impl rand::Rng) -> Vec<SoundPair> {
    own.shuffle(rng);
    built.shuffle(rng);
    let mut pairs: Vec<SoundPair> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for pair in own.into_iter().chain(built) {
        let (a, b) = (key(&pair.a), key(&pair.b));
        if seen.insert(if a <= b { (a, b) } else { (b, a) }) {
            pairs.push(pair);
        }
    }
    pairs.truncate(ROUND);
    pairs
}

/// The whole words of some sentences, as [`key`]s.
fn words_in(sentences: &[&str]) -> HashSet<String> {
    sentences
        .iter()
        .flat_map(|s| s.split_whitespace())
        .map(|token| crate::tiles::fold(split_punctuation(token).1))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Every built-in word a lesson's round could say, for words and sentences:
/// both words of each built-in pair with one of the words, or with a word
/// of one of the sentences. For keeping a course's audio ready (offline.rs).
// Spec: OFF-3
pub fn spoken_for(words: &[String], sentences: &[String]) -> Vec<String> {
    let texts: Vec<&str> = sentences.iter().map(String::as_str).collect();
    let mut keys = words_in(&texts);
    keys.extend(words.iter().map(|w| key(w)));
    built_in_with(&keys).into_iter().flat_map(|p| [p.a, p.b]).collect()
}

/// The pairs for a words lesson, each with one of its words: a word's own
/// near neighbour, or a built-in pair that has the word itself; at most
/// [`ROUND`], none when nothing matches.
// Spec: SPK-9
pub fn for_words(words: &[&WordRow], rng: &mut impl rand::Rng) -> Vec<SoundPair> {
    let keys: HashSet<String> = words.iter().map(|w| key(&w.da)).collect();
    pick(words.iter().filter_map(|w| own_pair(w)).collect(), built_in_with(&keys), rng)
}

/// The pairs for a sentences lesson, each with a whole word of one of its
/// sentences: a built-in pair that has the word, or the near neighbour of
/// one of `words` (its course's) where that word stands in a sentence, as
/// it is or in a form the sentence's endings give for it; at most
/// [`ROUND`], none when nothing matches.
// Spec: SPK-11
pub fn for_sentences(sentences: &[&SentenceRow], words: &[WordRow], rng: &mut impl rand::Rng) -> Vec<SoundPair> {
    let texts: Vec<&str> = sentences.iter().map(|s| s.da.as_str()).collect();
    let tokens = words_in(&texts);
    // Each sentence as its words joined by one space, padded, so a word of
    // several ("god morgen") is found whole.
    let lines: Vec<String> = texts.iter().map(|t| format!(" {} ", crate::tiles::fold(t))).collect();
    let bases: HashSet<String> = sentences
        .iter()
        .flat_map(|s| &s.endings)
        .filter(|e| tokens.contains(&crate::tiles::fold(&e.form)))
        .map(|e| key(&e.base))
        .collect();
    let stands = |w: &WordRow| {
        let k = key(&w.da);
        !k.is_empty() && (bases.contains(&k) || lines.iter().any(|l| l.contains(&format!(" {k} "))))
    };
    let own = words.iter().filter(|w| stands(w)).filter_map(own_pair).collect();
    pick(own, built_in_with(&tokens), rng)
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
// Spec: SPK-9
pub fn round(words: &[&WordRow], lesson_id: LessonId, rng: &mut impl rand::Rng) -> Vec<Exercise> {
    for_words(words, rng).iter().map(|pair| exercise(pair, lesson_id, rng)).collect()
}

/// A sentences lesson's round of sound pairs.
// Spec: SPK-11
pub fn sentence_round(sentences: &[&SentenceRow], words: &[WordRow], lesson_id: LessonId, rng: &mut impl rand::Rng) -> Vec<Exercise> {
    for_sentences(sentences, words, rng).iter().map(|pair| exercise(pair, lesson_id, rng)).collect()
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

/// Which known contrast tells `word` from `heard`, and which way: the
/// contrast's label, the spelling in the word and the one heard in its
/// place ("æ or e", "æ", "e" for "træ" heard as "tre"). For the learning
/// log (mistakes.rs): a sound pair missed, a word misheard.
pub fn swap(word: &str, heard: &str) -> Option<Swap> {
    let (w, h) = (word.to_lowercase(), heard.to_lowercase());
    if w == h || w.is_empty() || h.is_empty() {
        return None;
    }
    let find = |want: &Sound, got: &Sound| -> Option<(String, String)> {
        want.spellings.iter().find_map(|ws| {
            w.match_indices(ws).find_map(|(at, _)| {
                got.spellings.iter().find_map(|gs| {
                    let candidate = format!("{}{gs}{}", &w[..at], &w[at + ws.len()..]);
                    (candidate == h).then(|| ((*ws).to_string(), (*gs).to_string()))
                })
            })
        })
    };
    CONFUSIONS.iter().find_map(|c| {
        let (said, got) = find(&c.a, &c.b).or_else(|| find(&c.b, &c.a))?;
        Some(Swap { label: c.label.into(), said, heard: got })
    })
}

/// A contrast and which way it went (see [`swap`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Swap {
    pub label: String,
    pub said: String,
    pub heard: String,
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
            familiarity: 0,
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

    fn sentence(da: &str, endings: &[(&str, &str)]) -> SentenceRow {
        SentenceRow {
            id: SentenceId::new(),
            da: da.into(),
            meaning: String::new(),
            endings: endings.iter().map(|(form, base)| crate::tiles::Ending { form: (*form).into(), base: (*base).into(), wrong: vec![] }).collect(),
            familiarity: 0,
        }
    }

    // Spec: SPK-9
    #[test]
    fn a_words_lesson_plays_only_pairs_with_its_own_words() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        // "en skole" shares sounds with built-in pairs but is in none, and
        // has no neighbour: no round.
        let plain = [row("en skole", None), row("et hus", None)];
        let refs: Vec<&WordRow> = plain.iter().collect();
        assert!(for_words(&refs, &mut rng).is_empty());
        assert!(round(&refs, LessonId::new(), &mut rng).is_empty());
        // A word that is itself in a built-in pair gets that pair.
        let own = [row("en skole", None), row("en Hund", None)];
        let refs: Vec<&WordRow> = own.iter().collect();
        let pairs = for_words(&refs, &mut rng);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].a.as_str(), pairs[0].b.as_str()), ("hund", "hun"));
        assert!(pairs[0].id.is_some());
        let card = exercise(&pairs[0], LessonId::new(), &mut rng);
        assert!(card.options.contains(&card.answer));
        assert_eq!(card.options, [pairs[0].a.clone(), pairs[0].b.clone()]);
        // A near neighbour is used, first, and not again as the built-in pair
        // of the same two words.
        let near = [row("kaffe", None), row("en by", Some(("en bi", "bee"))), row("mælk", None)];
        let refs: Vec<&WordRow> = near.iter().collect();
        let pairs = for_words(&refs, &mut rng);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].a.as_str(), pairs[0].b.as_str(), pairs[0].id), ("by", "bi", None));
    }

    // Spec: SPK-9, SPK-11
    #[test]
    fn a_round_has_at_most_round_pairs() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(3);
        let many = [row("hun", None), row("to", None), row("ni", None), row("tre", Some(("træ", "tree")))];
        let refs: Vec<&WordRow> = many.iter().collect();
        assert_eq!(for_words(&refs, &mut rng).len(), ROUND);
        let s = [sentence("Hun har to katte og ni fisk.", &[])];
        let refs: Vec<&SentenceRow> = s.iter().collect();
        assert_eq!(for_sentences(&refs, &[], &mut rng).len(), ROUND);
    }

    // Spec: SPK-11
    #[test]
    fn a_sentences_lesson_plays_pairs_with_words_of_its_sentences() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        // "to" stands in a sentence: the built-in pair with "to".
        let s = [sentence("Jeg har to katte.", &[])];
        let refs: Vec<&SentenceRow> = s.iter().collect();
        let pairs = for_sentences(&refs, &[], &mut rng);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].a.as_str(), pairs[0].b.as_str()), ("tå", "to"));
        // Whole words only: "tog" is not "to", and shared sounds bring nothing.
        let none = [sentence("Toget er på vej, sødt.", &[])];
        let refs: Vec<&SentenceRow> = none.iter().collect();
        assert!(for_sentences(&refs, &[], &mut rng).is_empty());
        let plain = [sentence("Jeg har en kat.", &[])];
        let refs: Vec<&SentenceRow> = plain.iter().collect();
        assert!(sentence_round(&refs, &[], LessonId::new(), &mut rng).is_empty());
        // A course word's near neighbour, where the word stands in a sentence,
        // as it is or in a form its endings give; not where it doesn't.
        let words = [row("et træ", Some(("tre", "three"))), row("en kop", Some(("en kok", "a cook"))), row("en ske", Some(("ski", "skis")))];
        let said = |da: &str, endings: &[(&str, &str)], rng: &mut rand::rngs::StdRng| {
            let s = [sentence(da, endings)];
            let refs: Vec<&SentenceRow> = s.iter().collect();
            for_sentences(&refs, &words, rng).into_iter().map(|p| format!("{}/{}", p.a, p.b)).collect::<Vec<_>>()
        };
        assert_eq!(said("Jeg ser træet.", &[("træet", "et træ")], &mut rng), ["træ/tre"]);
        assert!(said("Jeg ser træet.", &[], &mut rng).is_empty(), "no form given: no match");
        assert_eq!(said("En kop kaffe, tak!", &[], &mut rng), ["kop/kok"]);
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

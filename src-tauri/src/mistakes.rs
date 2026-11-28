//! What kind of mistake an answer was: worked out in Rust when the answer
//! is graded, and written with it to the learning log (learnlog.rs), so the
//! screens and the tutor can say *what* the learner finds hard — æ, ø and å,
//! the endings (and en / et), the order (verb second, ikke), a sound, a
//! preposition — not just how often they miss.
//!
//! One class per answer, the most telling one that fits, in this order:
//! a sound contrast or a preposition (the card was about exactly that), the
//! Danish letters alone, the order alone, an ending, a typo, some words
//! right (a sentence), and otherwise a word not known.

use serde::{Deserialize, Serialize};

use crate::dictation::DiffWord;
use crate::practice::normalize;
use crate::session::split_punctuation;
use crate::tiles::{self, Ending};

/// Every class, with what the screens call it.
pub const CLASSES: &[(&str, &str)] = &[
    ("diacritics", "Danish letters"),
    ("ending", "Endings and en / et"),
    ("word_order", "Word order"),
    ("sound_contrast", "Sounds"),
    ("preposition", "Prepositions"),
    ("typo", "Typos"),
    ("lookalike", "Look-alike words"),
    ("partial", "Some words wrong"),
    ("unknown_word", "Didn't know it"),
];

pub fn label(class: &str) -> &'static str {
    CLASSES.iter().find(|(c, _)| *c == class).map_or("", |(_, l)| l)
}

/// Exercises where the answer is picked from options, not typed: a pick that
/// looks like the answer is a look-alike, never a typo.
pub(crate) const PICKED: &[&str] = &[
    "recognise", "produce", "wordPick", "picturePick", "dialogueGap", "storyQuestion", "sceneFind", "sceneName", "sceneWhere",
    "soundPair", "game:sounds", "game:match", "memory", "gapFill",
];

/// Prepositions, longest first so "ved siden af" is found before "ved": a
/// scene's place words (scenes.rs) and the others a learner mixes up.
const PREPOSITIONS: &[&str] = &[
    "ved siden af", "over for", "inde i", "oven på", "uden for", "i stedet for", "under", "over", "ved", "på", "i", "bag", "bagved", "foran",
    "mellem", "til", "fra", "hos", "med", "uden", "om", "af", "for", "efter", "før", "langs", "gennem", "omkring", "mod",
];

/// Little words that agree with their noun (en / et, den / det / de, min /
/// mit / mine…): one taken for another of its set is an agreement slip,
/// counted with the endings.
const AGREEING: &[&[&str]] = &[
    &["en", "et"],
    &["den", "det", "de"],
    &["min", "mit", "mine"],
    &["din", "dit", "dine"],
    &["sin", "sit", "sine"],
    &["ingen", "intet"],
    &["nogen", "noget", "nogle"],
    &["hvilken", "hvilket", "hvilke"],
    &["denne", "dette", "disse"],
    &["anden", "andet", "andre"],
    &["lille", "små"],
];

fn agreeing(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    a != b && AGREEING.iter().any(|set| set.contains(&a.as_str()) && set.contains(&b.as_str()))
}

/// An answer, as graded: everything the classes are told apart by.
#[derive(Debug, Default)]
pub struct Graded<'a> {
    pub exercise: &'a str,
    pub expected: &'a str,
    pub given: &'a str,
    pub correct: bool,
    /// A dictation's or a spoken answer's word-by-word comparison.
    pub diff: &'a [DiffWord],
    /// The options shown (a sound pair: both words).
    pub options: &'a [String],
    /// What the card showed (a true-or-false scene card: the statement).
    pub prompt: &'a str,
    /// The Danish the card is about (a scene card: the true sentence).
    pub da: &'a str,
    /// A sentence's endings (word-order tiles).
    pub endings: &'a [Ending],
}

/// One word of a sentence, compared: what was expected, what was given,
/// and how it went (r right, a æ ø å written otherwise, w wrong, e wrong ending or en / et,
/// m missing, x extra, o out of order).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WordResult {
    pub e: String,
    pub g: String,
    pub s: char,
}

/// A mistake: its class ('' when the answer was right and clean), what
/// exactly, a sound contrast when there was one, and word-by-word results
/// for a sentence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mistake {
    pub class: &'static str,
    pub detail: String,
    pub contrast: String,
    pub words: Vec<WordResult>,
}

impl Mistake {
    fn of(class: &'static str, detail: impl Into<String>) -> Self {
        Mistake { class, detail: detail.into(), ..Mistake::default() }
    }
}

/// Lower case, letters kept as written (unlike [`normalize`], which writes
/// æ, ø and å as ae, oe and aa).
fn plain(text: &str) -> String {
    tiles::fold(text)
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(|t| split_punctuation(t).1.to_lowercase()).filter(|w| !normalize(w).is_empty()).collect()
}

/// Edits (insert, delete, change a letter) from one to the other.
pub fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(row[j + 1] + 1);
            prev = here;
        }
    }
    row[b.len()]
}

/// The same word with another ending: a shared stem of at least three
/// letters (Danish letters folded), and short different tails ("bilen" and
/// "bilerne", "stor" and "stort", "spiser" and "spiste") — or en for et, den
/// for det, min for mit: the agreement that goes with the noun.
pub fn same_stem(a: &str, b: &str) -> bool {
    if agreeing(a, b) {
        return true;
    }
    let (a, b) = (normalize(a), normalize(b));
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let shared = ac.iter().zip(&bc).take_while(|(x, y)| x == y).count();
    let raw_differ = a != b || ac.len() != bc.len();
    let (ta, tb): (Vec<char>, Vec<char>) = (ac[shared..].to_vec(), bc[shared..].to_vec());
    // Letters swapped round ("tusnid" for "tusind") are a slip of the
    // fingers, not an ending.
    let mut sa = ta.clone();
    let mut sb = tb.clone();
    sa.sort_unstable();
    sb.sort_unstable();
    let swapped = !ta.is_empty() && sa == sb;
    // So is a letter dropped or added before an ending that is still there
    // ("æbe" for "æble").
    let (short, long) = if ta.len() <= tb.len() { (&ta, &tb) } else { (&tb, &ta) };
    let dropped = !short.is_empty() && long.ends_with(short);
    shared >= 3 && ta.len() <= 4 && tb.len() <= 4 && !swapped && !dropped && (raw_differ || shared < ac.len())
}

/// Only æ, ø and å differ: written ae, oe, aa (accepted when typed:
/// "maelk" for "mælk") or as a bare a, o ("pa" for "på", "ol" for "øl",
/// "malk" for "mælk"). Not æ as e, å as o or ø as y: those are other words
/// ("tre", "to", "lyd"), the sounds' business.
fn only_diacritics(expected: &str, given: &str) -> bool {
    let (e, g) = (plain(expected), plain(given));
    e != g && !normalize(&e).is_empty() && (normalize(&e) == normalize(&g) || loosely(&normalize(&e)) == loosely(&normalize(&g)))
}

/// A normalized text (æ ø å already ae, oe, aa) with those written as a
/// bare a or o: what a keyboard without them gives.
fn loosely(normalized: &str) -> String {
    normalized.replace("ae", "a").replace("oe", "o").replace("aa", "a")
}

/// The preposition a place phrase or statement has: the first (longest)
/// one, as words ("ved siden af bordet" → "ved siden af").
fn preposition_in(text: &str) -> String {
    let ws = words(text);
    for start in 0..ws.len() {
        for p in PREPOSITIONS {
            let pw: Vec<&str> = p.split(' ').collect();
            if ws.len() >= start + pw.len() && ws[start..start + pw.len()].iter().zip(&pw).all(|(a, b)| a == b) {
                return (*p).to_string();
            }
        }
    }
    String::new()
}

/// Classifies one graded answer.
pub fn classify(g: &Graded) -> Mistake {
    let given = g.given.trim();
    // A sound pair: which sound was heard as which, right or wrong.
    if g.exercise == "soundPair" || g.exercise == "game:sounds" {
        return sound_pair(g);
    }
    if g.exercise == "sceneWhere" {
        if let Some(m) = preposition(g) {
            return m;
        }
    }
    if !g.diff.is_empty() {
        return sentence(g);
    }
    if g.exercise == "gapFill" {
        return gaps(g);
    }
    if g.correct {
        // Right, but with Danish letters left off (typing forgives them).
        return if only_diacritics(g.expected, given) { Mistake::of("diacritics", format!("{}→{}", plain(g.expected), plain(given))) } else { Mistake::default() };
    }
    if normalize(given).is_empty() {
        return Mistake::of("unknown_word", "no answer");
    }
    if crate::practice::TILE_KINDS.contains(&g.exercise) {
        return tiles_order(g);
    }
    one_answer(g.exercise, g.expected, given)
}

/// A word or phrase given for another: the classes in order.
fn one_answer(exercise: &str, expected: &str, given: &str) -> Mistake {
    // Glosses carry alternatives ("piece, bit"): the nearest one counts.
    let expected = expected
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .min_by_key(|e| distance(&normalize(e), &normalize(given)))
        .unwrap_or(expected);
    let what = format!("{}→{}", plain(expected), plain(given));
    if only_diacritics(expected, given) {
        return Mistake::of("diacritics", what);
    }
    let (ew, gw) = (words(expected), words(given));
    // Picking an English meaning: endings and sounds are Danish matters.
    let danish = exercise != "recognise";
    // The right words in another order: verb second missed ("I dag jeg
    // spiser" for "I dag spiser jeg"), ikke in the wrong place.
    if ew.len() > 1 && ew.len() == gw.len() {
        let (mut a, mut b) = (ew.iter().map(|w| normalize(w)).collect::<Vec<_>>(), gw.iter().map(|w| normalize(w)).collect::<Vec<_>>());
        a.sort();
        b.sort();
        if a == b {
            return Mistake::of("word_order", what);
        }
    }
    // One word each: a sound heard (or written) as its neighbour.
    if danish && ew.len() == 1 && gw.len() == 1 {
        if let Some(swap) = crate::sounds::swap(&ew[0], &gw[0]) {
            return Mistake { class: "sound_contrast", detail: format!("{}→{}", swap.said, swap.heard), contrast: swap.label, words: vec![] };
        }
    }
    if danish && ew.len() == gw.len() && !ew.is_empty() {
        let changed: Vec<(&String, &String)> = ew.iter().zip(&gw).filter(|(e, g)| normalize(e) != normalize(g)).collect();
        // One preposition for another ("i arbejde" for "på arbejde").
        if changed.len() == 1 && ew.len() > 1 && PREPOSITIONS.contains(&changed[0].0.as_str()) && PREPOSITIONS.contains(&changed[0].1.as_str()) {
            return Mistake::of("preposition", format!("{}→{}", changed[0].0, changed[0].1));
        }
        if !changed.is_empty() && changed.iter().all(|(e, g)| same_stem(e, g)) {
            let detail: Vec<String> = changed.iter().map(|(e, g)| format!("{e}→{g}")).collect();
            return Mistake::of("ending", detail.join(", "));
        }
    }
    let edits = distance(&normalize(expected), &normalize(given));
    if (1..=2).contains(&edits) && normalize(expected).chars().count() >= 3 {
        return Mistake::of(if PICKED.contains(&exercise) { "lookalike" } else { "typo" }, what);
    }
    if ew.len() > 1 && gw.iter().any(|g| ew.iter().any(|e| normalize(e) == normalize(g))) {
        return Mistake::of("partial", what);
    }
    Mistake::of("unknown_word", what)
}

fn sound_pair(g: &Graded) -> Mistake {
    let other = g.options.iter().find(|o| o.trim().to_lowercase() != g.expected.trim().to_lowercase()).cloned().unwrap_or_default();
    if g.correct {
        // Heard right: which sound it was, so the Sounds page counts hits.
        let said = crate::sounds::swap(g.expected, &other);
        let label = said.as_ref().map_or_else(|| crate::sounds::label_for(g.expected, &other), |s| s.label.clone());
        return Mistake { class: "", detail: said.map(|s| s.said).unwrap_or_default(), contrast: label, words: vec![] };
    }
    match crate::sounds::swap(g.expected, g.given) {
        Some(swap) => Mistake { class: "sound_contrast", detail: format!("{}→{}", swap.said, swap.heard), contrast: swap.label, words: vec![] },
        None => Mistake {
            class: "sound_contrast",
            detail: format!("{}→{}", plain(g.expected), plain(g.given)),
            contrast: crate::sounds::label_for(g.expected, g.given),
            words: vec![],
        },
    }
}

/// A scene's "Where is it?": which preposition was taken for which. The
/// choice card gives the phrase picked; a true-or-false card, the statement
/// shown against the true one.
fn preposition(g: &Graded) -> Option<Mistake> {
    use crate::lesson_types::scene::{FALSE, TRUE};
    let is_tf = g.expected == TRUE || g.expected == FALSE;
    if !is_tf {
        if g.correct {
            return Some(Mistake::default());
        }
        let (right, picked) = (preposition_in(g.expected), preposition_in(g.given));
        if right != picked && !right.is_empty() && !picked.is_empty() {
            return Some(Mistake::of("preposition", format!("{right}→{picked}")));
        }
        return None;
    }
    if g.correct {
        return Some(Mistake::default());
    }
    // The prepositions of the statement shown and of the true sentence;
    // when they are the same, the words where the two part.
    let (right, said) = (preposition_in(g.da), preposition_in(g.prompt));
    let (shown, truth) = (words(g.prompt), words(g.da));
    let start = shown.iter().zip(&truth).take_while(|(a, b)| a == b).count();
    if g.expected == FALSE {
        // A wrong place believed: the true preposition taken for the shown one.
        let (right, believed) = if right != said && !right.is_empty() && !said.is_empty() {
            (right, said)
        } else {
            (truth.get(start).cloned().unwrap_or_default(), shown.get(start).cloned().unwrap_or_default())
        };
        return Some(Mistake::of("preposition", format!("{right}→{believed}")));
    }
    // The truth doubted: its preposition was not trusted.
    Some(Mistake::of("preposition", format!("{right}→?")))
}

/// A dictation or something said: word by word.
fn sentence(g: &Graded) -> Mistake {
    let results: Vec<WordResult> = g
        .diff
        .iter()
        .map(|w| {
            let s = match w.status.as_str() {
                "right" => 'r',
                "accents" => 'a',
                "missing" => 'm',
                "extra" => 'x',
                _ if only_diacritics(&w.expected, &w.given) => 'a',
                _ if same_stem(&w.expected, &w.given) => 'e',
                _ => 'w',
            };
            WordResult { e: w.expected.clone(), g: w.given.clone(), s }
        })
        .collect();
    let code: String = results.iter().map(|r| r.s).collect();
    let off: Vec<&WordResult> = results.iter().filter(|r| r.s != 'r').collect();
    let with = |class: &'static str, contrast: String| Mistake { class, detail: code.clone(), contrast, words: results.clone() };
    if off.is_empty() {
        return Mistake { words: results.clone(), ..Mistake::default() };
    }
    if off.iter().all(|r| r.s == 'a') {
        return with("diacritics", String::new());
    }
    if g.correct {
        // Accepted (a dictation forgives the letters; speech, a word the ear
        // can't tell apart): kept with the words, but no mistake.
        return Mistake { words: results.clone(), detail: code.clone(), ..Mistake::default() };
    }
    // The right words, in another order (verb second, ikke): the comparison
    // shows them missing in one place and extra in another.
    let mut said: Vec<String> = results.iter().filter(|r| !r.e.is_empty()).map(|r| normalize(&r.e)).collect();
    let mut put: Vec<String> = results.iter().filter(|r| !r.g.is_empty()).map(|r| normalize(&r.g)).collect();
    said.sort();
    put.sort();
    if said.len() > 1 && said == put {
        return with("word_order", String::new());
    }
    let wrong: Vec<&&WordResult> = off.iter().filter(|r| r.s != 'a').collect();
    let swaps: Vec<crate::sounds::Swap> = wrong.iter().filter_map(|r| crate::sounds::swap(&r.e, &r.g)).collect();
    if !swaps.is_empty() && swaps.len() == wrong.len() {
        return with("sound_contrast", swaps[0].label.clone());
    }
    if wrong.iter().all(|r| r.s == 'e') {
        return with("ending", String::new());
    }
    if results.iter().any(|r| r.s == 'r' || r.s == 'a') {
        return with("partial", swaps.first().map(|s| s.label.clone()).unwrap_or_default());
    }
    with("unknown_word", String::new())
}

/// Fill the gaps: each gap on its own, the class of the worst.
fn gaps(g: &Graded) -> Mistake {
    let expected: Vec<&str> = g.expected.split(" | ").collect();
    let given: Vec<&str> = g.given.split(" | ").collect();
    let mut results = Vec::new();
    let mut classes: Vec<Mistake> = Vec::new();
    for (i, e) in expected.iter().enumerate() {
        let gv = given.get(i).copied().unwrap_or("");
        let right = normalize(e) == normalize(gv);
        let m = if right { Mistake::default() } else { one_answer("gapFill", e, gv) };
        results.push(WordResult { e: (*e).into(), g: gv.into(), s: if right { 'r' } else if m.class == "ending" { 'e' } else { 'w' } });
        if !right {
            classes.push(m);
        }
    }
    let code: String = results.iter().map(|r| r.s).collect();
    match classes.len() {
        0 => Mistake { words: results, ..Mistake::default() },
        1 => Mistake { words: results, ..classes.remove(0) },
        n if n < expected.len() => Mistake { class: "partial", detail: code, contrast: String::new(), words: results },
        _ => {
            let class = if classes.iter().all(|c| c.class == "ending") { "ending" } else { "unknown_word" };
            Mistake { class, detail: code, contrast: String::new(), words: results }
        }
    }
}

/// Word-order tiles: the order, and the endings picked on tiles.
fn tiles_order(g: &Graded) -> Mistake {
    let (ew, gw) = (words(g.expected), words(g.given));
    let mut results: Vec<WordResult> = Vec::new();
    let mut endings: Vec<String> = Vec::new();
    let mut misplaced = false;
    for (i, e) in ew.iter().enumerate() {
        let given = gw.get(i).cloned().unwrap_or_default();
        let s = if plain(e) == plain(&given) {
            'r'
        } else if g.endings.iter().any(|end| plain(&end.form) == plain(e) && (plain(&end.base) == plain(&given) || end.wrong.iter().any(|w| plain(w) == plain(&given))))
            || (same_stem(e, &given) && !gw.iter().any(|x| plain(x) == plain(e)))
        {
            endings.push(format!("{e}→{given}"));
            'e'
        } else {
            misplaced = true;
            'o'
        };
        results.push(WordResult { e: e.clone(), g: given, s });
    }
    let code: String = results.iter().map(|r| r.s).collect();
    if !endings.is_empty() && !misplaced {
        return Mistake { class: "ending", detail: endings.join(", "), contrast: String::new(), words: results };
    }
    if !endings.is_empty() {
        // Both: the endings are the finer point, the order shows in the words.
        return Mistake { class: "ending", detail: format!("{} · order {code}", endings.join(", ")), contrast: String::new(), words: results };
    }
    Mistake { class: "word_order", detail: code, contrast: String::new(), words: results }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrong(exercise: &str, expected: &str, given: &str) -> Mistake {
        classify(&Graded { exercise, expected, given, ..Graded::default() })
    }

    #[test]
    fn danish_letters_written_otherwise() {
        // ae, oe, aa are accepted when typed: right, but noted.
        let right = classify(&Graded { exercise: "type", expected: "mælk", given: "maelk", correct: true, ..Graded::default() });
        assert_eq!((right.class, right.detail.as_str()), ("diacritics", "mælk→maelk"));
        let clean = classify(&Graded { exercise: "type", expected: "Smørrebrød", given: "smørrebrød!", correct: true, ..Graded::default() });
        assert_eq!(clean.class, "");
        // A bare a or o is not accepted, but it is the letters, not the word.
        assert_eq!(wrong("type", "på", "pa").class, "diacritics");
        assert_eq!(wrong("type", "øl", "ol").class, "diacritics");
        assert_eq!(wrong("type", "Hun går hjem", "Hun gar hjem").class, "diacritics");
        // æ as e is another word: a sound.
        assert_eq!(wrong("type", "træ", "tre").class, "sound_contrast");
    }

    #[test]
    fn endings_word_order_typos_and_unknown_words() {
        let ending = wrong("type", "bilen", "bilerne");
        assert_eq!((ending.class, ending.detail.as_str()), ("ending", "bilen→bilerne"));
        assert_eq!(wrong("type", "et stort hus", "et stor hus").class, "ending");
        assert_eq!(wrong("type", "spiste", "spiser").class, "ending");
        let gender = wrong("type", "et hus", "en hus");
        assert_eq!((gender.class, gender.detail.as_str()), ("ending", "et→en"));
        assert_eq!(wrong("type", "mit hus", "min hus").class, "ending");
        // Verb second and ikke: the right words, the wrong order.
        assert_eq!(wrong("type", "I dag spiser jeg fisk", "I dag jeg spiser fisk").class, "word_order");
        assert_eq!(wrong("type", "fordi jeg ikke har tid", "fordi jeg har ikke tid").class, "word_order");
        assert_eq!(wrong("type", "tusind", "tusnid").class, "typo");
        assert_eq!(wrong("type", "æble", "æbe").class, "typo");
        assert_eq!(wrong("produce", "kat", "kan").class, "lookalike", "a pick is never a typo");
        assert_eq!(wrong("type", "hund", "cykel").class, "unknown_word");
        assert_eq!(wrong("type", "hund", "").detail, "no answer");
        // A gloss with alternatives: the nearest counts.
        assert_eq!(wrong("recognise", "piece, bit", "bite").class, "lookalike");
    }

    #[test]
    fn sounds_heard_as_their_neighbour() {
        let m = wrong("type", "by", "bi");
        assert_eq!((m.class, m.contrast.as_str(), m.detail.as_str()), ("sound_contrast", "y or i", "y→i"));
        assert_eq!(wrong("type", "hund", "hun").contrast, "stød or none");
        assert_eq!(wrong("type", "rat", "hat").contrast, "r or h");
        let options = vec!["tå".to_string(), "to".to_string()];
        let missed = classify(&Graded { exercise: "soundPair", expected: "tå", given: "to", options: &options, ..Graded::default() });
        assert_eq!((missed.class, missed.contrast.as_str(), missed.detail.as_str()), ("sound_contrast", "å or o", "å→o"));
        let heard = classify(&Graded { exercise: "soundPair", expected: "to", given: "to", correct: true, options: &options, ..Graded::default() });
        assert_eq!((heard.class, heard.contrast.as_str(), heard.detail.as_str()), ("", "å or o", "o"));
    }

    #[test]
    fn prepositions_in_a_scene_and_in_a_phrase() {
        let pick = wrong("sceneWhere", "under bordet", "på bordet");
        assert_eq!((pick.class, pick.detail.as_str()), ("preposition", "under→på"));
        let beside = wrong("sceneWhere", "ved siden af bordet", "ved bordet");
        assert_eq!((beside.class, beside.detail.as_str()), ("preposition", "ved siden af→ved"));
        let believed = classify(&Graded {
            exercise: "sceneWhere",
            expected: "Falsk",
            given: "Sandt",
            prompt: "Katten er på bordet.",
            da: "Katten er under bordet.",
            ..Graded::default()
        });
        assert_eq!((believed.class, believed.detail.as_str()), ("preposition", "under→på"));
        let longer = classify(&Graded {
            exercise: "sceneWhere",
            expected: "Falsk",
            given: "Sandt",
            prompt: "Katten er ved bordet.",
            da: "Katten er ved siden af bordet.",
            ..Graded::default()
        });
        assert_eq!(longer.detail, "ved siden af→ved");
        let doubted = classify(&Graded { exercise: "sceneWhere", expected: "Sandt", given: "Falsk", prompt: "Katten er i kassen.", da: "Katten er i kassen.", ..Graded::default() });
        assert_eq!(doubted.detail, "i→?");
        let typed = wrong("type", "Jeg er på arbejde", "Jeg er i arbejde");
        assert_eq!((typed.class, typed.detail.as_str()), ("preposition", "på→i"));
    }

    #[test]
    fn sentences_word_by_word() {
        let checked = crate::dictation::check("Jeg drikker kaffen med mælk.", "Jeg drikker kaffe med maelk");
        let m = classify(&Graded { exercise: "dictation", expected: "Jeg drikker kaffen med mælk.", given: "Jeg drikker kaffe med maelk", correct: checked.correct, diff: &checked.words, ..Graded::default() });
        assert_eq!((m.class, m.detail.as_str()), ("ending", "rrera"));
        let checked = crate::dictation::check("Jeg bor i København.", "Jeg bor i huset.");
        let m = classify(&Graded { exercise: "dictation", expected: "", given: "", correct: checked.correct, diff: &checked.words, ..Graded::default() });
        assert_eq!((m.class, m.words.len()), ("partial", 4));
        let spoken = crate::speaking::check("Jeg bor i en by", "Jeg bor i en bi");
        let m = classify(&Graded { exercise: "sayIt", correct: spoken.correct, diff: &spoken.words, ..Graded::default() });
        assert_eq!((m.class, m.contrast.as_str()), ("sound_contrast", "y or i"));
        let letters = crate::dictation::check("Hun går hjem", "Hun gaar hjem");
        let m = classify(&Graded { exercise: "dictation", correct: letters.correct, diff: &letters.words, ..Graded::default() });
        assert_eq!(m.class, "diacritics");
        let order = crate::dictation::check("I dag spiser jeg fisk.", "I dag jeg spiser fisk.");
        let m = classify(&Graded { exercise: "dictation", correct: order.correct, diff: &order.words, ..Graded::default() });
        assert_eq!(m.class, "word_order");
    }

    #[test]
    fn tiles_tell_endings_from_order() {
        let endings = vec![Ending { form: "kaffen".into(), base: "kaffe".into(), wrong: vec!["kaffer".into(), "kaffens".into()] }];
        let m = classify(&Graded { exercise: "wordOrder", expected: "Jeg drikker kaffen.", given: "Jeg drikker kaffens.", endings: &endings, ..Graded::default() });
        assert_eq!((m.class, m.detail.as_str()), ("ending", "kaffen→kaffens"));
        assert_eq!(m.words.iter().map(|w| w.s).collect::<String>(), "rre");
        let m = classify(&Graded { exercise: "wordOrder", expected: "Nu drikker jeg kaffen.", given: "Nu jeg drikker kaffen.", endings: &endings, ..Graded::default() });
        assert_eq!((m.class, m.detail.as_str()), ("word_order", "roor"));
    }

    #[test]
    fn gaps_one_by_one() {
        let m = wrong("gapFill", "kaffen | mælk | huset", "kaffen | mælken | huset");
        assert_eq!((m.class, m.words.iter().map(|w| w.s).collect::<String>().as_str()), ("ending", "rer"));
        let m = wrong("gapFill", "kaffen | mælk | huset", "kaffen | hund | bil");
        assert_eq!(m.class, "partial");
    }
}

//! Dictation: the app says a sentence, the learner writes what they heard.
//!
//! Checking is as forgiving as typing a word (practice::normalize): case,
//! punctuation and spacing never count, and Danish letters typed the keyboard
//! way, ae oe aa ("maelk" for "mælk"), are accepted — but pointed out, word
//! by word, so the learner sees which letters they missed. The answer comes
//! back as a word-by-word comparison: right words, wrong ones with the right
//! form, words left out, and words that were not said.

use serde::Serialize;

use crate::practice::normalize;
use crate::session::split_punctuation;

/// One word of a dictation, compared.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffWord {
    /// The word as the sentence has it, without the punctuation around it;
    /// empty for a word typed that was not said.
    pub expected: String,
    /// What was typed in its place; empty for a word left out.
    pub given: String,
    /// right | accents (right, but its Danish letters were typed as ae, oe
    /// or aa: accepted, and shown) | wrong | missing | extra
    pub status: String,
}

/// A dictation checked: right when every word was heard (Danish letters
/// forgiven), with the comparison to show.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub correct: bool,
    pub words: Vec<DiffWord>,
}

/// The words of a text, without their punctuation; dashes and other marks
/// standing alone are not words.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| split_punctuation(token).1.to_string())
        .filter(|word| !normalize(word).is_empty())
        .collect()
}

fn word(expected: &str, given: &str, status: &str) -> DiffWord {
    DiffWord { expected: expected.into(), given: given.into(), status: status.into() }
}

/// Compares what was typed with the sentence, word by word. The words in
/// common are lined up first (the longest common run, letters folded), so
/// one word left out doesn't make every word after it wrong; between them,
/// words typed in place of others are wrong, and the rest missing or extra.
pub fn check(expected: &str, given: &str) -> Checked {
    let (said, typed) = (words(expected), words(given));
    let said_keys: Vec<String> = said.iter().map(|w| normalize(w)).collect();
    let typed_keys: Vec<String> = typed.iter().map(|w| normalize(w)).collect();
    let (n, m) = (said.len(), typed.len());
    // common[i][j]: the longest run in common of said[i..] and typed[j..].
    let mut common = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i][j] = if said_keys[i] == typed_keys[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut matched = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if said_keys[i] == typed_keys[j] {
            matched.push((i, j));
            i += 1;
            j += 1;
        } else if common[i + 1][j] >= common[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    let mut out = Vec::new();
    let (mut from_said, mut from_typed) = (0, 0);
    for (at_said, at_typed) in matched.into_iter().chain(std::iter::once((n, m))) {
        out.extend(between(&said[from_said..at_said], &typed[from_typed..at_typed]));
        if at_said < n {
            let (e, g) = (&said[at_said], &typed[at_typed]);
            let status = if e.to_lowercase() == g.to_lowercase() { "right" } else { "accents" };
            out.push(word(e, g, status));
        }
        from_said = at_said + 1;
        from_typed = at_typed + 1;
    }
    let correct = n > 0 && out.iter().all(|w| w.status == "right" || w.status == "accents");
    Checked { correct, words: out }
}

/// The fewest words a sentence needs for one word off to be "almost".
pub const NEARLY_WORDS: usize = 3;

/// A sentence written (or said) all but one word right: of a sentence of
/// three words or more, exactly one word wrong, left out or added — the
/// rest right (Danish letters forgiven). Wrong, but nearly there: it counts
/// as almost (half XP, half a point; practice.rs).
pub fn nearly(words: &[DiffWord]) -> bool {
    let said = words.iter().filter(|w| !w.expected.is_empty()).count();
    let off = words.iter().filter(|w| !matches!(w.status.as_str(), "right" | "accents")).count();
    said >= NEARLY_WORDS && off == 1
}

/// How alike two words are, 0 to 1, by their letters (folded): one minus
/// the edits between them over the longer's length.
fn likeness(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (normalize(a).chars().collect(), normalize(b).chars().collect());
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let here = (row[j + 1] + 1).min(row[j] + 1).min(diagonal + usize::from(x != y));
            diagonal = row[j + 1];
            row[j + 1] = here;
        }
    }
    1.0 - crate::convert::real_len(row[b.len()]) / crate::convert::real_len(longest)
}

/// The words between two lined-up ones: each typed word goes against the
/// said word most like it, in order ("mælkn" against "mælken", not
/// against the "med" before it); said words left over were missed, typed
/// ones left over were not said.
fn between(said: &[String], typed: &[String]) -> Vec<DiffWord> {
    let (n, m) = (said.len(), typed.len());
    // best[i][j]: the most likeness from pairing said[i..] with typed[j..].
    let mut best = vec![vec![0.0f64; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let pair = best[i + 1][j + 1] + likeness(&said[i], &typed[j]) + 0.01;
            best[i][j] = pair.max(best[i + 1][j]).max(best[i][j + 1]);
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && (best[i][j] - (best[i + 1][j + 1] + likeness(&said[i], &typed[j]) + 0.01)).abs() < 1e-9 {
            out.push(word(&said[i], &typed[j], "wrong"));
            i += 1;
            j += 1;
        } else if i < n && (j == m || (best[i][j] - best[i + 1][j]).abs() < 1e-9) {
            out.push(word(&said[i], "", "missing"));
            i += 1;
        } else {
            out.push(word("", &typed[j], "extra"));
            j += 1;
        }
    }
    out
}

/// Checks a dictation: for the quick game, which checks as it goes.
#[tauri::command]
pub fn dictation_check(expected: String, given: String) -> Checked {
    check(&expected, &given)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statuses(checked: &Checked) -> Vec<(&str, &str, &str)> {
        checked.words.iter().map(|w| (w.expected.as_str(), w.given.as_str(), w.status.as_str())).collect()
    }

    #[test]
    fn case_punctuation_and_danish_letters_are_forgiven_but_shown() {
        let checked = check("Kaffe med mælk, tak.", "kaffe, med MAELK tak");
        assert!(checked.correct);
        assert_eq!(
            statuses(&checked),
            [("Kaffe", "kaffe", "right"), ("med", "med", "right"), ("mælk", "MAELK", "accents"), ("tak", "tak", "right")]
        );
        assert_eq!(check("Hun går hjem.", "Hun gaar hjem").words[1].status, "accents");
        // A letter left plain is another word: "gar" is not "går".
        assert_eq!(check("Hun går hjem.", "Hun gar hjem").words[1].status, "wrong");
    }

    #[test]
    fn a_word_left_out_does_not_make_the_rest_wrong() {
        let checked = check("Hvor er stationen?", "hvor stationen");
        assert!(!checked.correct);
        assert_eq!(statuses(&checked), [("Hvor", "hvor", "right"), ("er", "", "missing"), ("stationen", "stationen", "right")]);
    }

    #[test]
    fn a_wrong_word_shows_the_right_one_and_extras_are_marked() {
        let checked = check("Jeg køber brød hos bageren.", "jeg koeber brødet hos hos bageren");
        assert!(!checked.correct);
        assert_eq!(
            statuses(&checked),
            [
                ("Jeg", "jeg", "right"),
                ("køber", "koeber", "accents"),
                ("brød", "brødet", "wrong"),
                ("hos", "hos", "right"),
                ("", "hos", "extra"),
                ("bageren", "bageren", "right"),
            ]
        );
    }

    #[test]
    fn a_misspelt_word_goes_against_the_word_it_was_meant_for() {
        let checked = check("Kaffe med mælken.", "kaffe mælkn");
        assert_eq!(
            statuses(&checked)[1..],
            [("med", "", "missing"), ("mælken", "mælkn", "wrong")]
        );
        assert!(likeness("mælken", "mælkn") > likeness("med", "mælkn"));
    }

    #[test]
    fn nothing_typed_is_every_word_missing() {
        let checked = check("Jeg forstår — tak.", "  ");
        assert!(!checked.correct);
        assert_eq!(checked.words.len(), 3, "the dash is not a word");
        assert!(checked.words.iter().all(|w| w.status == "missing"));
    }

    #[test]
    fn one_word_off_a_longer_sentence_is_nearly() {
        assert!(nearly(&check("Jeg vil gerne have kaffe.", "jeg vil gerne have kafe").words), "one word wrong");
        assert!(nearly(&check("Jeg vil gerne have kaffe.", "jeg vil have kaffe").words), "one word left out");
        assert!(!nearly(&check("Jeg løber.", "jeg ikke løber").words), "too short to be nearly");
        assert!(!nearly(&check("Jeg vil gerne have kaffe.", "jeg vil gerne drikke te").words), "two off");
        assert!(!nearly(&check("Jeg vil gerne have kaffe.", "jeg vil gerne have kaffe").words), "all right is not nearly");
    }
}

//! What the words of a song mean, for the music player's transcript: tap a
//! word to see its meaning, and Sing along, where the words to fill in are
//! the lesson's own first.
//!
//! The meanings come from what the app already knows — the words of the
//! course the song is filed in (its listening lesson's), the words the song
//! was written from (its origin: "Danish = English"), and every word the
//! learner has — matched to the sung forms. Danish words take endings
//! ("hunden" is "hund", "katte" is "kat"), so a sung form also matches a
//! word it is with a Danish ending (see [`same_word`]); the screen then
//! shows the dictionary form with the meaning. Nothing is asked of a model
//! and nothing is written: a word with no meaning offers the tutor instead.

use std::collections::HashMap;

use diesel::prelude::*;
use serde::Serialize;
use tauri::State;

use crate::db::{Conn, Db};
use crate::error::Result;
use crate::schema::{course_words, words};
use crate::session::split_punctuation;
#[allow(unused_imports)]
use crate::ids::*;

/// A sung word's meaning.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlossWord {
    /// The word as sung, lowercased, without punctuation: what the screen
    /// looks a word up by.
    pub form: String,
    /// Its dictionary form, as the learner's words have it.
    pub da: String,
    pub en: String,
    /// The learner's word, when it is one (to talk about it with the tutor).
    pub word_id: Option<WordId>,
    /// One of the words of the course the song is filed in: Sing along
    /// blanks these first.
    pub lesson: bool,
}

/// Lowercased, without the punctuation around it.
pub fn form_of(token: &str) -> String {
    split_punctuation(token).1.to_lowercase()
}

/// A dictionary form without the little word it is listed with: "at" before
/// a verb ("at hedde"), "en"/"et" before a noun ("en kat", "et hus"). The
/// word itself is what is sung, written and heard.
pub fn bare(word: &str) -> &str {
    let word = word.trim();
    for article in ["at ", "en ", "et ", "At ", "En ", "Et "] {
        if let Some(rest) = word.strip_prefix(article) {
            let rest = rest.trim_start();
            if !rest.is_empty() {
                return rest;
            }
        }
    }
    word
}

/// Endings a Danish word takes after its dictionary form: the definite
/// -en/-et, the plural -e/-er/-r, -ene/-erne/-ne, the genitive -s, the
/// adjective's -e/-t, the verb's -r/-ede/-t/-ende, and the comparative and
/// superlative.
const ENDINGS: &[&str] = &[
    "e", "n", "r", "s", "t", "en", "et", "er", "es", "ne", "ene", "ens", "ets", "ers", "erne", "rne", "ede", "ende", "ere", "est", "este",
];

/// After a final e dropped: "spise" → "spiste", "have" → "havde",
/// "lave" → "lavede".
const AFTER_E: &[&str] = &["te", "de", "t", "ede"];

/// Whether a sung form is a form of a dictionary word: the same, or the
/// word with a Danish ending ("hunden" / "hund", "kaffen" / "kaffe",
/// "store" / "stor"), with its final consonant doubled ("katte" / "kat",
/// "koppen" / "kop") or its final e dropped ("spiste" / "spise"). Changed
/// stems ("børn" / "barn", "bøger" / "bog") are not guessed: those are for
/// the tutor. A one-letter ending needs a word of three letters, so "ost"
/// is not "os" with a -t.
pub fn same_word(form: &str, base: &str) -> bool {
    let base = base.to_lowercase();
    if form == base {
        return true;
    }
    let letters = base.chars().count();
    if letters < 2 {
        return false;
    }
    if let Some(rest) = form.strip_prefix(base.as_str()) {
        if ENDINGS.contains(&rest) && (rest.chars().count() > 1 || letters >= 3) {
            return true;
        }
    }
    // A stem that changes at its end: a doubled consonant, or an e dropped.
    if let Some(last) = base.chars().last().filter(|c| !"aeiouyæøå".contains(*c)) {
        if let Some(rest) = form.strip_prefix(format!("{base}{last}").as_str()) {
            if rest.starts_with('e') && ENDINGS.contains(&rest) {
                return true;
            }
        }
    }
    if let Some(stem) = base.strip_suffix('e').filter(|stem| stem.chars().count() >= 2) {
        if let Some(rest) = form.strip_prefix(stem) {
            return AFTER_E.contains(&rest);
        }
    }
    false
}

/// One candidate meaning: (Danish, English, word, from the song's course).
pub(crate) type Known = (String, String, Option<WordId>, bool);

/// Finds a form's meaning: an exact match first, then a form of a word;
/// within each, in the order given (the course's words first).
pub(crate) fn look_up(form: &str, known: &[Known]) -> Option<GlossWord> {
    let single = known.iter().filter(|(da, ..)| !bare(da).contains(' '));
    let exact = single.clone().find(|(da, ..)| bare(da).to_lowercase() == form);
    let found = exact.or_else(|| single.clone().find(|(da, ..)| form.chars().count() >= 3 && same_word(form, bare(da))))?;
    Some(GlossWord { form: form.into(), da: found.0.clone(), en: found.1.clone(), word_id: found.2, lesson: found.3 })
}

/// The forms of some lines' words, each once, in order.
pub fn forms_in<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut forms: Vec<String> = Vec::new();
    for line in lines {
        for token in line.split_whitespace() {
            let form = form_of(token);
            if !form.is_empty() && !forms.contains(&form) {
                forms.push(form);
            }
        }
    }
    forms
}

/// The meanings of a song's words, for every sung form that has one.
pub fn glossary(conn: &mut Conn, song_id: SongId) -> Result<Vec<GlossWord>> {
    let song = crate::songs::load_song(conn, song_id)?;
    let sung: Vec<String> = song.sections.iter().flat_map(|s| s.lines.iter().map(|l| l.da.clone())).collect();
    let timed: Vec<String> =
        song.tracks.iter().flat_map(|t| t.lines.iter().flatten().filter(|l| l.lang == "da").map(|l| l.text.clone())).collect();
    let forms = forms_in(sung.iter().chain(&timed).map(String::as_str));

    let mut known: Vec<Known> = Vec::new();
    if let Some(course) = song.course_id {
        let rows: Vec<(WordId, String, String)> = course_words::table
            .filter(course_words::course_id.eq(course))
            .order(course_words::ord)
            .select((course_words::id, course_words::da, course_words::en))
            .load(conn)?;
        known.extend(rows.into_iter().map(|(id, da, en)| (da, en, Some(id), true)));
    }
    for given in &song.origin.given.words {
        if let Some((da, en)) = given.split_once('=') {
            known.push((da.trim().into(), en.trim().into(), None, false));
        }
    }
    meanings(conn, &forms, known)
}

/// The meanings of the given forms: from `known` first (in its order), then
/// from every word the learner has. Shared by songs and stories (stories.rs:
/// the story's own words first, marked `lesson`).
pub(crate) fn meanings(conn: &mut Conn, forms: &[String], mut known: Vec<Known>) -> Result<Vec<GlossWord>> {
    let rows: Vec<(WordId, String, String)> = words::table.select((words::id, words::da, words::en)).load(conn)?;
    known.extend(rows.into_iter().map(|(id, da, en)| (da, en, Some(id), false)));

    // A word the song was written from that the learner has: its id, so the
    // tutor can be asked about it.
    let ids: HashMap<String, WordId> =
        known.iter().filter_map(|(da, _, id, _)| Some((da.to_lowercase(), (*id)?))).collect();
    Ok(forms
        .iter()
        .filter_map(|form| look_up(form, &known))
        .map(|mut g| {
            g.word_id = g.word_id.or_else(|| ids.get(&g.da.to_lowercase()).copied());
            g
        })
        .collect())
}

#[tauri::command]
pub async fn song_glossary(db: State<'_, Db>, song_id: SongId) -> Result<Vec<GlossWord>> {
    glossary(&mut db.lock(), song_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sung_form_finds_its_dictionary_word() {
        assert!(same_word("hunden", "hund"));
        assert!(same_word("hundene", "hund"));
        assert!(same_word("katte", "kat"));
        assert!(same_word("katten", "kat"));
        assert!(same_word("kaffen", "kaffe"));
        assert!(same_word("huset", "Hus"));
        assert!(same_word("store", "stor"));
        assert!(same_word("stort", "stor"));
        assert!(same_word("spiste", "spise"));
        assert!(same_word("drikker", "drikke"));
        assert!(same_word("byen", "by"));
        assert!(same_word("århus", "Århus"));
        assert!(!same_word("hun", "hund"), "she is not a dog");
        assert!(!same_word("hund", "hun"));
        assert!(!same_word("ost", "os"));
        assert!(!same_word("børn", "barn"), "a changed stem is not guessed");
        assert!(!same_word("kat", "kage"));
    }

    #[test]
    fn exact_words_come_before_forms_and_the_course_first() {
        let known: Vec<Known> = vec![
            ("kaffe".into(), "coffee".into(), None, true),
            ("kaffebar".into(), "café".into(), None, false),
            ("kaffen".into(), "the coffee".into(), None, false),
        ];
        assert_eq!(look_up("kaffen", &known).unwrap().en, "the coffee", "an exact match wins");
        let g = look_up("kaffer", &known).unwrap();
        assert_eq!((g.da.as_str(), g.lesson), ("kaffe", true));
        assert_eq!(look_up("te", &known), None);
        // Listed with its article or "at": found by the word itself.
        let known: Vec<Known> = vec![("en hund".into(), "dog".into(), None, true), ("at spise".into(), "to eat".into(), None, false)];
        assert_eq!(look_up("hunden", &known).unwrap().da, "en hund");
        assert_eq!(look_up("spiste", &known).unwrap().en, "to eat");
        assert_eq!(bare("et hus"), "hus");
        assert_eq!(bare("en"), "en");
        assert_eq!(form_of("»Kaffen,«"), "kaffen");
    }
}

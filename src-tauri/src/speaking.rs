//! Speaking: the learner says a sentence or a word, and the app hears it.
//!
//! The webview records (getUserMedia and MediaRecorder: src/lib/mic.ts) and
//! hands the recording over as base64; a model that takes sound in writes
//! down exactly what it heard (`ai::listen_to_audio`, the model chosen in
//! Settings → AI as "Hearing you speak"), and what it heard is compared
//! with what was to be said, word by word, as a dictation is
//! (dictation.rs): case and punctuation never count, and a word heard
//! without its Danish letters is accepted. A word heard as another word is
//! marked; when the two differ by one of the sounds an English mouth runs
//! together (sounds.rs: æ/e, ø/y, y/i, the stød…) the card says which, and what
//! to try.
//!
//! Recordings live in memory only: the webview keeps the learner's own
//! takes to play back until the card is left, and this side passes the
//! bytes to the request and drops them. Nothing is stored or synced.
//!
//! Speaking cards:
//! - *Say it* (`sayIt`): a sentence or word already met, in sentence and
//!   words lessons, in place of one of their usual practice cards (so a
//!   lesson is as long as before); a review like the others.
//! - *A spoken turn* (`dialogueSay`): one of a dialogue lesson's "what comes
//!   next" lines, said instead of picked; the other speaker answers.
//! - The quick game *Say it*: five sentences met.
//!
//! They are made only on a device where speaking is on (Settings →
//! Practice → Speaking exercises, a device setting) and that has an
//! OpenRouter key to hear with. In a lesson the learner can still skip
//! speaking for the rest of the go (a noisy place): a spoken turn becomes a
//! choice again, a Say it card is left out unscored.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{self, Conn, Db};
use crate::dictation::{self, DiffWord};
use crate::error::{Error, Result};
use crate::sounds::{self, SoundTip};

/// The device setting that turns speaking cards on or off (on unless set).
pub const SETTING: &str = "speaking";

/// Whether this device makes speaking cards: speaking is on, and there is a
/// key to hear with.
pub fn available(conn: &mut Conn) -> bool {
    db::get_setting_i64(conn, SETTING, 1) == 1 && crate::ai::api_key(conn).is_ok()
}

/// The speaking cards' kinds.
pub const KINDS: &[&str] = &["sayIt", "dialogueSay"];

/// Say it cards in a words lesson, at most…
pub const WORD_CARDS: usize = 1;
/// …and in a sentences lesson.
pub const SENTENCE_CARDS: usize = 2;

/// A spoken attempt compared with what was to be said.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spoken {
    /// Every word heard (Danish letters forgiven).
    pub correct: bool,
    /// Word by word, as a dictation shows it.
    pub words: Vec<DiffWord>,
    /// Words heard as other words by a known contrast, and what to try.
    pub tips: Vec<SoundTip>,
}

/// Compares what was heard with what was to be said. A word written with
/// another spelling of the same word ("Aarhus" for "Århus", "smör" for
/// "smør") was said right: the ear wrote it its own way.
pub fn check(expected: &str, heard: &str) -> Spoken {
    let mut words = dictation::check(expected, heard).words;
    for word in &mut words {
        if word.status == "wrong" && sounds::sound_alike(&word.expected, &word.given) {
            word.status = "right".into();
        }
    }
    let mut tips: Vec<SoundTip> = Vec::new();
    for word in words.iter().filter(|w| w.status == "wrong" || w.status == "accents") {
        if let Some(tip) = sounds::misheard(&word.expected, &word.given) {
            if !tips.iter().any(|t| t.label == tip.label) {
                tips.push(tip);
            }
        }
    }
    let correct = !words.is_empty() && words.iter().all(|w| w.status == "right" || w.status == "accents");
    Spoken { correct, words, tips }
}

/// What the model is asked to reply.
#[derive(Debug, Deserialize)]
struct TranscriptReply {
    transcript: String,
}

fn transcript_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transcript": { "type": "string", "description": "Exactly what was said, word for word; empty if nothing was said." }
        },
        "required": ["transcript"],
        "additionalProperties": false,
    })
}

/// The prompt for a speaking card: Danish, verbatim, mistakes kept. The
/// sentence to be said is not given — a model told what to expect hears it.
pub const SAY_PROMPT: &str = "\
You will hear a learner of Danish saying a Danish sentence or word aloud. \
Write down exactly what you hear, word for word, in Danish spelling.\n\
- Do not correct anything: not the grammar, not the endings, not the pronunciation, not the word choice. \
If a word sounds like a different Danish word (for example \"tre\" instead of \"træ\", or \"hun\" instead of \"hund\"), write the word you heard.\n\
- Do not add words that were not said, and do not finish a sentence that was cut off.\n\
- If you hear nothing, only noise, or speech that is not Danish, the transcript is empty.\n\
Reply with JSON only: {\"transcript\": \"...\"}";

/// The prompt for the mic in a chat box: whatever language, as said.
pub const COMPOSE_PROMPT: &str = "\
You will hear a voice message to a Danish tutor from someone learning Danish. They may speak English, Danish or a mix. \
Write down exactly what was said, word for word, each word in the language it was spoken, with ordinary punctuation and capitals. \
Do not translate, correct, answer or summarise it. Leave out filler sounds (um, uh). \
If you hear nothing, the transcript is empty.\n\
Reply with JSON only: {\"transcript\": \"...\"}";

/// The transcript in the model's reply: its JSON, or — from a model that
/// answered in plain words despite being asked for JSON — the text itself.
pub fn read_transcript(text: &str) -> Result<String> {
    match crate::ai::json_in::<TranscriptReply>(text) {
        Ok(reply) => Ok(crate::text::clean(&reply.transcript)),
        Err(_) if !text.contains('{') => Ok(crate::text::clean(text.trim().trim_matches('"'))),
        Err(error) => Err(Error::msg(format!("Could not read what the model heard ({error}). Try again, or pick another model in Settings → AI."))),
    }
}

/// The recording formats a hearing model is sent (OpenRouter's names).
const FORMATS: &[&str] = &["wav", "mp3", "m4a", "aac", "ogg", "webm", "flac"];
/// More than this (base64) is no short sentence: about a minute of WAV.
// Spec: SPK-6
const MAX_AUDIO: usize = 4 * 1024 * 1024;

/// What was heard, and — for a speaking card — how it compares.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    pub text: String,
    /// Compared with the sentence to say; None when nothing was to be said
    /// (the mic in a chat box).
    pub spoken: Option<Spoken>,
}

/// Hears a recording: for a speaking card (`expected`: what was to be
/// said), or for a chat box (None: any language, as said). The recording
/// is used for this one request and dropped.
#[tauri::command]
pub async fn speech_hear(db: State<'_, Db>, audio: String, format: String, expected: Option<String>) -> Result<Heard> {
    if audio.is_empty() {
        return Err(Error::msg("Nothing was recorded. Tap the microphone and speak."));
    }
    if audio.len() > MAX_AUDIO {
        return Err(Error::msg("That recording is too long. Keep it to a sentence or two."));
    }
    if !FORMATS.contains(&format.as_str()) {
        return Err(Error::msg(format!("Can't send a recording in '{format}'.")));
    }
    let (key, model) = {
        let mut conn = db.lock();
        (crate::ai::api_key(&mut conn)?, crate::ai::hearing_model(&mut conn))
    };
    let prompt = if expected.is_some() { SAY_PROMPT } else { COMPOSE_PROMPT };
    let reply = crate::ai::listen_to_audio(&key, &model, prompt, &format, &audio, "transcript", &transcript_schema()).await?;
    drop(audio);
    let detail = if expected.is_some() { "say it" } else { "dictation into a chat" };
    let _ = crate::costs::record_tokens(&mut db.lock(), crate::costs::HEARING, &model, reply.cost, detail, reply.tokens);
    let text = read_transcript(&reply.text)?;
    let spoken = expected.map(|e| check(&e, &text));
    Ok(Heard { text, spoken })
}

/// Compares a heard attempt with what was to be said (the quick game, and
/// the preview).
#[tauri::command]
pub fn speech_check(expected: String, heard: String) -> Spoken {
    check(&expected, &heard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statuses(spoken: &Spoken) -> Vec<(&str, &str, &str)> {
        spoken.words.iter().map(|w| (w.expected.as_str(), w.given.as_str(), w.status.as_str())).collect()
    }

    #[test]
    fn said_right_is_right_whatever_the_case_and_punctuation() {
        let spoken = check("Jeg vil gerne have en kop kaffe.", "jeg vil gerne have en kop kaffe");
        assert!(spoken.correct);
        assert!(spoken.tips.is_empty());
        assert!(check("Godmorgen!", "Godmorgen.").correct);
    }

    #[test]
    fn a_word_heard_as_another_is_marked_with_the_sound_to_try() {
        let spoken = check("Der står et træ i haven.", "Der står et tre i haven");
        assert!(!spoken.correct);
        assert_eq!(statuses(&spoken)[3], ("træ", "tre", "wrong"));
        assert_eq!(spoken.tips.len(), 1);
        let tip = &spoken.tips[0];
        assert_eq!((tip.word.as_str(), tip.heard.as_str()), ("træ", "tre"));
        assert!(tip.advice.contains("open the mouth"));
        // The stød: "hund" said smoothly is "hun".
        let spoken = check("Min hund sover.", "Min hun sover");
        assert_eq!(spoken.tips[0].label, "stød or none");
    }

    #[test]
    fn a_wrong_word_that_is_no_sound_slip_gets_no_tip() {
        let spoken = check("Hvor er stationen?", "Hvor er huset");
        assert!(!spoken.correct);
        assert_eq!(statuses(&spoken)[2], ("stationen", "huset", "wrong"));
        assert!(spoken.tips.is_empty());
        // Left out: missing, and no tip.
        let spoken = check("Hvor er stationen?", "Hvor stationen");
        assert_eq!(statuses(&spoken)[1], ("er", "", "missing"));
        assert!(!spoken.correct);
    }

    #[test]
    fn the_old_spelling_of_a_is_accepted() {
        let spoken = check("Jeg bor i Århus.", "Jeg bor i Aarhus");
        assert!(spoken.correct, "aa is å");
        assert!(spoken.tips.is_empty());
    }

    #[test]
    fn spellings_of_the_same_word_are_right() {
        let spoken = check("Smør og brød.", "Smör og brød");
        assert!(spoken.correct);
        assert_eq!(statuses(&spoken)[0].2, "right");
    }

    #[test]
    fn nothing_heard_is_nothing_right() {
        let spoken = check("Tak.", "");
        assert!(!spoken.correct);
        assert!(spoken.words.iter().all(|w| w.status == "missing"));
    }

    #[test]
    fn the_reply_is_read_from_json_or_plain_words() {
        assert_eq!(read_transcript(r#"{"transcript": "Der står et tre i haven."}"#).unwrap(), "Der står et tre i haven.");
        assert_eq!(read_transcript("```json\n{\"transcript\": \"tak\"}\n```").unwrap(), "tak");
        assert_eq!(read_transcript(r#"{"transcript": ""}"#).unwrap(), "");
        assert_eq!(read_transcript("Godmorgen").unwrap(), "Godmorgen");
        assert!(read_transcript(r#"{"text": "tak"}"#).is_err(), "another shape is no transcript");
    }

    #[test]
    fn the_prompts_ask_for_verbatim_json() {
        for prompt in [SAY_PROMPT, COMPOSE_PROMPT] {
            assert!(prompt.contains("{\"transcript\""));
        }
        assert!(SAY_PROMPT.contains("Do not correct"));
        assert_eq!(transcript_schema()["required"][0], "transcript");
    }
}

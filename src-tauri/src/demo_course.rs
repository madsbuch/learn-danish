//! One small, optional starting point, with a lesson of every type. It is
//! seeded once, then belongs to the learner.
//!
//! Lessons whose pictures, recording or check-in need an OpenRouter key (or a
//! chat model) say so until one is set; then [`make_media_in_background`]
//! makes what they wait for, and they work like any other lesson.

use diesel::prelude::*;
use serde_json::json;

use crate::courses::{self, NewLine, NewSentence, NewWord};
use crate::db::{self, Conn, Db};
use crate::error::{Error, Result};
use crate::schema::courses as courses_table;

pub(crate) const SLUG: &str = "demo-first-steps";
// Spec: DEMO-2
pub(crate) const SEEDED: &str = "demo_first_steps_seeded_v1";

/// What a demo scene or story picture says until a key is set. Also how
/// [`make_media_in_background`] knows the ones to draw once there is one.
pub(crate) const NEEDS_KEY: &str = "Drawn once an OpenRouter key is set in Settings → AI.";

fn words(list: &[(&str, &str, &str)]) -> Vec<NewWord> {
    list.iter().map(|(da, en, pos)| NewWord { da: (*da).into(), en: (*en).into(), pos: (*pos).into(), ..Default::default() }).collect()
}

fn sentences(list: &[(&str, &str)]) -> Vec<NewSentence> {
    list.iter().map(|(da, en)| NewSentence { da: (*da).into(), en: (*en).into(), ..Default::default() }).collect()
}

/// A drill of one type with its own items.
fn drill(conn: &mut Conn, course: crate::ids::CourseId, title: &str, summary: &str, lesson_type: &str, items: courses::Items) -> Result<()> {
    crate::lessons::create_drill(conn, course, title, summary, lesson_type, "low", &items, None, "starter")?;
    Ok(())
}

pub fn seed_once(conn: &mut Conn) -> Result<()> {
    if db::get_setting(conn, SEEDED)?.is_some() {
        return Ok(());
    }
    conn.transaction::<_, Error, _>(|conn| {
        let exists = courses_table::table.filter(courses_table::slug.eq(SLUG)).count().get_result::<i64>(conn)? > 0;
        // An existing course with this identity is user data: never refill it.
        if !exists {
            seed(conn)?;
            rekey(conn)?;
        }
        db::set_setting(conn, SEEDED, "1")
    })
}

/// The demo: a small course on meeting people, with one lesson (or two) of
/// every registered type.
fn seed(conn: &mut Conn) -> Result<()> {
    use crate::courses::Items;
    let course = courses::ensure(
        conn, SLUG, "First steps", "De første skridt",
        "A first look at every kind of lesson: say hello, introduce yourself, and order a coffee.",
    )?;
    let dialogue = [
        ("Anna", "f", "Hej! Jeg hedder Anna.", "Hello! My name is Anna."),
        ("Alex", "m", "Hej, Anna. Jeg hedder Alex.", "Hello, Anna. My name is Alex."),
        ("Anna", "f", "Taler du dansk?", "Do you speak Danish?"),
        ("Alex", "m", "Nej, jeg taler engelsk.", "No, I speak English."),
        ("Anna", "f", "Jeg taler også engelsk.", "I also speak English."),
        ("Alex", "m", "Tak! Vi ses.", "Thanks! See you."),
    ].into_iter().map(|(speaker, gender, da, en)| NewLine {
        speaker: speaker.into(), gender: gender.into(), da: da.into(), en: en.into(),
    }).collect::<Vec<_>>();
    drill(conn, course, "Conversation: De første skridt", "Read a short dialogue, then fill in its missing lines.", "dialogue",
        Items { dialogue, ..Default::default() })?;
    drill(conn, course, "Words 1", "hej, godmorgen, farvel, …", "words", Items { words: words(&[
        ("hej", "hello; hi", "interj"),
        ("godmorgen", "good morning", "interj"),
        ("farvel", "goodbye", "interj"),
        ("tak", "thank you", "interj"),
        ("undskyld", "excuse me; sorry", "interj"),
        ("værsgo", "here you are", "interj"),
    ]), ..Default::default() })?;
    drill(conn, course, "Words 2 · typing", "ja, nej, jeg, …", "typing", Items { words: words(&[
        ("ja", "yes", "interj"),
        ("nej", "no", "interj"),
        ("jeg", "I", "pron"),
        ("du", "you", "pron"),
    ]), ..Default::default() })?;
    drill(conn, course, "Memory: languages", "dansk, engelsk, at tale, …", "memory", Items { words: words(&[
        ("dansk", "Danish (language)", "n"),
        ("engelsk", "English (language)", "n"),
        ("at tale", "to speak", "v"),
        ("at forstå", "to understand", "v"),
        ("at hedde", "to be called", "v"),
    ]), ..Default::default() })?;
    drill(conn, course, "Pictures: at the café", "kaffe, te, vand, …", "pictures", Items { words: words(&[
        ("kaffe", "coffee", "n-common"),
        ("te", "tea", "n-common"),
        ("vand", "water", "n-neuter"),
        ("brød", "bread", "n-neuter"),
        ("æble", "apple", "n-neuter"),
        ("ost", "cheese", "n-common"),
    ]), ..Default::default() })?;
    drill(conn, course, "Sentences 1", "Hej, jeg hedder Anna.", "sentences", Items { sentences: sentences(&[
        ("Hej, jeg hedder Anna.", "Hello, my name is Anna."),
        ("Hvad hedder du?", "What is your name?"),
        ("Jeg taler engelsk.", "I speak English."),
        ("Jeg forstår ikke.", "I do not understand."),
    ]), ..Default::default() })?;
    drill(conn, course, "Gaps: about me", "Jeg hedder Adam.", "gaps", Items { sentences: sentences(&[
        ("Jeg hedder Adam.", "My name is Adam."),
        ("Jeg kommer fra England.", "I come from England."),
        ("Jeg bor i København.", "I live in Copenhagen."),
        ("Jeg lærer dansk.", "I am learning Danish."),
    ]), ..Default::default() })?;
    crate::scenes::create(conn, serde_json::from_value(json!({
        "title": "Scene: in the kitchen",
        "summary": "Find the things in a kitchen, and say where they are.",
        "scene": "A small, bright kitchen: a wooden table in the middle with a mug on it, a dog asleep under the table, a lamp hanging above it, and a chair standing by the window.",
        "things": [
            { "da": "bord", "en": "table", "pos": "n-neuter" },
            { "da": "stol", "en": "chair", "pos": "n-common" },
            { "da": "krus", "en": "mug", "pos": "n-neuter" },
            { "da": "hund", "en": "dog", "pos": "n-common" },
            { "da": "lampe", "en": "lamp", "pos": "n-common" },
            { "da": "vindue", "en": "window", "pos": "n-neuter" }
        ],
        "statements": [
            { "da": "Kruset står på bordet.", "en": "The mug is on the table.", "subject": "krus", "preposition": "på", "object": "bord", "phrase": "på bordet", "wrong": ["under bordet", "ved siden af bordet"] },
            { "da": "Hunden sover under bordet.", "en": "The dog is asleep under the table.", "subject": "hund", "preposition": "under", "object": "bord", "phrase": "under bordet", "wrong": ["på bordet", "over bordet"] },
            { "da": "Lampen hænger over bordet.", "en": "The lamp hangs above the table.", "subject": "lampe", "preposition": "over", "object": "bord", "phrase": "over bordet", "wrong": ["under bordet", "på bordet"] },
            { "da": "Stolen står ved vinduet.", "en": "The chair stands by the window.", "subject": "stol", "preposition": "ved", "object": "vindue", "phrase": "ved vinduet", "wrong": ["under vinduet", "over vinduet"] }
        ],
        "course_id": course
    }))?, "starter")?;
    crate::stories::create(conn, serde_json::from_value(json!({
        "title": "Story: coffee with Mads",
        "summary": "Anna meets a friend in a café.",
        "theme": "Anna meets her friend Mads for a coffee.",
        "setting": "A small, cosy café in a Danish town.",
        "characters": [
            { "name": "Anna", "look": "a young woman with short red hair and a green coat", "gender": "f" },
            { "name": "Mads", "look": "a tall young man with round glasses and a blue jumper", "gender": "m" }
        ],
        "panels": [
            { "picture": "Anna walks into a small café and waves to Mads, who sits at a table by the window.",
              "lines": [{ "da": "Det er en café.", "en": "This is a café.", "speaker": "" },
                        { "da": "Hej, Mads!", "en": "Hi, Mads!", "speaker": "Anna" }],
              "question": { "prompt": "Where does Anna meet Mads?", "answer": "In a café", "wrong": ["In a shop", "At home"] } },
            { "picture": "Mads smiles and holds up the menu; Anna sits down opposite him.",
              "lines": [{ "da": "Hej! Vil du have en kop kaffe?", "en": "Hi! Would you like a cup of coffee?", "speaker": "Mads" },
                        { "da": "Ja tak.", "en": "Yes, please.", "speaker": "Anna", "practise": true }],
              "question": { "prompt": "What does Mads offer?", "answer": "A cup of coffee", "wrong": ["A cup of tea", "A glass of water"] } },
            { "picture": "A waiter brings two cups of coffee to their table.",
              "lines": [{ "da": "Tjeneren kommer med to kopper kaffe.", "en": "The waiter brings two cups of coffee.", "speaker": "" },
                        { "da": "Tak!", "en": "Thank you!", "speaker": "Anna" }],
              "question": { "prompt": "How many cups does the waiter bring?", "answer": "Two", "wrong": ["One", "Three"] } },
            { "picture": "Anna and Mads drink their coffee and laugh together.",
              "lines": [{ "da": "Kaffen er god.", "en": "The coffee is good.", "speaker": "Mads", "practise": true }],
              "question": { "prompt": "What does Mads say about the coffee?", "answer": "It is good", "wrong": ["It is cold", "It is too sweet"] } }
        ],
        "words": [
            { "da": "café", "en": "café", "pos": "n-common" },
            { "da": "kop", "en": "cup", "pos": "n-common" },
            { "da": "kaffe", "en": "coffee", "pos": "n-common" },
            { "da": "tjener", "en": "waiter", "pos": "n-common" }
        ],
        "course_id": course
    }))?, "starter")?;
    crate::lessons::create_study(conn, serde_json::from_value(json!({
        "title": "At være — to be",
        "summary": "The present and past of at være.",
        "complexity": "low",
        "content": "**At være** means *to be*. A Danish verb keeps one form for every person, so the present is always **er**:\n\n- jeg **er** — I am\n- du **er** — you are\n- han, hun, den, det **er** — he, she, it is\n- vi **er** — we are\n- I **er** — you (all) are\n- de **er** — they are\n\nThe past is **var**: *Jeg var træt.* — I was tired. *Kaffen er god.* — The coffee is good.",
        "objectives": ["Say who you are with jeg er", "Use er for every person, and var for the past"],
        "course_id": course
    }))?, "starter")?;
    // A short episode to listen to: written here, recorded once there is a
    // key (make_media_in_background).
    let draft: crate::songs::SongDraft = serde_json::from_value(json!({
        "title": "De første skridt",
        "chorusDa": "Hej! Jeg hedder Anna.",
        "chorusEn": "Hello! My name is Anna.",
        "style": "pairs",
        "vocalGender": "f",
        "kind": "podcast",
        "courseId": course,
        "origin": { "brief": "A first listen: Anna introduces herself." },
        "sections": [{ "kind": "verse", "lines": [
            { "en": "Hello! My name is Anna.", "da": "Hej! Jeg hedder Anna." },
            { "en": "I am from Aarhus.", "da": "Jeg er fra Aarhus." },
            { "en": "I speak Danish and a little English.", "da": "Jeg taler dansk og lidt engelsk." },
            { "en": "And you? What is your name?", "da": "Og du? Hvad hedder du?" },
            { "en": "Nice to meet you.", "da": "Hyggeligt at møde dig." },
            { "en": "Goodbye!", "da": "Farvel!" }
        ] }]
    }))?;
    let draft = crate::podcasts::normalize_episode(draft)?;
    let song = crate::ids::SongId::derived("song", &format!("{SLUG}/listening"));
    // An episode from a demo seeded before (and deleted since) stays: made,
    // it is the learner's; still a script, it is written again.
    use crate::schema::songs;
    let before: Option<String> = songs::table.find(song).select(songs::status).first(conn).optional()?;
    if before.as_deref() == Some("draft") {
        diesel::delete(songs::table.find(song)).execute(conn)?;
    }
    if before.is_none() || before.as_deref() == Some("draft") {
        let made = crate::songs::insert_draft(conn, &draft)?;
        diesel::update(songs::table.find(made)).set(songs::id.eq(song)).execute(conn)?;
    }
    crate::lessons::create_listening(conn, song, Some("low"), None, Some(course), "starter")?;
    crate::lessons::create_writing(conn, serde_json::from_value(json!({
        "title": "Meet your neighbour",
        "summary": "Say hello to a new neighbour and introduce yourself.",
        "complexity": "low",
        "content": "You meet your new neighbour, Mette, on the stairs of your building. She speaks only Danish. Say hello and introduce yourself.",
        "objectives": ["Say hello", "Say your name", "Say which language you speak"],
        "course_id": course
    }))?, "starter")?;
    // Pictures wait for a key: they say so (DEMO-10).
    use crate::schema::{scenes, story_panels};
    let lessons: Vec<crate::ids::LessonId> = crate::schema::course_lessons::table
        .filter(crate::schema::course_lessons::course_id.eq(course))
        .select(crate::schema::course_lessons::lesson_id)
        .load(conn)?;
    diesel::update(scenes::table.filter(scenes::lesson_id.eq_any(&lessons)))
        .set((scenes::status.eq(crate::scenes::FAILED), scenes::error.eq(NEEDS_KEY)))
        .execute(conn)?;
    diesel::update(story_panels::table.filter(story_panels::lesson_id.eq_any(&lessons)))
        .set((story_panels::status.eq(crate::stories::FAILED), story_panels::error.eq(NEEDS_KEY)))
        .execute(conn)?;
    courses::arrange_path(conn, course)
}

/// Makes what the demo's lessons wait for, once there is an OpenRouter key:
/// its scene and story pictures, its picture words and its episode. Only rows
/// this device wrote (`sync::written_here`): a demo from another device is
/// that device's to finish, or both would pay. Run at startup and when a key
/// is saved; without a key it does nothing.
pub fn make_media_in_background(app: &tauri::AppHandle) {
    use tauri::Manager;
    let db = app.state::<Db>();
    let found = waiting(&mut db.lock());
    let Waiting { scenes, stories, pictures, drafts } = match found {
        Ok(found) => found,
        Err(error) => {
            crate::diagnostics::note("background", format!("The demo's pictures and episode could not be started: {error}"));
            return;
        }
    };
    for lesson in scenes {
        crate::scenes::make_in_background(app, lesson, false);
    }
    for lesson in stories {
        crate::stories::draw_in_background(app, lesson);
    }
    for (lesson, words) in pictures {
        crate::pictures::draw_words_in_background(app, lesson, words);
    }
    for song in drafts {
        if let Err(error) = crate::songs::sing(app, &db, song) {
            crate::diagnostics::note("background", format!("The demo's episode could not be started: {error}"));
        }
    }
}

/// What the demo waits for a key to make, set going: scenes back to being
/// drawn, story panels back to waiting.
#[derive(Debug, Default)]
struct Waiting {
    scenes: Vec<crate::ids::LessonId>,
    stories: Vec<crate::ids::LessonId>,
    pictures: Vec<(crate::ids::LessonId, Vec<crate::ids::WordId>)>,
    drafts: Vec<crate::ids::SongId>,
}

fn waiting(conn: &mut Conn) -> Result<Waiting> {
    use crate::ids::{LessonId, SongId};
    use crate::schema::{course_lessons, lessons, scenes, songs, story_panels};
    conn.transaction::<_, Error, _>(|conn| {
        if crate::ai::api_key(conn).is_err() {
            return Ok(Default::default());
        }
        let Some(course) = courses_table::table.filter(courses_table::slug.eq(SLUG)).select(courses_table::id).first::<crate::ids::CourseId>(conn).optional()? else {
            return Ok(Default::default());
        };
        let listed: Vec<(LessonId, String, Option<SongId>)> = course_lessons::table
            .inner_join(lessons::table)
            .filter(course_lessons::course_id.eq(course))
            .filter(lessons::origin.eq("starter"))
            .select((lessons::id, lessons::lesson_type, lessons::song_id))
            .load(conn)?;
        let ids: Vec<LessonId> = listed.iter().map(|l| l.0).collect();
        let waiting_scenes: Vec<LessonId> = scenes::table
            .filter(scenes::lesson_id.eq_any(&ids))
            .filter(scenes::status.eq(crate::scenes::FAILED))
            .filter(scenes::error.eq(NEEDS_KEY))
            .select(scenes::lesson_id)
            .load::<LessonId>(conn)?
            .into_iter()
            .filter(|id| crate::sync::written_here(conn, "scenes", &id.to_string()))
            .collect();
        diesel::update(scenes::table.filter(scenes::lesson_id.eq_any(&waiting_scenes)))
            .set((scenes::status.eq(crate::scenes::DRAWING), scenes::error.eq("")))
            .execute(conn)?;
        let panels: Vec<(crate::ids::PanelId, LessonId)> = story_panels::table
            .filter(story_panels::lesson_id.eq_any(&ids))
            .filter(story_panels::status.eq(crate::stories::FAILED))
            .filter(story_panels::error.eq(NEEDS_KEY))
            .select((story_panels::id, story_panels::lesson_id))
            .load::<(crate::ids::PanelId, LessonId)>(conn)?
            .into_iter()
            .filter(|(id, _)| crate::sync::written_here(conn, "story_panels", &id.to_string()))
            .collect();
        let mut stories: Vec<LessonId> = Vec::new();
        for (panel, lesson) in panels {
            crate::stories::redraw(conn, lesson, Some(panel))?;
            if !stories.contains(&lesson) {
                stories.push(lesson);
            }
        }
        let mut pictures = Vec::new();
        for (id, _, _) in listed.iter().filter(|l| l.1 == "pictures") {
            if crate::sync::written_here(conn, "lessons", &id.to_string()) {
                let missing = crate::pictures::missing(conn, *id)?;
                if !missing.is_empty() {
                    pictures.push((*id, missing));
                }
            }
        }
        let wanted: Vec<SongId> = listed.iter().filter_map(|l| l.2).collect();
        let drafts: Vec<SongId> = songs::table
            .filter(songs::id.eq_any(&wanted))
            .filter(songs::status.eq("draft"))
            .select(songs::id)
            .load::<SongId>(conn)?
            .into_iter()
            .filter(|id| crate::sync::written_here(conn, "songs", &id.to_string()))
            .collect();
        Ok(Waiting { scenes: waiting_scenes, stories, pictures, drafts })
    })
}

/// Gives the demo course's lessons, words, sentences and lines ids derived
/// from the course and their places in it, so two devices that install it
/// agree on every row (DATA.md). Run after seeding, and once when a database
/// is converted to tagged ids; a no-op when the ids are derived already.
/// References follow each id; foreign keys are checked when the transaction
/// commits.
pub fn rekey(conn: &mut Conn) -> Result<()> {
    use crate::ids::*;
    use crate::schema::{
        checkins, conversation_lines, course_lessons, lessons, reviews, scene_spots, scenes, sentences, settings, songs, stories,
        story_panels, tutor_messages, wiki_sources, word_examples, words, xp_log,
    };
    use diesel::connection::SimpleConnection;
    conn.batch_execute("PRAGMA defer_foreign_keys = ON")?;
    // Scenes and stories came later (versions 11 and 12): a database being
    // converted from before has neither.
    let pictured = crate::db::table_exists(conn, "story_panels")?;
    let Some(course) = courses_table::table
        .filter(courses_table::slug.eq(SLUG))
        .select(courses_table::id)
        .first::<CourseId>(conn)
        .optional()?
    else {
        return Ok(());
    };
    // Only the course's own built-in lessons: ones the learner or the tutor
    // added to it are their own, with ids of their own. The order is fixed
    // (place, then type and title for ties), so every device numbers them
    // alike.
    let listed: Vec<LessonId> = course_lessons::table
        .inner_join(lessons::table)
        .filter(course_lessons::course_id.eq(course))
        .filter(lessons::origin.eq("starter"))
        .order((course_lessons::ord, lessons::lesson_type, lessons::title))
        .select(course_lessons::lesson_id)
        .load(conn)?;
    for (place, old) in listed.into_iter().enumerate() {
        let key = format!("{SLUG}/{place}");
        let new = LessonId::derived("lesson", &key);
        if new != old {
            diesel::update(lessons::table.find(old)).set(lessons::id.eq(new)).execute(conn)?;
            diesel::update(course_lessons::table.filter(course_lessons::lesson_id.eq(old))).set(course_lessons::lesson_id.eq(new)).execute(conn)?;
            diesel::update(words::table.filter(words::lesson_id.eq(old))).set(words::lesson_id.eq(new)).execute(conn)?;
            diesel::update(sentences::table.filter(sentences::lesson_id.eq(old))).set(sentences::lesson_id.eq(new)).execute(conn)?;
            diesel::update(conversation_lines::table.filter(conversation_lines::lesson_id.eq(old)))
                .set(conversation_lines::lesson_id.eq(new))
                .execute(conn)?;
            diesel::update(checkins::table.filter(checkins::lesson_id.eq(old))).set(checkins::lesson_id.eq(new)).execute(conn)?;
            diesel::update(wiki_sources::table.filter(wiki_sources::lesson_id.eq(old))).set(wiki_sources::lesson_id.eq(new)).execute(conn)?;
            diesel::update(xp_log::table.filter(xp_log::lesson_id.eq(old))).set(xp_log::lesson_id.eq(new)).execute(conn)?;
            // A scene's or a story's own rows, keyed by their lesson.
            if pictured {
                diesel::update(scenes::table.filter(scenes::lesson_id.eq(old))).set(scenes::lesson_id.eq(new)).execute(conn)?;
                diesel::update(scene_spots::table.filter(scene_spots::lesson_id.eq(old))).set(scene_spots::lesson_id.eq(new)).execute(conn)?;
                diesel::update(stories::table.filter(stories::lesson_id.eq(old))).set(stories::lesson_id.eq(new)).execute(conn)?;
                diesel::update(story_panels::table.filter(story_panels::lesson_id.eq(old))).set(story_panels::lesson_id.eq(new)).execute(conn)?;
            }
            // Ids inside stored JSON (the day's list, the tutor's buttons, a
            // song's origin): each id is a unique string, so a plain text
            // replace finds exactly its uses.
            diesel::update(settings::table.filter(settings::key.eq("daily_ids")))
                .set(settings::value.eq(diesel::dsl::sql::<diesel::sql_types::Text>(&format!("replace(value, '{old}', '{new}')"))))
                .execute(conn)?;
            diesel::update(tutor_messages::table.filter(tutor_messages::actions.like(format!("%{old}%"))))
                .set(tutor_messages::actions.eq(diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::Text>>(&format!(
                    "replace(actions, '{old}', '{new}')"
                ))))
                .execute(conn)?;
            diesel::update(songs::table.filter(songs::origin.like(format!("%{old}%"))))
                .set(songs::origin.eq(diesel::dsl::sql::<diesel::sql_types::Text>(&format!("replace(origin, '{old}', '{new}')"))))
                .execute(conn)?;
        }
        let owned: Vec<WordId> = words::table.filter(words::lesson_id.eq(new)).order((words::ord, words::id)).select(words::id).load(conn)?;
        for (n, old) in owned.into_iter().enumerate() {
            let fresh = WordId::derived("word", &format!("{key}/{n}"));
            if fresh != old {
                diesel::update(words::table.find(old)).set(words::id.eq(fresh)).execute(conn)?;
                diesel::update(reviews::table.filter(reviews::kind.eq("word")).filter(reviews::item_id.eq(old.to_string())))
                    .set(reviews::item_id.eq(fresh.to_string()))
                    .execute(conn)?;
                diesel::update(word_examples::table.filter(word_examples::word_id.eq(old))).set(word_examples::word_id.eq(fresh)).execute(conn)?;
                if pictured {
                    diesel::update(scene_spots::table.filter(scene_spots::word_id.eq(old))).set(scene_spots::word_id.eq(fresh)).execute(conn)?;
                    // A scene's "Where is it?" sentences name their things by id.
                    diesel::update(scenes::table.filter(scenes::lesson_id.eq(new)))
                        .set(scenes::statements.eq(diesel::dsl::sql::<diesel::sql_types::Text>(&format!("replace(statements, '{old}', '{fresh}')"))))
                        .execute(conn)?;
                }
            }
        }
        let owned: Vec<SentenceId> =
            sentences::table.filter(sentences::lesson_id.eq(new)).order((sentences::ord, sentences::id)).select(sentences::id).load(conn)?;
        for (n, old) in owned.into_iter().enumerate() {
            let fresh = SentenceId::derived("sentence", &format!("{key}/{n}"));
            if fresh != old {
                diesel::update(sentences::table.find(old)).set(sentences::id.eq(fresh)).execute(conn)?;
                diesel::update(reviews::table.filter(reviews::kind.eq("sentence")).filter(reviews::item_id.eq(old.to_string())))
                    .set(reviews::item_id.eq(fresh.to_string()))
                    .execute(conn)?;
            }
        }
        let owned: Vec<LineId> = conversation_lines::table
            .filter(conversation_lines::lesson_id.eq(new))
            .order((conversation_lines::ord, conversation_lines::id))
            .select(conversation_lines::id)
            .load(conn)?;
        for (n, old) in owned.into_iter().enumerate() {
            let fresh = LineId::derived("line", &format!("{key}/{n}"));
            if fresh != old {
                diesel::update(conversation_lines::table.find(old)).set(conversation_lines::id.eq(fresh)).execute(conn)?;
            }
        }
        if pictured {
            let panels: Vec<PanelId> =
                story_panels::table.filter(story_panels::lesson_id.eq(new)).order((story_panels::ord, story_panels::id)).select(story_panels::id).load(conn)?;
            for (n, old) in panels.into_iter().enumerate() {
                let fresh = PanelId::derived("panel", &format!("{key}/{n}"));
                if fresh != old {
                    diesel::update(story_panels::table.find(old)).set(story_panels::id.eq(fresh)).execute(conn)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{lessons, conversation_lines, sentences, words};

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("danish-demo-{name}-{}.db", std::process::id()))
    }

    fn fresh(name: &str) -> Conn {
        let file = path(name);
        let _ = std::fs::remove_file(&file);
        db::open(&file).unwrap()
    }

    #[test]
    fn a_fresh_install_has_one_demo_with_a_lesson_of_every_type() {
        use crate::schema::{scenes, songs, story_panels};
        let mut conn = fresh("fresh");
        assert_eq!(courses_table::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
        let (id, slug): (crate::ids::CourseId, String) = courses_table::table.select((courses_table::id, courses_table::slug)).first(&mut conn).unwrap();
        assert_eq!(slug, SLUG);
        let types: Vec<String> = lessons::table.select(lessons::lesson_type).load(&mut conn).unwrap();
        for kind in crate::lesson_types::REGISTRY {
            assert!(types.iter().any(|t| t == kind.id), "the demo has no {} lesson", kind.id);
        }
        assert_eq!(types.len(), crate::lesson_types::REGISTRY.len());
        // Every lesson is the demo's, in one course, with a derived id.
        for (place, lesson) in crate::lessons::for_course(&mut conn, id).unwrap().iter().enumerate() {
            assert_eq!(lesson.id, crate::ids::LessonId::derived("lesson", &format!("{SLUG}/{place}")), "{}", lesson.title);
            assert_eq!(lesson.course_ids, vec![id]);
        }
        // The path: the conversation first, writing last.
        let route = crate::lessons::for_course(&mut conn, id).unwrap();
        assert_eq!(route[0].lesson_type, "dialogue");
        assert_eq!(route.last().unwrap().lesson_type, "writing");
        // What needs a key says so, and waits for one.
        let scene: (String, String) = scenes::table.select((scenes::status, scenes::error)).first(&mut conn).unwrap();
        assert_eq!(scene, (crate::scenes::FAILED.to_string(), NEEDS_KEY.to_string()));
        let panels: Vec<String> = story_panels::table.select(story_panels::error).load(&mut conn).unwrap();
        assert!(!panels.is_empty() && panels.iter().all(|e| e == NEEDS_KEY));
        let song = crate::ids::SongId::derived("song", &format!("{SLUG}/listening"));
        let (status, kind): (String, String) = songs::table.find(song).select((songs::status, songs::kind)).first(&mut conn).unwrap();
        assert_eq!((status.as_str(), kind.as_str()), ("draft", "podcast"));
        let listening = route.iter().find(|l| l.lesson_type == "listening").unwrap();
        assert_eq!(lessons::table.find(listening.id).select(lessons::song_id).first::<Option<crate::ids::SongId>>(&mut conn).unwrap(), Some(song));
        // The scene's sentences name its things by their derived ids.
        let statements: String = scenes::table.select(scenes::statements).first(&mut conn).unwrap();
        let things: Vec<crate::ids::WordId> = words::table.filter(words::lesson_id.eq_any(route.iter().filter(|l| l.lesson_type == "scene").map(|l| l.id))).select(words::id).load(&mut conn).unwrap();
        assert!(things.iter().any(|w| statements.contains(&w.to_string())), "{statements}");
        assert!(crate::scenes::card(&mut conn, route.iter().find(|l| l.lesson_type == "scene").unwrap().id).is_ok());

        assert!(!crate::lessons::overview(&mut conn).unwrap().due.is_empty(), "today offers the demo's first lessons");
        crate::stats::collect(&mut conn).unwrap();
        let counts = |conn: &mut Conn| -> (i64, i64, i64) {
            (
                words::table.count().get_result(conn).unwrap(),
                sentences::table.count().get_result(conn).unwrap(),
                conversation_lines::table.count().get_result(conn).unwrap(),
            )
        };
        let before = counts(&mut conn);
        assert_eq!(before.2, 6);
        // Reopening neither duplicates the demo nor overwrites a learner's rename.
        diesel::update(courses_table::table.find(id)).set(courses_table::title_en.eq("My first Danish")).execute(&mut conn).unwrap();
        drop(conn);
        let mut conn = db::open(&path("fresh")).unwrap();
        assert_eq!(courses_table::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
        assert_eq!(counts(&mut conn), before);
        assert_eq!(courses_table::table.find(id).select(courses_table::title_en).first::<String>(&mut conn).unwrap(), "My first Danish");
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
    }

    #[test]
    fn the_demo_waits_for_a_key_then_makes_what_it_needs() {
        use crate::schema::{scenes, story_panels};
        let mut conn = fresh("key");
        // No key: nothing is set going, and the lessons keep saying so.
        let none = waiting(&mut conn).unwrap();
        assert!(none.scenes.is_empty() && none.stories.is_empty() && none.pictures.is_empty() && none.drafts.is_empty());
        db::set_setting(&mut conn, "openrouter_api_key", "sk-test").unwrap();
        let set = waiting(&mut conn).unwrap();
        assert_eq!((set.scenes.len(), set.stories.len(), set.pictures.len(), set.drafts.len()), (1, 1, 1, 1));
        assert_eq!(set.pictures[0].1.len(), 6, "every picture word is drawn");
        assert_eq!(scenes::table.select(scenes::status).first::<String>(&mut conn).unwrap(), crate::scenes::DRAWING);
        let panels: Vec<String> = story_panels::table.select(story_panels::status).load(&mut conn).unwrap();
        assert!(panels.iter().all(|s| s == crate::stories::WAITING), "{panels:?}");
        // Asked again, the scene and the story are under way already.
        let again = waiting(&mut conn).unwrap();
        assert!(again.scenes.is_empty() && again.stories.is_empty());
    }

    // Spec: DEMO-2
    #[test]
    fn deleting_the_demo_is_respected_after_reopen() {
        let mut conn = fresh("deleted");
        let id: crate::ids::CourseId = courses_table::table.select(courses_table::id).first(&mut conn).unwrap();
        courses::delete(&mut conn, id).unwrap();
        drop(conn);
        let mut conn = db::open(&path("deleted")).unwrap();
        assert_eq!(courses_table::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(lessons::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(words::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
    }
}

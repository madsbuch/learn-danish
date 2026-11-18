//! Retire the old bundled courses without treating a learner's edits as defaults.
//!
//! Only definitions that match a frozen fingerprint of a shipped course are
//! removed. Practice progress is deliberately absent from that fingerprint;
//! changed content, added lessons, shared lessons, songs and sources are kept.

use diesel::prelude::*;
use sha2::{Digest, Sha256};

use crate::db::{self, Conn};
use crate::error::{Error, Result};
use crate::schema::{lessons, conversation_lines, course_lessons, sentences, songs, courses, wiki_sources, words};
#[allow(unused_imports)]
use crate::ids::*;

pub(crate) const RETIRED: &str = "bundled_courses_retired_v1";

// SHA-256 of the definitions shipped before default courses were removed.
// These hashes contain no vocabulary or lesson content and cannot seed courses.
const LEGACY_COURSES: &[(&str, &str)] = &[
    ("basics", "523bcf4fe9b8c8ad8ca358e4530a5351963091669e4df3ae5476254195032402"),
    ("family", "5054bd89520555893759d77710e9103ed4388e2acbd1ba2b1390154399a57dfd"),
    ("food", "1ee538c699546ed6991b329387ec05c8002dd5e0f60e3e25248d1e10380b6b35"),
    ("shopping", "d3220f4dec3f7145c9874b4ec32c4d0f1f4d6c452436e52202c60a6576840695"),
    ("travel", "5f84e007807f61fc0df21d6e383cdd02c1b124ed455f4df45a96ba05e2a44312"),
    ("home", "0c1d82702a1e46fc26d2b0b781e78d13bac10bc56dbba0c583851655298e4b5c"),
    ("work", "f96d5c1cc09175b339dbbeb3eee5f113c6c07fcaf3a154308bf52952de69f689"),
    ("health", "d84b9a0c84e95b18cc79fb3cb8acd66a9f2e7209efe4ebcf24832519e08be38f"),
    ("time-nature", "ae557a424d216ec87d34724c6cd10b38f3ccc11add02a3da4b1d3091fd1c9ef5"),
    ("leisure", "788a497f8566de6c684da1ccbeb44af253f9d7118ddb3c10a2ef3978ae8c39c1"),
    ("grammar-essentials", "9c3d644068fc65fcc07fe46a59c3bcb4b6e4761bc396ca8b4d7a0f93260d1208"),
];

/// Hash the complete editable definition, without database IDs or review state.
fn fingerprint(conn: &mut Conn, course_id: CourseId) -> Result<Option<String>> {
    let course: (String, String, String, String, i64, Option<FileId>) = courses::table
        .find(course_id)
        .select((courses::slug, courses::title_da, courses::title_en, courses::summary_en, courses::created_at, courses::image_file_id))
        .first(conn)?;
    if course.4 != 0 || course.5.is_some() {
        return Ok(None);
    }
    if songs::table.filter(songs::course_id.eq(course_id)).count().get_result::<i64>(conn)? > 0 {
        return Ok(None);
    }
    let mut definition = vec![serde_json::to_value((&course.0, &course.1, &course.2, &course.3))?];
    let listed: Vec<(LessonId, i64)> = course_lessons::table
        .filter(course_lessons::course_id.eq(course_id))
        .order((course_lessons::ord, course_lessons::lesson_id))
        .select((course_lessons::lesson_id, course_lessons::ord))
        .load(conn)?;
    for (lesson_id, ord) in listed {
        if course_lessons::table.filter(course_lessons::lesson_id.eq(lesson_id)).count().get_result::<i64>(conn)? != 1
            || wiki_sources::table.filter(wiki_sources::lesson_id.eq(lesson_id)).count().get_result::<i64>(conn)? > 0
        {
            return Ok(None);
        }
        let mut lesson: (Option<String>, String, String, String, String, String, String, String, i64, String, i64, Option<SongId>) = lessons::table
            .find(lesson_id)
            .select((lessons::slug, lessons::lesson_type, lessons::lesson_type, lessons::title, lessons::summary,
                lessons::complexity, lessons::content, lessons::objectives, lessons::period_days,
                lessons::origin, lessons::challenge, lessons::song_id))
            .first(conn)?;
        // The fingerprints were taken when a lesson had a kind and a format:
        // they are rebuilt from its type, so the old courses still match.
        let (kind, format) = legacy_kind_and_format(&lesson.1);
        (lesson.1, lesson.2) = (kind.to_string(), format);
        if lesson.9 != "starter" || lesson.11.is_some() {
            return Ok(None);
        }
        let vocabulary: Vec<(i64, String, String, String)> = words::table
            .filter(words::lesson_id.eq(lesson_id)).order(words::ord)
            .select((words::ord, words::da, words::en, words::pos)).load(conn)?;
        let phrases: Vec<(i64, String, String)> = sentences::table
            .filter(sentences::lesson_id.eq(lesson_id)).order(sentences::ord)
            .select((sentences::ord, sentences::da, sentences::en)).load(conn)?;
        let dialogue: Vec<(i64, String, String, String, String)> = conversation_lines::table
            .filter(conversation_lines::lesson_id.eq(lesson_id)).order(conversation_lines::ord)
            .select((conversation_lines::ord, conversation_lines::speaker, conversation_lines::gender,
                conversation_lines::da, conversation_lines::en)).load(conn)?;
        definition.push(serde_json::to_value((ord, lesson, vocabulary, phrases, dialogue))?);
    }
    Ok(Some(format!("{:x}", Sha256::digest(serde_json::to_vec(&definition)?))))
}

/// A type as the kind (drill | study) and format it was stored as before
/// lesson types were flattened.
fn legacy_kind_and_format(lesson_type: &str) -> (&'static str, String) {
    match lesson_type {
        "study" => ("study", String::new()),
        "writing" => ("study", "writing".into()),
        "listening" => ("study", "song".into()),
        drill => ("drill", drill.to_string()),
    }
}

fn retire_matching(conn: &mut Conn, manifest: &[(&str, &str)]) -> Result<()> {
    for (slug, expected) in manifest {
        let id = courses::table.filter(courses::slug.eq(slug)).select(courses::id).first::<CourseId>(conn).optional()?;
        if let Some(id) = id {
            if fingerprint(conn, id)?.as_deref() == Some(*expected) {
                crate::courses::delete(conn, id)?;
            }
        }
    }
    Ok(())
}

pub fn run(conn: &mut Conn) -> Result<()> {
    if db::get_setting(conn, RETIRED)?.is_some() {
        return Ok(());
    }
    conn.transaction::<_, Error, _>(|conn| {
        retire_matching(conn, LEGACY_COURSES)?;
        db::set_setting(conn, RETIRED, "1")?;
        // An older build must not reseed content when opening this database.
        db::set_setting(conn, "starter_seeded", "1")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lessons as practice_lessons, schema, stats};

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("danish-retirement-{name}-{}.db", std::process::id()))
    }

    fn empty(name: &str) -> Conn {
        let file = path(name);
        let _ = std::fs::remove_file(&file);
        db::open(&file).unwrap()
    }

    fn legacy(name: &str) -> Conn {
        let mut conn = empty(name);
        db::seed_test_courses(&mut conn).unwrap();
        db::unify_courses(&mut conn).unwrap();
        db::delete_setting(&mut conn, RETIRED).unwrap();
        db::delete_setting(&mut conn, crate::demo_course::SEEDED).unwrap();
        conn
    }

    fn course(conn: &mut Conn, slug: &str) -> CourseId {
        courses::table.filter(courses::slug.eq(slug)).select(courses::id).first(conn).unwrap()
    }

    #[test]
    fn released_defaults_are_retired_once_without_resetting_the_learner() {
        let mut conn = legacy("released");
        assert_eq!(courses::table.count().get_result::<i64>(&mut conn).unwrap(), crate::convert::count(LEGACY_COURSES.len()));
        for (slug, expected) in LEGACY_COURSES {
            let id = course(&mut conn, slug);
            assert_eq!(fingerprint(&mut conn, id).unwrap().as_deref(), Some(*expected), "{slug}");
        }
        let today = db::today(&mut conn);
        stats::add_xp(&mut conn, today, 42).unwrap();
        db::set_setting(&mut conn, "daily_goal", "250").unwrap();
        diesel::update(lessons::table).set((lessons::reps.eq(3), lessons::due_day.eq(today + 8), lessons::passed.eq(1))).execute(&mut conn).unwrap();
        run(&mut conn).unwrap();
        assert_eq!(courses::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(lessons::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(words::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(db::get_setting(&mut conn, "daily_goal").unwrap().as_deref(), Some("250"));
        assert_eq!(schema::day_totals::table.find(today).select(schema::day_totals::xp).first::<i64>(&mut conn).unwrap(), 42);
        drop(conn);
        let mut conn = db::open(&path("released")).unwrap();
        assert_eq!(courses::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
        assert_eq!(courses::table.select(courses::slug).first::<String>(&mut conn).unwrap(), crate::demo_course::SLUG);
        assert_eq!(words::table.count().get_result::<i64>(&mut conn).unwrap(), 12);
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
    }

    #[test]
    fn customized_extended_shared_and_user_courses_survive() {
        let mut conn = legacy("custom");
        let renamed = course(&mut conn, "basics");
        diesel::update(courses::table.find(renamed)).set(courses::title_en.eq("My Danish basics")).execute(&mut conn).unwrap();
        let extended = course(&mut conn, "family");
        crate::courses::extend(&mut conn, extended, &[crate::courses::NewWord { da: "mit eget ord".into(), en: "my own word".into(), pos: "phr".into() }], &[], &[], "you").unwrap();
        let edited = course(&mut conn, "food");
        let edited_lesson = practice_lessons::for_course(&mut conn, edited).unwrap()[0].id;
        practice_lessons::update(&mut conn, edited_lesson, practice_lessons::LessonChanges { title: Some("My dialogue".into()), ..Default::default() }).unwrap();
        let shared = course(&mut conn, "shopping");
        let shared_lesson = practice_lessons::for_course(&mut conn, shared).unwrap()[0].id;
        let own = crate::courses::create_empty(&mut conn, "My practice", "Made by me").unwrap();
        crate::courses::add_lesson(&mut conn, own, shared_lesson).unwrap();
        let changed_word = course(&mut conn, "travel");
        let word = schema::course_words::table.filter(schema::course_words::course_id.eq(changed_word)).select(schema::course_words::id).first::<WordId>(&mut conn).unwrap();
        diesel::update(words::table.find(word)).set(words::en.eq("my edited translation")).execute(&mut conn).unwrap();
        let scheduled = course(&mut conn, "home");
        let scheduled_lesson = practice_lessons::for_course(&mut conn, scheduled).unwrap()[0].id;
        practice_lessons::update(&mut conn, scheduled_lesson, practice_lessons::LessonChanges { period_days: Some(19), ..Default::default() }).unwrap();
        run(&mut conn).unwrap();
        let retained: Vec<CourseId> = courses::table.select(courses::id).load(&mut conn).unwrap();
        for id in [renamed, extended, edited, shared, changed_word, scheduled, own] {
            assert!(retained.contains(&id), "customized course {id} was lost");
        }
        assert_eq!(retained.len(), 7);
        assert_eq!(course_lessons::table.filter(course_lessons::lesson_id.eq(shared_lesson)).count().get_result::<i64>(&mut conn).unwrap(), 2);
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
        // Once retained, restoring a title later must not trigger another sweep.
        diesel::update(courses::table.find(renamed)).set(courses::title_en.eq("A new neighbour")).execute(&mut conn).unwrap();
        run(&mut conn).unwrap();
        assert_eq!(courses::table.count().get_result::<i64>(&mut conn).unwrap(), 7);
        drop(conn);
        let mut conn = db::open(&path("custom")).unwrap();
        assert_eq!(courses::table.count().get_result::<i64>(&mut conn).unwrap(), 8);
        assert_eq!(courses::table.filter(courses::slug.eq(crate::demo_course::SLUG)).count().get_result::<i64>(&mut conn).unwrap(), 1);
    }
}

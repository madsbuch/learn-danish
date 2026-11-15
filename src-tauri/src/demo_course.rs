//! One small, optional starting point. It is seeded once, then belongs to the learner.

use diesel::prelude::*;

use crate::courses::{self, NewLine, NewSentence, NewWord};
use crate::db::{self, Conn};
use crate::error::{Error, Result};
use crate::schema::topics;

pub(crate) const SLUG: &str = "demo-first-steps";
pub(crate) const SEEDED: &str = "demo_first_steps_seeded_v1";

pub fn seed_once(conn: &mut Conn) -> Result<()> {
    if db::get_setting(conn, SEEDED)?.is_some() {
        return Ok(());
    }
    conn.transaction::<_, Error, _>(|conn| {
        let exists = topics::table.filter(topics::slug.eq(SLUG)).count().get_result::<i64>(conn)? > 0;
        // An existing course with this identity is user data: never refill it.
        if !exists {
            let course = courses::ensure(
                conn, SLUG, "First steps", "De første skridt",
                "A tiny introduction: say hello, introduce yourself, and ask about language.",
            )?;
            let words = [
                ("hej", "hello; hi", "interj"),
                ("farvel", "goodbye", "interj"),
                ("tak", "thank you", "interj"),
                ("undskyld", "excuse me; sorry", "interj"),
                ("ja", "yes", "interj"),
                ("nej", "no", "interj"),
                ("jeg", "I", "pron"),
                ("du", "you", "pron"),
                ("dansk", "Danish (language)", "n"),
                ("engelsk", "English (language)", "n"),
                ("at hedde", "to be called", "v"),
                ("at forstå", "to understand", "v"),
            ].into_iter().map(|(da, en, pos)| NewWord { da: da.into(), en: en.into(), pos: pos.into() }).collect::<Vec<_>>();
            let sentences = [
                ("Hej, jeg hedder Anna.", "Hello, my name is Anna."),
                ("Hvad hedder du?", "What is your name?"),
                ("Jeg taler engelsk.", "I speak English."),
                ("Jeg forstår ikke.", "I do not understand."),
            ].into_iter().map(|(da, en)| NewSentence { da: da.into(), en: en.into() }).collect::<Vec<_>>();
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
            courses::cut_into_drills(conn, course, &words, &sentences, &dialogue, "starter")?;
        }
        db::set_setting(conn, SEEDED, "1")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{blocks, conversation_lines, sentences, words};

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("danish-demo-{name}-{}.db", std::process::id()))
    }

    fn fresh(name: &str) -> Conn {
        let file = path(name);
        let _ = std::fs::remove_file(&file);
        db::open(&file).unwrap()
    }

    #[test]
    fn a_fresh_install_has_one_small_demo_without_grammar_courses() {
        let mut conn = fresh("fresh");
        assert_eq!(topics::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
        let (id, slug): (i64, String) = topics::table.select((topics::id, topics::slug)).first(&mut conn).unwrap();
        assert_eq!(slug, SLUG);
        assert_eq!(words::table.count().get_result::<i64>(&mut conn).unwrap(), 12);
        assert_eq!(sentences::table.count().get_result::<i64>(&mut conn).unwrap(), 4);
        assert_eq!(conversation_lines::table.count().get_result::<i64>(&mut conn).unwrap(), 6);
        assert_eq!(blocks::table.count().get_result::<i64>(&mut conn).unwrap(), 4);
        assert_eq!(blocks::table.filter(blocks::kind.ne("drill")).count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert!(!crate::blocks::overview(&mut conn).unwrap().next.is_empty());
        crate::stats::collect(&mut conn).unwrap();
        // Reopening neither duplicates the demo nor overwrites a learner's rename.
        diesel::update(topics::table.find(id)).set(topics::title_en.eq("My first Danish")).execute(&mut conn).unwrap();
        drop(conn);
        let mut conn = db::open(&path("fresh")).unwrap();
        assert_eq!(topics::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
        assert_eq!(words::table.count().get_result::<i64>(&mut conn).unwrap(), 12);
        assert_eq!(topics::table.find(id).select(topics::title_en).first::<String>(&mut conn).unwrap(), "My first Danish");
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
    }

    #[test]
    fn deleting_the_demo_is_respected_after_reset_and_reopen() {
        let mut conn = fresh("deleted");
        let id: i64 = topics::table.select(topics::id).first(&mut conn).unwrap();
        courses::delete(&mut conn, id).unwrap();
        crate::commands::reset_progress_in(&mut conn).unwrap();
        drop(conn);
        let mut conn = db::open(&path("deleted")).unwrap();
        assert_eq!(topics::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(blocks::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(words::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
    }
}

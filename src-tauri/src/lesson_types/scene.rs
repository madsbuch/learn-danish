//! Scenes: one illustrated scene full of things to find. Explore it (tap a
//! thing to hear it), find a thing by its Danish name, name a thing that
//! lights up, and — when the tutor wrote some — say where things are ("Katten
//! er under bordet"). The things are the lesson's own words; the picture
//! and where each thing is in it are made in the background (crate::scenes).
//! Until it is ready, the words are practised as a plain word drill.

use rand::seq::{IndexedRandom, SliceRandom};
use rand::Rng as _;

use super::{LessonType, PathGroup, Run};
use crate::db::Conn;
use crate::error::Result;
use crate::models::Exercise;
use crate::scenes::{SceneCard, Statement};
use crate::session::{self, Built, Drill, WordRow};

pub const TYPE: LessonType = LessonType {
    id: "scene",
    name: "Scene",
    icon: "scene",
    description: "One illustrated scene — a market stall, a kitchen, a station — with its things to explore: tap a thing to hear its name, find a thing in the picture by its Danish name, name a thing that lights up, and say where things are.",
    tutor_hint: "For a place full of everyday things (create_scene_lesson): 6–10 things you can point at, nouns only, the course's own words where they fit. Add “Where is it?” sentences (under, på, over, ved siden af, ved, i) when the learner is ready for prepositions and definite forms.",
    run: Run::Exercises(build),
    path: PathGroup::Words,
    // Made with its own tutor tool (create_scene_lesson): a scene and its things.
    takes_items: false,
    unaided_to_master: false,
    sections: &["scene", "words", "where"],
    start_label: "Start",
    again_label: "Practise again",
};

/// A drill's word, and whether it is new.
type Word = (WordRow, bool);

/// "Where is it?" cards a session asks at most.
const WHERE_CARDS: usize = 3;
/// A known word is named by typing it (harder lessons) once it has come
/// back this many times.
const TYPE_AFTER: i64 = 2;

fn build(conn: &mut Conn, drill: &Drill) -> Result<Built> {
    let rng = &mut rand::rng();
    let Some((card, statements)) = crate::scenes::card(conn, drill.lesson.id)? else {
        // Still being drawn (or failed): the words, as a word drill.
        return Ok(Built { exercises: plain(drill.words.iter().collect(), drill, rng), ..Default::default() });
    };
    let (spotted, unspotted): (Vec<&Word>, Vec<&Word>) =
        drill.words.iter().partition(|(w, _)| card.spots.iter().any(|s| s.word_id == w.id));
    let mut exercises = Vec::new();
    if spotted.iter().any(|(_, is_new)| *is_new) {
        exercises.push(explore(drill, &card));
    }
    let typed = drill.lesson.complexity != "low";
    let mut finds = Vec::new();
    let mut names = Vec::new();
    for (w, is_new) in &spotted {
        if *is_new {
            finds.push(find(w, true, &card));
            names.push(name(w, false, &card, drill, rng));
        } else if w.reps % 2 == 0 {
            finds.push(find(w, false, &card));
        } else {
            names.push(name(w, typed && w.reps >= TYPE_AFTER, &card, drill, rng));
        }
    }
    finds.shuffle(rng);
    names.shuffle(rng);
    exercises.extend(finds);
    exercises.extend(names);
    exercises.extend(plain(unspotted, drill, rng));
    let mut statements: Vec<&Statement> = statements.iter().collect();
    statements.shuffle(rng);
    for (n, statement) in statements.into_iter().take(WHERE_CARDS).enumerate() {
        let truth = rng.random_bool(0.5);
        exercises.push(where_is(statement, n, truth, drill.lesson.id, &card, rng));
    }
    Ok(Built { exercises, ..Default::default() })
}

/// Words without a place in the picture: new ones met and recognised,
/// known ones practised as in a word drill.
fn plain(words: Vec<&Word>, drill: &Drill, rng: &mut impl rand::Rng) -> Vec<Exercise> {
    let mut exercises = Vec::new();
    for (w, is_new) in words {
        if *is_new {
            exercises.push(session::word_intro(w));
            exercises.push(session::recognise(w, &drill.pool, rng));
        } else {
            exercises.push(session::practise_word(w, &drill.pool, rng));
        }
    }
    exercises
}

/// The scene with the given things pointed out.
fn lit(card: &SceneCard, lit: &[crate::ids::WordId]) -> SceneCard {
    let mut card = card.clone();
    for spot in &mut card.spots {
        spot.lit = lit.contains(&spot.word_id);
    }
    card
}

/// The whole scene, to look around: tap a thing to hear its name.
fn explore(drill: &Drill, card: &SceneCard) -> Exercise {
    let id = drill.lesson.id;
    Exercise {
        key: format!("b{id}-explore"),
        kind: "sceneExplore".into(),
        item_kind: "set".into(),
        item_id: id.to_string(),
        is_new: true,
        prompt: drill.lesson.title.clone(),
        scene: Some(card.clone()),
        ..Exercise::default()
    }
}

/// "Hvor er der et ur?" — tap it in the picture. The word as the lesson has
/// it, with its article ("et ur", "en kat"), or without one for a mass noun
/// or a plural ("mælk", "æbler"): a definite form ("uret") can't be told
/// from the dictionary form (kat → katten, bil → bilen), so the question
/// asks for the thing as it is written. A name is just "Hvor er Ida?".
pub fn where_prompt(da: &str, pos: &str) -> String {
    if pos == "proper" {
        format!("Hvor er {da}?")
    } else {
        format!("Hvor er der {da}?")
    }
}

fn find(w: &WordRow, is_new: bool, card: &SceneCard) -> Exercise {
    Exercise {
        key: format!("w{}-find", w.id),
        kind: "sceneFind".into(),
        item_kind: "word".into(),
        item_id: w.id.to_string(),
        is_new,
        prompt: where_prompt(&w.da, &w.pos),
        answer: w.da.clone(),
        da: w.da.clone(),
        meaning: w.meaning.clone(),
        scene: Some(card.clone()),
        ..Exercise::default()
    }
}

/// A thing lights up: pick its name from the scene's others, or type it.
fn name(w: &WordRow, typed: bool, card: &SceneCard, drill: &Drill, rng: &mut impl rand::Rng) -> Exercise {
    let mut options = Vec::new();
    if !typed {
        let mut others: Vec<String> = card.spots.iter().filter(|s| s.da != w.da).map(|s| s.da.clone()).collect();
        others.shuffle(rng);
        others.truncate(3);
        // A small scene borrows decoys from the course.
        let mut pool: Vec<String> = drill.pool.danish.iter().filter(|p| **p != w.da && !others.contains(p)).cloned().collect();
        pool.shuffle(rng);
        others.extend(pool.into_iter().take(3 - others.len()));
        options = others;
        options.push(w.da.clone());
        options.shuffle(rng);
    }
    Exercise {
        key: format!("w{}-name", w.id),
        kind: if typed { "sceneType" } else { "sceneName" }.into(),
        item_kind: "word".into(),
        item_id: w.id.to_string(),
        is_new: false,
        prompt: "Hvad er det?".into(),
        hint: session::pretty_pos(&w.pos),
        answer: w.da.clone(),
        options,
        da: w.da.clone(),
        meaning: w.meaning.clone(),
        scene: Some(lit(card, &[w.id])),
        ..Exercise::default()
    }
}

/// The two Danish words for true and false, the options of a true-or-false card.
pub const TRUE: &str = "Sandt";
pub const FALSE: &str = "Falsk";

/// "Where is it?": a sentence about the scene, true or not (`truth`), or with
/// the place left out to choose. The two things it is about are pointed out.
fn where_is(s: &Statement, n: usize, truth: bool, lesson: crate::ids::LessonId, card: &SceneCard, rng: &mut impl rand::Rng) -> Exercise {
    let base = Exercise {
        key: format!("b{lesson}-where{n}"),
        kind: "sceneWhere".into(),
        item_kind: "set".into(),
        item_id: lesson.to_string(),
        da: s.da.clone(),
        meaning: s.en.clone(),
        scene: Some(lit(card, &[s.subject, s.object])),
        ..Exercise::default()
    };
    let wrong = s.wrong.choose(rng).cloned().unwrap_or_default();
    // Every other card asks for the place itself.
    if n % 2 == 1 && !wrong.is_empty() {
        let mut options: Vec<String> = s.wrong.clone();
        options.push(s.phrase.clone());
        options.shuffle(rng);
        return Exercise {
            prompt: s.da.replacen(&s.phrase, "…", 1),
            hint: "Which fits the picture?".into(),
            answer: s.phrase.clone(),
            options,
            ..base
        };
    }
    let shown = if truth || wrong.is_empty() { s.da.clone() } else { s.da.replacen(&s.phrase, &wrong, 1) };
    Exercise {
        prompt: shown,
        hint: "True or false? Look at the picture.".into(),
        answer: if truth || wrong.is_empty() { TRUE } else { FALSE }.into(),
        options: vec![TRUE.into(), FALSE.into()],
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_is_it_asks_in_danish() {
        assert_eq!(where_prompt("et ur", "n-neuter"), "Hvor er der et ur?");
        assert_eq!(where_prompt("mælk", "n-common"), "Hvor er der mælk?");
        assert_eq!(where_prompt("æbler", "n-pl"), "Hvor er der æbler?");
        assert_eq!(where_prompt("Ida", "proper"), "Hvor er Ida?");
    }
}

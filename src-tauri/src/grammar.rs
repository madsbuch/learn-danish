//! Grammar tags: the structures of Danish a word or sentence shows — en and
//! et, the definite ending ("-en / -et"), plurals, adjective endings, sin and
//! hans, tenses, modal verbs, verb second and ikke, prepositions, numbers… A
//! curated list kept here, like the built-in sound pairs: each tag's id is
//! derived from its key (DATA.md), the same on every device, and nothing is
//! written for the list itself.
//!
//! Words and sentences are linked to tags (`word_grammar`,
//! `sentence_grammar`). The tutor tags what it makes (`grammar` on a word or
//! sentence in its tools); older items are tagged only when the learner asks
//! it to ("Tag the grammar in your courses" on the Structures page) — never
//! filled in behind their back (PRINCIPLES.md #4).

use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::db::Conn;
use crate::error::{Error, Result};
#[allow(unused_imports)]
use crate::ids::*;
use crate::schema::{sentence_grammar, word_grammar};

/// One structure: its key (what the tutor writes), its English name, the
/// group it is listed under, and a line on what it is.
#[derive(Debug)]
pub struct Tag {
    pub key: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub explain: &'static str,
}

const fn tag(key: &'static str, name: &'static str, group: &'static str, explain: &'static str) -> Tag {
    Tag { key, name, group, explain }
}

const NOUNS: &str = "Nouns and articles";
const ADJECTIVES: &str = "Adjectives and adverbs";
const PRONOUNS: &str = "Pronouns";
const VERBS: &str = "Verbs";
const ORDER: &str = "Word order and clauses";
const QUESTIONS: &str = "Questions and answers";
const PREPS: &str = "Prepositions";
const NUMBERS: &str = "Numbers and time";
const PHRASES: &str = "Everyday phrases";

/// Every tag, in the order they are listed.
pub const TAGS: &[Tag] = &[
    tag("common-gender", "en-words (common gender)", NOUNS, "Most nouns take en, about three in four: en bil, en kop, en dag."),
    tag("neuter", "et-words (neuter)", NOUNS, "The other nouns take et, learnt with each word: et hus, et æble, et barn."),
    tag("definite-singular", "Definite -en / -et", NOUNS, "'The' goes on the end of the noun: bilen, huset, kaffen."),
    tag("definite-plural", "Definite plural -ne / -ene", NOUNS, "'The' on a plural: bilerne, husene, børnene."),
    tag("front-article", "den / det / de + adjective", NOUNS, "With an adjective, 'the' comes first as a word: den røde bil, det store hus, de nye sko."),
    tag("plural-er", "Plural in -er / -r", NOUNS, "The commonest plural: en bil → biler, en skole → skoler, et æble → æbler."),
    tag("plural-e", "Plural in -e", NOUNS, "Many short words: en dag → dage, et hus → huse, en stol → stole."),
    tag("plural-zero", "Plural without an ending", NOUNS, "Some words stay the same, mostly et-words: et år → år, et ord → ord, en sko → sko."),
    tag("plural-irregular", "Irregular plurals", NOUNS, "A vowel change or an odd form: mand → mænd, barn → børn, bog → bøger, øje → øjne."),
    tag("genitive-s", "Possessive -s", NOUNS, "Just -s, no apostrophe: Peters bil, husets dør, Danmarks hovedstad."),
    tag("compound-nouns", "Compound nouns", NOUNS, "Words join into one, and the last part decides en or et: kaffekop, en banegård."),
    tag("no-article", "Nouns without an article", NOUNS, "No article for jobs, nationality or stuff: Hun er læge, Jeg drikker kaffe."),
    tag("adjective-t", "Adjective + -t with et-words", ADJECTIVES, "An indefinite et-word takes -t on its adjective: et stort hus, huset er stort."),
    tag("adjective-e", "Adjective + -e: plural and definite", ADJECTIVES, "-e in the plural and after den, det, de, min or a genitive: store huse, det store hus, mit store hus."),
    tag("adjective-invariable", "Adjectives that never change", ADJECTIVES, "Words ending in -e, -s and a few others: et moderne hus, gratis kaffe, bange."),
    tag("lille-smaa", "lille / små", ADJECTIVES, "lille in the singular, små in the plural: en lille bil, to små biler."),
    tag("comparison", "Comparative and superlative", ADJECTIVES, "-ere, -est or mere, mest: billigere, billigst; god → bedre → bedst."),
    tag("adverbs", "Adverbs from adjectives", ADJECTIVES, "The -t form works as an adverb: Hun synger godt, Han kører hurtigt."),
    tag("modal-particles", "jo, vel, nok, da, bare", ADJECTIVES, "Small unstressed words that colour a sentence: Det er jo rigtigt, Du kommer vel?"),
    tag("personal-pronouns", "Subject pronouns", PRONOUNS, "jeg, du, han, hun, den, det, vi, I, de — den and det also for things."),
    tag("object-pronouns", "Object pronouns", PRONOUNS, "mig, dig, ham, hende, den, det, os, jer, dem: Jeg ser ham, Hun hjælper os."),
    tag("possessives", "Possessives", PRONOUNS, "min / mit / mine, din / dit / dine, hans, hendes, vores, jeres, deres: min bil, mit hus."),
    tag("sin-sit-sine", "sin / sit / sine vs hans / hendes", PRONOUNS, "sin points back to the subject: Han tager sin jakke (his own), hans jakke (someone else's)."),
    tag("demonstratives", "This and that", PRONOUNS, "den her, det der, de her; in writing denne, dette, disse."),
    tag("indefinite-pronouns", "Some, any, none, all", PRONOUNS, "nogen, noget, nogle; ingen, intet; alle, alt: Har du noget kaffe?"),
    tag("man", "man (one, you, people)", PRONOUNS, "The general 'you': Man siger det, Her må man ikke ryge."),
    tag("det-subject", "det as an empty subject", PRONOUNS, "Weather and situations need a subject: Det regner, Det er koldt, Det er fint."),
    tag("infinitive", "Infinitive with at", VERBS, "The dictionary form, with at except after modal verbs: Jeg kan lide at svømme."),
    tag("present", "Present tense -r", VERBS, "One form for every person, ending in -r: jeg spiser, vi spiser, de bor."),
    tag("past-ede", "Past in -ede", VERBS, "Regular verbs like arbejde: arbejdede, boede, snakkede."),
    tag("past-te", "Past in -te", VERBS, "Regular verbs like spise, køre: spiste, kørte, læste."),
    tag("past-strong", "Strong and irregular past", VERBS, "A vowel change and no ending: gå → gik, se → så, drikke → drak, være → var."),
    tag("perfect", "Perfect: har / er + participle", VERBS, "Har du spist? Hun er gået — er with verbs of moving or changing."),
    tag("past-perfect", "Past perfect: havde / var + participle", VERBS, "What had happened first: Jeg havde allerede spist."),
    tag("future", "Talking about the future", VERBS, "vil, skal or kommer til at + infinitive — or just the present: Jeg rejser i morgen."),
    tag("conditional", "Would: ville, kunne, skulle", VERBS, "Wishes and what-ifs: Jeg ville gerne have en kaffe; Hvis jeg var rig, ville jeg rejse."),
    tag("imperative", "Imperative", VERBS, "The bare stem for commands and requests: Kom! Spis! Vent lidt!"),
    tag("modal-verbs", "Modal verbs", VERBS, "kan, vil, skal, må, bør + infinitive without at: Jeg kan svømme, Du må gerne komme."),
    tag("passive-s", "s-passive", VERBS, "-s on the verb, mostly in rules and with modals: Døren lukkes kl. 22, Det kan gøres."),
    tag("passive-blive", "Passive with blive", VERBS, "blive + participle, for events: Huset blev bygget i 1900."),
    tag("s-verbs", "Verbs ending in -s", VERBS, "Active meaning, only an s-form: synes, findes, lykkes, mødes (meet each other)."),
    tag("reflexive-verbs", "Reflexive verbs with sig", VERBS, "Verbs with mig, dig, sig…: glæde sig, sætte sig, skynde sig — Jeg glæder mig."),
    tag("particle-verbs", "Particle verbs", VERBS, "A verb and a stressed little word: stå op, tage på, komme ind, holde op."),
    tag("vaere", "være (to be)", VERBS, "er, var, har været — one form for every person."),
    tag("have", "have (to have)", VERBS, "har, havde, har haft — and the helper of the perfect."),
    tag("liking-opinion", "kan lide, gerne, synes, tror", VERBS, "Liking and opinions: Jeg kan godt lide kaffe, Jeg spiser gerne fisk, Jeg synes, det er godt."),
    tag("v2", "Verb second", ORDER, "The verb is always the second element of a main clause: Jeg spiser fisk i dag."),
    tag("inversion", "Inversion after a fronted element", ORDER, "Anything but the subject first puts the verb before the subject: I dag spiser jeg fisk."),
    tag("ikke-main", "ikke in main clauses", ORDER, "ikke after the verb: Jeg drikker ikke kaffe; after a pronoun object: Jeg kender ham ikke."),
    tag("subordinate-order", "Subordinate clause order", ORDER, "Subject, then ikke or an adverb, then the verb: …fordi jeg ikke har tid."),
    tag("der-er", "der er (there is / are)", ORDER, "Der er en kat i haven; Er der en bank her?"),
    tag("cleft", "Det er … der / som", ORDER, "Putting one thing in focus: Det er Peter, der kommer."),
    tag("relative", "Relative clauses: som / der", ORDER, "manden, som bor her; der only for the subject: manden, der bor her."),
    tag("conjunctions", "og, men, eller, for, så", ORDER, "Joining main clauses; the order after them does not change: Jeg er træt, så jeg går hjem."),
    tag("subordinators", "at, fordi, hvis, når, mens, selvom", ORDER, "Words that start a subordinate clause, with its own order: Jeg tror, at han kommer."),
    tag("naar-da", "når vs da", ORDER, "når for now, the future and every time; da for one time in the past."),
    tag("hjem-hjemme", "Motion vs place: hjem / hjemme", ORDER, "hjem, ud, ind, op for going; hjemme, ude, inde, oppe for being: Jeg går hjem / Jeg er hjemme."),
    tag("yes-no-questions", "Yes/no questions", QUESTIONS, "The verb comes first: Kommer du? Har du tid?"),
    tag("question-words", "Question words (hv-)", QUESTIONS, "hvad, hvem, hvor, hvornår, hvorfor, hvordan, hvilken / hvilket / hvilke, hvor mange."),
    tag("tag-questions", "ikke? / vel?", QUESTIONS, "Asking for agreement: Det er koldt, ikke? Du kommer vel?"),
    tag("ja-jo-nej", "ja, jo, nej", QUESTIONS, "jo is 'yes' against a negative question: Kommer du ikke? — Jo!"),
    tag("i-place", "i: in", PREPS, "Inside, and in towns and countries: i huset, i København, i Danmark."),
    tag("paa-place", "på: on, at", PREPS, "Surfaces, islands, many places and institutions: på bordet, på Fyn, på arbejde, på biblioteket."),
    tag("ved", "ved, ved siden af", PREPS, "By, at, next to: ved vinduet, ved siden af banken."),
    tag("til", "til: to", PREPS, "Where to, and for whom: til København, til lægen, en gave til dig."),
    tag("fra", "fra: from", PREPS, "Where from, since when: fra Danmark, fra klokken ni."),
    tag("place-preps", "under, over, bag, foran, mellem", PREPS, "Where things are: under bordet, bag huset, foran skolen, mellem husene."),
    tag("med-uden", "med, uden", PREPS, "With, without — and how you travel: kaffe med mælk, uden sukker, med bussen."),
    tag("time-preps", "Time: i, på, om, for … siden", PREPS, "i dag, i to år; på mandag; om morgenen, om en uge; for to år siden."),
    tag("fixed-prepositions", "Prepositions after verbs and adjectives", PREPS, "Learnt with the word: tænke på, glad for, bange for, interesseret i."),
    tag("numbers", "Numbers 1–20", NUMBERS, "en / et, to, tre … tyve; en or et with the noun: en kat, et hus."),
    tag("numbers-tens", "Tens: tyve to halvfems", NUMBERS, "Counted in twenties from 50: halvtreds (50), tres (60), halvfjerds (70), firs (80), halvfems (90)."),
    tag("units-first", "Units before tens", NUMBERS, "21 is 'one-and-twenty': enogtyve, femogfyrre, niogtres."),
    tag("ordinals", "Ordinal numbers", NUMBERS, "første, anden, tredje, fjerde: dates and floors (på anden sal)."),
    tag("clock", "Telling the time", NUMBERS, "halv tre is 2:30 (half to three), kvart over fire, fem minutter i seks: Hvad er klokken?"),
    tag("dates", "Days, months and dates", NUMBERS, "den 5. maj, på mandag, i weekenden; days and months without capitals."),
    tag("politeness", "Polite phrases", PHRASES, "tak, undskyld, må jeg bede om, vil du være sød at… — there is no word for 'please'."),
    tag("greetings", "Greetings and goodbyes", PHRASES, "hej, godmorgen, goddag, farvel, hej hej, vi ses."),
    tag("du-address", "du for everyone", PHRASES, "Danes say du to nearly everyone; De is very formal and rare."),
];

/// A tag's id: derived from its key, so it is the same everywhere.
pub fn id_of(key: &str) -> GrammarTagId {
    GrammarTagId::derived("grammar-tag", key)
}

/// Every tag's key, for the tutor's tool schemas (an enum).
pub fn keys() -> Vec<&'static str> {
    TAGS.iter().map(|t| t.key).collect()
}

pub fn by_key(key: &str) -> Option<&'static Tag> {
    let key = key.trim().to_lowercase();
    TAGS.iter().find(|t| t.key == key)
}

pub fn by_id(id: GrammarTagId) -> Option<&'static Tag> {
    TAGS.iter().find(|t| id_of(t.key) == id)
}

/// A tag as the screens and the tutor see it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub id: GrammarTagId,
    pub key: String,
    pub name: String,
    pub group: String,
    pub explain: String,
}

impl From<&Tag> for TagInfo {
    fn from(t: &Tag) -> Self {
        TagInfo { id: id_of(t.key), key: t.key.into(), name: t.name.into(), group: t.group.into(), explain: t.explain.into() }
    }
}

/// The tags given, checked: every key must be one of the list (the error
/// names the unknown ones, so the tutor can correct itself); duplicates
/// are dropped. At most eight: a tag says what the item is *for*.
pub fn validate(keys: &[String]) -> Result<Vec<&'static Tag>> {
    let mut kept: Vec<&'static Tag> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for key in keys {
        match by_key(key) {
            Some(tag) if !kept.iter().any(|k| k.key == tag.key) => kept.push(tag),
            Some(_) => {}
            None => unknown.push(key.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(Error::msg(format!(
            "Unknown grammar tags: {}. Use only keys from the list (e.g. \"definite-singular\", \"adjective-t\", \"past-te\").",
            unknown.join(", ")
        )));
    }
    kept.truncate(8);
    Ok(kept)
}

/// Gives a word its tags (replacing any it had).
pub fn set_word(conn: &mut Conn, word_id: WordId, tags: &[&Tag]) -> Result<()> {
    diesel::delete(word_grammar::table.filter(word_grammar::word_id.eq(word_id))).execute(conn)?;
    for t in tags {
        diesel::insert_or_ignore_into(word_grammar::table)
            .values((word_grammar::word_id.eq(word_id), word_grammar::tag_id.eq(id_of(t.key))))
            .execute(conn)?;
    }
    Ok(())
}

/// Gives a sentence its tags (replacing any it had).
pub fn set_sentence(conn: &mut Conn, sentence_id: SentenceId, tags: &[&Tag]) -> Result<()> {
    diesel::delete(sentence_grammar::table.filter(sentence_grammar::sentence_id.eq(sentence_id))).execute(conn)?;
    for t in tags {
        diesel::insert_or_ignore_into(sentence_grammar::table)
            .values((sentence_grammar::sentence_id.eq(sentence_id), sentence_grammar::tag_id.eq(id_of(t.key))))
            .execute(conn)?;
    }
    Ok(())
}

/// Every tagged item's tags: word or sentence id (as text) → tag ids.
pub fn all_links(conn: &mut Conn) -> Result<std::collections::HashMap<String, Vec<GrammarTagId>>> {
    let mut out: std::collections::HashMap<String, Vec<GrammarTagId>> = std::collections::HashMap::new();
    for (word, tag) in word_grammar::table.select((word_grammar::word_id, word_grammar::tag_id)).load::<(WordId, GrammarTagId)>(conn)? {
        out.entry(word.to_string()).or_default().push(tag);
    }
    for (sentence, tag) in sentence_grammar::table
        .select((sentence_grammar::sentence_id, sentence_grammar::tag_id))
        .load::<(SentenceId, GrammarTagId)>(conn)?
    {
        out.entry(sentence.to_string()).or_default().push(tag);
    }
    Ok(out)
}

/// One item the tutor tags (`tag_grammar`): a word or a sentence by id.
#[derive(Debug, Deserialize)]
pub struct TagItem {
    pub id: String,
    #[serde(default)]
    pub grammar: Vec<String>,
}

/// Tags existing words and sentences (the tutor, asked by the learner).
/// Every id and tag is checked first; one bad one fails the whole call (it
/// runs in the tool's transaction), with a message saying which.
pub fn tag_items(conn: &mut Conn, items: &[TagItem]) -> Result<(usize, usize)> {
    if items.len() > 200 {
        return Err(Error::msg("At most 200 items per call: tag the rest in another call."));
    }
    let (mut words, mut sentences) = (0, 0);
    for item in items {
        let tags = validate(&item.grammar)?;
        let id = item.id.trim();
        if id.starts_with("wd_") {
            let word = WordId::parse(id).map_err(|e| Error::msg(e.to_string()))?;
            let exists: i64 = crate::schema::words::table.find(word).count().get_result(conn)?;
            if exists == 0 {
                return Err(Error::msg(format!("No word {id}.")));
            }
            set_word(conn, word, &tags)?;
            words += 1;
        } else {
            let sentence = SentenceId::parse(id).map_err(|_| Error::msg(format!("{id}: give a word (wd_…) or a sentence (sn_…) id.")))?;
            let exists: i64 = crate::schema::sentences::table.find(sentence).count().get_result(conn)?;
            if exists == 0 {
                return Err(Error::msg(format!("No sentence {id}.")));
            }
            set_sentence(conn, sentence, &tags)?;
            sentences += 1;
        }
    }
    Ok((words, sentences))
}

/// How many words and sentences in open courses have no tag yet: what the
/// Structures page's "Tag the grammar" button offers to do.
pub fn untagged_count(conn: &mut Conn) -> Result<i64> {
    let words: i64 = diesel::sql_query(
        "SELECT count(DISTINCT w.id) AS n FROM words w
           JOIN course_lessons cl ON cl.lesson_id = w.lesson_id
           JOIN courses c ON c.id = cl.course_id AND c.closed_at IS NULL
          WHERE NOT EXISTS (SELECT 1 FROM word_grammar g WHERE g.word_id = w.id)",
    )
    .get_result::<Count>(conn)?
    .n;
    let sentences: i64 = diesel::sql_query(
        "SELECT count(DISTINCT s.id) AS n FROM sentences s
           JOIN course_lessons cl ON cl.lesson_id = s.lesson_id
           JOIN courses c ON c.id = cl.course_id AND c.closed_at IS NULL
          WHERE NOT EXISTS (SELECT 1 FROM sentence_grammar g WHERE g.sentence_id = s.id)",
    )
    .get_result::<Count>(conn)?
    .n;
    Ok(words + sentences)
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    n: i64,
}

/// Untagged words and sentences, a page of them, for the tutor to tag: open
/// courses only, one course when asked.
pub fn untagged(conn: &mut Conn, course_id: Option<CourseId>, limit: i64) -> Result<Vec<(String, String, String, String)>> {
    #[derive(QueryableByName)]
    struct Row {
        #[diesel(sql_type = diesel::sql_types::Text)]
        id: String,
        #[diesel(sql_type = diesel::sql_types::Text)]
        da: String,
        #[diesel(sql_type = diesel::sql_types::Text)]
        en: String,
        #[diesel(sql_type = diesel::sql_types::Text)]
        course: String,
    }
    let course = course_id.map(|c| c.to_string()).unwrap_or_default();
    let rows: Vec<Row> = diesel::sql_query(
        "SELECT DISTINCT w.id AS id, w.da AS da, w.en AS en, c.title_en AS course FROM words w
           JOIN course_lessons cl ON cl.lesson_id = w.lesson_id
           JOIN courses c ON c.id = cl.course_id AND c.closed_at IS NULL
          WHERE NOT EXISTS (SELECT 1 FROM word_grammar g WHERE g.word_id = w.id) AND (?1 = '' OR c.id = ?1)
         UNION
         SELECT DISTINCT s.id, s.da, s.en, c.title_en FROM sentences s
           JOIN course_lessons cl ON cl.lesson_id = s.lesson_id
           JOIN courses c ON c.id = cl.course_id AND c.closed_at IS NULL
          WHERE NOT EXISTS (SELECT 1 FROM sentence_grammar g WHERE g.sentence_id = s.id) AND (?1 = '' OR c.id = ?1)
         ORDER BY course, id LIMIT ?2",
    )
    .bind::<diesel::sql_types::Text, _>(course)
    .bind::<diesel::sql_types::BigInt, _>(limit.clamp(1, 200))
    .load(conn)?;
    Ok(rows.into_iter().map(|r| (r.id, r.da, r.en, r.course)).collect())
}

/// The vocabulary as the tutor's prompt lists it: key — name, grouped.
pub fn for_tutor() -> String {
    let mut out = String::new();
    let mut group = "";
    for t in TAGS {
        if t.group != group {
            group = t.group;
            out.push_str(&format!("\n  {group}: "));
        } else {
            out.push_str(", ");
        }
        out.push_str(&format!("{} ({})", t.key, t.name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_is_well_formed() {
        assert!(TAGS.len() >= 60, "{}", TAGS.len());
        let keys: std::collections::HashSet<_> = TAGS.iter().map(|t| t.key).collect();
        assert_eq!(keys.len(), TAGS.len(), "no two tags share a key");
        for t in TAGS {
            assert!(t.key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{}", t.key);
            assert!(!t.name.is_empty() && t.explain.len() > 10, "{}", t.key);
            assert_eq!(by_id(id_of(t.key)).map(|x| x.key), Some(t.key));
        }
        assert_eq!(id_of("definite-singular"), id_of("definite-singular"));
        assert!(id_of("definite-singular").to_string().starts_with("gr_"));
    }

    #[test]
    fn unknown_tags_are_named_and_duplicates_dropped() {
        let ok = validate(&["Definite-Singular".into(), "v2".into(), "definite-singular".into()]).unwrap();
        assert_eq!(ok.iter().map(|t| t.key).collect::<Vec<_>>(), ["definite-singular", "v2"]);
        let err = validate(&["v2".into(), "made-up-tag".into()]).unwrap_err().to_string();
        assert!(err.contains("made-up-tag") && !err.contains("v2,"), "{err}");
    }
}

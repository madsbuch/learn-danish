mod agent;
mod ai;
mod audio;
mod bench;
mod convert;
mod ids;
mod resolve;
mod upgrade_v7;
mod upgrade_v8;
mod sync;
mod wire;
mod lessons;
mod levels;
mod lesson_types;
mod cast;
mod commands;
#[cfg(test)]
mod content;
mod costs;
mod courses;
mod db;
mod demo_course;
mod diagnostics;
mod error;
mod examples;
mod files;
mod examiner;
mod http;
mod keyshare;
mod memories;
mod pictures;
mod plan;
mod practice;
mod models;
mod playlists;
mod schema;
mod session;
mod songs;
mod podcasts;
mod news;
mod offline;
mod speech;
mod srs;
mod stats;
mod starter_retirement;
mod text;
mod tiles;
mod tutor;
mod wiki;
mod work;
mod xplog;
mod games;
mod dictation;
mod sounds;
mod speaking;
mod singalong;
mod scenes;
mod stories;
mod grammar;
mod mistakes;
mod learnlog;
mod masterylog;
mod analytics;
mod tutor_analytics;
mod topics;
mod wordlinks;
mod rawlog;
mod contexts;


use tauri::{Emitter, Manager};

use db::Db;

#[cfg(mobile)]
fn barcode_scanner<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_barcode_scanner::init()
}

/// Desktops have no scanner; they show their QR code for a phone to read.
#[cfg(not(mobile))]
fn barcode_scanner<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("barcode-scanner-none").build()
}

/// Taps under the finger for right and wrong answers (src/lib/haptics.ts).
#[cfg(mobile)]
fn haptics<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_haptics::init()
}

/// Computers have nothing to buzz; the front end doesn't call it there.
#[cfg(not(mobile))]
fn haptics<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("haptics-none").build()
}


/// On a computer, the window opens like a computer program's: landscape,
/// 1200 × 860 (the size in tauri.conf.json, where the app's wide layouts —
/// the dashboard home, dialogs for sheets — take over), shrunk to fit a
/// smaller screen, and centred.
#[cfg(desktop)]
fn fit_window_to_screen(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else { return };
    let Some(configured) = app.config().app.windows.first() else { return };
    let Ok(Some(monitor)) = window.current_monitor() else { return };
    let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
    let width = configured.width.min(screen.width * 0.92);
    let height = configured.height.min(screen.height * 0.88);
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    let _ = window.center();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::started();
    diagnostics::note_panics();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_songplayer::init())
        // Android's background sync (sync/background.rs): scheduled by sync's
        // loop once the front end is up, while there is a device to sync with.
        .plugin(tauri_plugin_sync_background::init())
        // Scanning another device's API keys off a QR code (phones only).
        .plugin(barcode_scanner())
        // A soft tap for a right answer, a double buzz for a wrong one (phones).
        .plugin(haptics())
        // Every file the webview shows or plays — pictures, covers, songs —
        // by id, straight from the files table: media://localhost/<id>.
        .register_asynchronous_uri_scheme_protocol("media", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(files::serve(&app, &request)));
        })
        // Registered before the webview can call anything; filled in by setup.
        .manage(Db::default())
        .manage(cast::CastSessions::default())
        .manage(std::sync::Arc::new(sync::service::SyncService::default()))
        .setup(|app| {
            #[cfg(desktop)]
            fit_window_to_screen(app);
            let data = app.path().app_data_dir().map_err(|e| format!("no writable app data directory: {e}"))?;
            // First, so even opening the database can note what it finds.
            diagnostics::init(data.clone());
            // Work anywhere (every AI request) keeps the app alive (work.rs).
            work::set_app(app.handle());
            let path = data.join("learn-danish.db");
            // For the job that finishes a course's download without the app.
            offline::set_db_path(&path);

            let mut connection = db::open(&path).map_err(|e| e.to_string())?;
            // Replies that were being written when the app last closed.
            let _ = tutor::recover_interrupted(&mut connection);
            // Courses mastered under the old rules (closing was by hand) move
            // to Mastered now.
            let _ = courses::milestones(&mut connection);
            app.state::<Db>().fill(connection);
            // Sync with the learner's other devices (docs/SYNC.md): listening,
            // discovery and the loop, on the async runtime.
            let sync = app.state::<std::sync::Arc<sync::service::SyncService>>().inner().clone();
            let handle = app.handle().clone();
            let db_path = path.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = sync.start(handle, db_path).await {
                    diagnostics::note("sync", format!("Sync could not start: {error}"));
                }
            });
            // Work for the background starts a moment after the first screen
            // has its data: it all takes the same database, and the first
            // screen should not wait behind it.
            let later = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let app = &later;
                songs::resume_pending(app);
                // Picture lessons whose drawing an earlier run did not finish.
                pictures::draw_missing_in_background(app);
                // Course pictures the tutor asked for that an earlier run did not draw.
                pictures::draw_wanted_in_background(app);
                // Scene lessons whose picture an earlier run did not finish.
                scenes::resume_pending(app);
                // Story lessons whose panels an earlier run did not finish drawing.
                stories::resume_pending(app);
                // The demo's pictures and episode, once there is a key for them.
                demo_course::make_media_in_background(app);
                files::sync_to_disk(app);
                // Courses kept offline: audio their lessons have gained since.
                offline::keep_up_in_background(app);
                // Lessons built on Wikipedia, checked against it every three months.
                wiki::check_due_in_background(app);
            });
            Ok(())
        })
        // Back in front (a phone's app resumed, a computer's window focused):
        // the front end reloads what it shows and asks for a sync
        // (src/lib/sync.ts). A background sync ran meanwhile, perhaps, and it
        // tells no one.
        .on_window_event(|window, event| {
            let resumed = match event {
                tauri::WindowEvent::Focused(focused) => *focused,
                #[cfg(mobile)]
                tauri::WindowEvent::Resumed => true,
                _ => false,
            };
            if resumed {
                let _ = window.emit(sync::service::RESUMED_EVENT, ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_app,
            commands::get_settings,
            commands::update_settings,
            practice::submit_answer,
            learnlog::learning_log,
            games::quick_game_answer,
            analytics::analytics_overview,
            analytics::analytics_hard,
            analytics::analytics_mistakes,
            analytics::analytics_compare,
            analytics::analytics_activity,
            analytics::analytics_item,
            analytics::met_words_list,
            analytics::met_word_detail,
            analytics::met_structures,
            analytics::met_structure,
            analytics::met_sounds,
            contexts::context_stats,
            contexts::item_contexts_cmd,
            contexts::met_topics,
            contexts::met_topic,
            rawlog::learning_raw_log,
            rawlog::learning_log_export,
            lessons::commands::lessons_overview,
            lessons::commands::lessons_for_course,
            lessons::commands::lessons_by_importance,
            lessons::commands::lesson_detail,
            practice::lesson_session,
            practice::lesson_finish,
            masterylog::lesson_mastery_log,
            lessons::commands::lesson_set_period,
            lessons::commands::lesson_set_challenge,
            lessons::commands::lesson_delete,
            lessons::commands::lesson_for_song,
            practice::checkin_summary,
            examiner::checkin_start,
            examiner::checkin_send,
            examiner::checkin_end,
            commands::get_stats,
            commands::get_course_progress,
            commands::browse_words,
            commands::browse_sentences,
            commands::browse_catalogue,
            commands::ai_set_key,
            commands::ai_clear_key,
            commands::ai_test_key,
            commands::ai_list_models,
            tutor::tutor_send,
            tutor::tutor_retry,
            tutor::tutor_talk_about,
            examiner::writing_scaffold,
            costs::ai_usage,
            bench::bench_run,
            bench::bench_runs,
            bench::bench_delete,
            examiner::writing_hint,
            examples::word_examples,
            examiner::lesson_add_players,
            offline::course_offline,
            offline::course_offline_set,
            offline::course_offline_download,
            pictures::course_picture,
            wiki::lesson_sources,
            wiki::source_article,
            wiki::source_remove,
            wiki::fact_check_lesson,
            pictures::course_picture_delete,
            pictures::course_picture_status,
            pictures::lesson_add_pictures,
            scenes::scene_make,
            stories::story_draw,
            lesson_types::lesson_types,
            diagnostics::diagnostics_list,
            resolve::ids_resolve,
            db::db_upgrading,
            diagnostics::diagnostics_note,
            xplog::xp_day,
            games::quick_games,
            games::quick_game_deal,
            games::quick_game_finish,
            dictation::dictation_check,
            speaking::speech_hear,
            speaking::speech_check,
            singalong::song_glossary,
            xplog::xp_history,
            diagnostics::diagnostics_clear,
            pictures::word_picture_draw,
            pictures::word_picture_delete,
            tutor::app_visibility,
            tutor::tutor_conversations,
            tutor::tutor_history,
            tutor::tutor_rename,
            tutor::tutor_delete,
            courses::commands::course_delete,
            courses::commands::course_close,
            courses::commands::course_rename,
            commands::export_database,
            costs::commands::costs_summary,
            memories::commands::memories_overview,
            memories::commands::memories_set_enabled,
            memories::commands::memory_add,
            memories::commands::memory_update,
            memories::commands::memory_delete,
            memories::commands::memories_clear,
            speech::list_voices,
            speech::preview_voice,
            speech::speak,
            speech::cache_audio,
            speech::course_audio_lines,
            speech::audio_cache_stats,
            speech::clear_audio_cache,
            speech::audio_overview,
            speech::course_audio,
            speech::delete_unused_audio,
            speech::respeak,
            speech::prefetch_audio,
            commands::kie_set_key,
            commands::kie_clear_key,
            commands::keys_share_qr,
            commands::keys_import,
            songs::kie_credits,
            songs::songs_list,
            wiki::song_sources,
            wiki::song_source_open,
            wiki::wiki_find,
            podcasts::podcast_topics,
            podcasts::podcast_draft,
            podcasts::podcast_create,
            podcasts::podcast_make,
            podcasts::podcast_rerender,
            songs::song_draft,
            songs::song_create,
            songs::song_retry,
            songs::song_delete,
            songs::song_track_delete,
            songs::song_track_played,
            songs::song_track_lyrics,
            playlists::playlists_list,
            playlists::playlist_create,
            playlists::playlist_rename,
            playlists::playlist_delete,
            playlists::playlist_add,
            playlists::playlist_remove,
            playlists::playlist_move,
            playlists::song_track_save,
            playlists::song_track_share_file,
            cast::cast_discover,
            cast::cast_start,
            cast::cast_command,
            cast::cast_set_repeat,
            cast::cast_state,
            cast::cast_control_info,
            sync::service::sync_status,
            sync::service::sync_set_name,
            sync::service::sync_offer,
            sync::service::sync_cancel_offer,
            sync::service::sync_join,
            sync::service::sync_now,
            sync::service::sync_remove_device,
            sync::service::sync_held_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}

#[cfg(test)]
mod integration {
    //! Drives a real database through a full check-in, without Tauri.

    use super::*;
    use crate::db::Conn;
    use crate::schema as t;
    use diesel::connection::SimpleConnection;
    use crate::ids::*;
    use diesel::prelude::*;

    /// Reopens a scratch database without wiping it, to exercise migrations.
    fn scratch_db_keep(name: &str) -> Conn {
        let path = std::env::temp_dir()
            .join("learn-danish-tests")
            .join(format!("{name}-{}.db", std::process::id()));
        db::open(&path).expect("reopen")
    }

    fn grammar_course(conn: &mut Conn) -> CourseId {
        t::courses::table
            .filter(t::courses::slug.eq("grammar-essentials"))
            .select(t::courses::id)
            .first(conn)
            .expect("the starter lessons have a course")
    }

    fn starter_lesson(conn: &mut Conn) -> lessons::Lesson {
        let grammar = grammar_course(conn);
        lessons::for_course(conn, grammar).unwrap().remove(0)
    }

    /// Fixture ids: song `n`, take `n` and file `n` of a test.
    fn song(n: u32) -> SongId {
        SongId::derived("test", &format!("song-{n}"))
    }
    fn track(n: u32) -> TrackId {
        TrackId::derived("test", &format!("track-{n}"))
    }
    fn file(n: u32) -> FileId {
        FileId::derived("test", &format!("file-{n}"))
    }

    /// Raw SQL with `$s1`, `$t10`, `$f100` standing for (quoted) [`song`],
    /// [`track`] and [`file`] ids.
    fn fixture(sql: &str) -> String {
        let mut out = String::new();
        let mut rest = sql;
        while let Some(at) = rest.find('$') {
            out.push_str(&rest[..at]);
            let tail = &rest[at + 1..];
            let kind = tail.chars().next().unwrap_or(' ');
            let digits: String = tail[1..].chars().take_while(char::is_ascii_digit).collect();
            let n: u32 = digits.parse().expect("a fixture id");
            let id = match kind {
                's' => song(n).to_string(),
                't' => track(n).to_string(),
                'f' => file(n).to_string(),
                other => panic!("unknown fixture kind {other}"),
            };
            out.push_str(&format!("'{id}'"));
            rest = &tail[1 + digits.len()..];
        }
        out.push_str(rest);
        out
    }

    fn scratch_db(name: &str) -> Conn {
        let path = std::env::temp_dir()
            .join("learn-danish-tests")
            .join(format!("{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut conn = db::open(&path).expect("open");
        db::seed_test_courses(&mut conn).expect("historical test fixtures");
        db::unify_courses(&mut conn).unwrap();
        // Pin the clock to a known day so the assertions do not depend on today.
        db::set_setting(&mut conn, "tz_offset_minutes", "0").unwrap();
        conn
    }

    #[test]
    fn historical_course_fixtures_can_be_practised_and_deleted() {
        let mut conn = scratch_db("seed");
        // Ten courses and the course holding the grammar lessons.
        assert_eq!(t::courses::table.count().get_result::<i64>(&mut conn).unwrap(), 11);
        assert!(t::words::table.count().get_result::<i64>(&mut conn).unwrap() >= 160);
        assert_eq!(t::sentences::table.count().get_result::<i64>(&mut conn).unwrap(), content::COURSES.iter().map(|t| crate::convert::count(content::parse_sentences(t.sentences).len())).sum::<i64>());
        assert!(t::conversation_lines::table.count().get_result::<i64>(&mut conn).unwrap() >= 100);

        // Opening again must not duplicate anything — and a starter course the
        // learner deleted stays deleted: it is theirs now, never seeded again.
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        crate::courses::delete(&mut conn, basics).unwrap();
        drop(conn);
        let mut conn = scratch_db_keep("seed");
        assert_eq!(t::courses::table.count().get_result::<i64>(&mut conn).unwrap(), 10);
    }

    #[test]
    fn the_course_is_cut_into_small_lessons() {
        let mut conn = scratch_db("lessons");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let list = lessons::for_course(&mut conn, basics).unwrap();
        let count = |format: &str| list.iter().filter(|b| b.lesson_type == format).count();
        assert_eq!(count("dialogue"), 1);
        assert!(count("words") + count("typing") >= 20, "words come in eights");
        assert!(count("typing") > 0 && count("words") > count("typing"), "every third batch is typed");
        assert_eq!(count("sentences"), 4, "twenty sentences in fives");
        assert!(list.iter().all(|b| b.due_day.is_none()), "nothing is scheduled before it is done");

        // A word drill is small: eight new words, met then recognised, a
        // couple of sound pairs when the words have the sounds, and a memory round.
        let words = list.iter().find(|b| b.lesson_type == "words").unwrap();
        assert_eq!(words.complexity, "low");
        let plan = session::build_lesson(&mut conn, words.id).unwrap();
        assert_eq!(plan.new_words, 8);
        assert!(plan.exercises.len() <= 17 + crate::sounds::ROUND, "{} exercises", plan.exercises.len());
        assert!(plan.exercises.iter().filter(|e| e.kind == "soundPair").count() <= crate::sounds::ROUND);
        assert_eq!(plan.exercises[0].kind, "intro");
        assert!(plan.exercises.iter().any(|e| e.kind == "memory"));

        // The dialogue drill reads first, then drills its lines.
        let dialogue = list.iter().find(|b| b.lesson_type == "dialogue").unwrap();
        let plan = session::build_lesson(&mut conn, dialogue.id).unwrap();
        assert_eq!(plan.exercises[0].kind, "dialogueRead");
        assert!(plan.exercises.iter().any(|e| e.kind == "dialogueGap"));

        // Sentences: build each, then fill gaps from a bank that holds every answer.
        let sentences = list.iter().find(|b| b.lesson_type == "sentences").unwrap();
        assert_eq!(sentences.complexity, "medium");
        let plan = session::build_lesson(&mut conn, sentences.id).unwrap();
        assert!(plan.exercises.iter().any(|e| e.kind == "wordOrder"));
        let gaps = plan.exercises.iter().find(|e| e.kind == "gapFill").expect("fill the gaps");
        for gap in &gaps.gaps {
            assert!(gaps.options.contains(&gap.answer), "the bank holds every answer");
        }
        for exercise in &plan.exercises {
            if !exercise.options.is_empty() && exercise.kind != "gapFill" {
                assert!(exercise.options.contains(&exercise.answer), "{} lost its answer", exercise.key);
            }
        }

        // The starter lessons are there, as study lessons in their own course.
        let grammar = grammar_course(&mut conn);
        let lessons = lessons::for_course(&mut conn, grammar).unwrap();
        assert!(lessons.len() >= 7);
        assert!(lessons.iter().all(|b| b.lesson_type == "study" && b.origin == "starter"));
        let overview = lessons::overview(&mut conn).unwrap();
        assert!(overview.due.iter().all(|l| l.due_day.is_none()), "nothing is due on a fresh install: only new lessons");
        assert_eq!(overview.due[0].course_id, Some(basics), "today starts the course at its beginning");
    }

    #[test]
    fn a_version_3_database_moves_words_into_lessons() {
        // The shape before words belonged to lessons: words, sentences and
        // lines in courses, drills pointing at them, one course per lesson.
        let path = std::env::temp_dir()
            .join("learn-danish-tests")
            .join(format!("v3-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        {
            // An old schema, so raw SQL: these tables are not in crate::schema.
            let mut old = Conn::establish(&path.to_string_lossy()).unwrap();
            old.batch_execute(
                "CREATE TABLE topics (id INTEGER PRIMARY KEY, slug TEXT NOT NULL UNIQUE, ord INTEGER NOT NULL,
                     kind TEXT NOT NULL DEFAULT 'course', title_da TEXT NOT NULL, title_en TEXT NOT NULL,
                     summary_en TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE words (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL, ord INTEGER NOT NULL,
                     da TEXT NOT NULL, en TEXT NOT NULL, pos TEXT NOT NULL, UNIQUE (topic_id, da));
                 CREATE TABLE sentences (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL, ord INTEGER NOT NULL,
                     da TEXT NOT NULL, en TEXT NOT NULL, UNIQUE (topic_id, da));
                 CREATE TABLE conversation_lines (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL,
                     ord INTEGER NOT NULL, speaker TEXT NOT NULL, gender TEXT NOT NULL DEFAULT 'f',
                     da TEXT NOT NULL, en TEXT NOT NULL);
                 CREATE TABLE reviews (kind TEXT NOT NULL, item_id INTEGER NOT NULL, due_day INTEGER NOT NULL,
                     interval INTEGER NOT NULL, ease REAL NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL,
                     last_day INTEGER NOT NULL, PRIMARY KEY (kind, item_id));
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE blocks (id INTEGER PRIMARY KEY, slug TEXT UNIQUE, kind TEXT NOT NULL,
                     format TEXT NOT NULL DEFAULT '', topic_id INTEGER, ord INTEGER NOT NULL DEFAULT 0,
                     title TEXT NOT NULL, summary TEXT NOT NULL DEFAULT '', complexity TEXT NOT NULL,
                     lesson TEXT NOT NULL DEFAULT '', objectives TEXT NOT NULL DEFAULT '[]',
                     period_days INTEGER NOT NULL, interval_days INTEGER NOT NULL DEFAULT 0, due_day INTEGER,
                     last_day INTEGER, last_result TEXT, reps INTEGER NOT NULL DEFAULT 0,
                     origin TEXT NOT NULL DEFAULT 'course', created_at INTEGER NOT NULL, song_id INTEGER);
                 CREATE TABLE block_items (block_id INTEGER NOT NULL, kind TEXT NOT NULL, item_id INTEGER NOT NULL,
                     ord INTEGER NOT NULL, PRIMARY KEY (block_id, kind, item_id));
                 CREATE TABLE checkins (id INTEGER PRIMARY KEY, block_id INTEGER NOT NULL, started_at INTEGER NOT NULL,
                     finished_at INTEGER, result TEXT, feedback TEXT);
                 CREATE TABLE checkin_messages (id INTEGER PRIMARY KEY, checkin_id INTEGER NOT NULL,
                     role TEXT NOT NULL, content TEXT NOT NULL, created_at INTEGER NOT NULL);

                 INSERT INTO topics VALUES (1, 'basics', 1, 'course', 'Det første møde', 'Meeting', '', 0),
                                           (2, 'programme-1-1', 1, 'programme', 'Farver', 'Colours', '', 50);
                 INSERT INTO words VALUES (10, 1, 0, 'hej', 'hi', 'interj'),
                                          (20, 2, 0, 'rød', 'red', 'adj'),
                                          (21, 2, 1, 'hvid', 'white', 'adj'),
                                          (22, 2, 2, 'czarny', 'black', 'adj');
                 INSERT INTO sentences VALUES (30, 2, 0, 'Himlen er blå.', 'The sky is blue.');
                 INSERT INTO conversation_lines VALUES (40, 1, 0, 'Anna', 'f', 'Godmorgen!', 'Good morning!');
                 INSERT INTO reviews VALUES ('word', 10, 100, 3, 2.5, 2, 0, 97), ('word', 20, 100, 3, 2.5, 4, 0, 97);
                 -- A starter topic, cut into a dialogue and a word drill.
                 INSERT INTO blocks (id, slug, kind, format, topic_id, ord, title, complexity, period_days, created_at, due_day, reps)
                     VALUES (100, 'topic/1/dialogue', 'drill', 'dialogue', 1, 1, 'Conversation', 'low', 2, 0, NULL, 0),
                            (101, 'topic/1/words-1', 'drill', 'words', 1, 2, 'Words 1', 'low', 2, 0, 105, 3);
                 INSERT INTO block_items VALUES (101, 'word', 10, 0);
                 -- A programme: the tutor's memory drill over two words, which the
                 -- programme's own word drill also picked up (the duplicate).
                 INSERT INTO blocks (id, slug, kind, format, topic_id, ord, title, complexity, period_days, origin, created_at)
                     VALUES (200, NULL, 'drill', 'memory', 2, 1, 'Colours — memory', 'low', 2, 'tutor', 60),
                            (201, 'topic/2/words-1', 'drill', 'words', 2, 2, 'Words 1', 'low', 2, 'tutor', 50),
                            (202, 'topic/2/sentences-1', 'drill', 'sentences', 2, 3, 'Sentences 1', 'medium', 4, 'tutor', 50),
                            (203, 'topic/2/words-2', 'drill', 'words', 2, 4, 'Words 2', 'low', 2, 'tutor', 50);
                 INSERT INTO block_items VALUES (200, 'word', 20, 0), (200, 'word', 21, 1),
                                                (201, 'word', 20, 0), (201, 'word', 21, 1), (201, 'word', 22, 2),
                                                (202, 'sentence', 30, 0),
                                                (203, 'word', 21, 0);
                 -- A lesson with no topic, with a finished check-in.
                 INSERT INTO blocks (id, slug, kind, format, topic_id, title, complexity, lesson, period_days, origin, created_at)
                     VALUES (300, NULL, 'study', '', NULL, 'At gå', 'low', 'jeg går', 5, 'tutor', 70);
                 INSERT INTO checkins VALUES (1, 300, 0, 1, 'pass', 'Good.');
                 INSERT INTO settings VALUES ('content_version', '4');
                 PRAGMA user_version = 3;",
            )
            .unwrap();
        }

        let mut conn = db::open(&path).expect("upgrade");
        // Ids are tagged now (version 7), and the old slugs were cleared long
        // ago: rows are found by what they hold, and where. (The demo course,
        // added since, has a "hej" and a "Words 1" of its own.)
        let course = |conn: &mut Conn, slug: &str| -> CourseId {
            t::courses::table.filter(t::courses::slug.eq(slug)).select(t::courses::id).first(conn).unwrap()
        };
        let basics = course(&mut conn, "basics");
        let colours = course(&mut conn, "programme-1-1");
        let in_course = |conn: &mut Conn, course: CourseId, title: &str| -> LessonId {
            t::course_lessons::table
                .inner_join(t::lessons::table)
                .filter(t::course_lessons::course_id.eq(course))
                .filter(t::lessons::title.eq(title))
                .select(t::lessons::id)
                .first(conn)
                .unwrap()
        };
        let lesson = |conn: &mut Conn, slug: &str| -> LessonId {
            match slug {
                "topic/1/words-1" => in_course(conn, basics, "Words 1"),
                "topic/1/dialogue" => in_course(conn, basics, "Conversation"),
                "topic/2/words-1" => in_course(conn, colours, "Words 1"),
                "topic/2/sentences-1" => in_course(conn, colours, "Sentences 1"),
                other => panic!("no lesson {other} in this test"),
            }
        };
        let titled = |conn: &mut Conn, title: &str| -> LessonId {
            t::lessons::table.filter(t::lessons::title.eq(title)).select(t::lessons::id).first(conn).unwrap()
        };
        let course_word = |conn: &mut Conn, da: &str| -> (WordId, LessonId) {
            t::course_words::table
                .filter(t::course_words::course_id.eq_any([basics, colours]))
                .filter(t::course_words::da.eq(da))
                .select((t::course_words::id, t::course_words::lesson_id))
                .first(conn)
                .unwrap()
        };
        let word = |conn: &mut Conn, da: &str| -> WordId { course_word(conn, da).0 };
        let word_owner = |conn: &mut Conn, da: &str| -> LessonId { course_word(conn, da).1 };
        // Review history follows each word to its new id.
        assert_eq!(word_owner(&mut conn, "hej"), lesson(&mut conn, "topic/1/words-1"));
        let hej_id = word(&mut conn, "hej").to_string();
        assert_eq!(srs::load(&mut conn, "word", &hej_id).unwrap().unwrap().reps, 2);
        let red_id = word(&mut conn, "rød").to_string();
        assert_eq!(srs::load(&mut conn, "word", &red_id).unwrap().unwrap().reps, 4);
        // A shared word goes to the tutor's drill, not the automatic one.
        let memory = titled(&mut conn, "Colours — memory");
        assert_eq!(word_owner(&mut conn, "rød"), memory);
        assert_eq!(word_owner(&mut conn, "hvid"), memory);
        assert_eq!(word_owner(&mut conn, "czarny"), lesson(&mut conn, "topic/2/words-1"));
        let sentence: LessonId = t::sentences::table
            .filter(t::sentences::da.eq("Himlen er blå."))
            .select(t::sentences::lesson_id)
            .first(&mut conn)
            .unwrap();
        assert_eq!(sentence, lesson(&mut conn, "topic/2/sentences-1"));
        let line: LessonId = t::conversation_lines::table
            .filter(t::conversation_lines::da.eq("Godmorgen!"))
            .select(t::conversation_lines::lesson_id)
            .first(&mut conn)
            .unwrap();
        assert_eq!(line, lesson(&mut conn, "topic/1/dialogue"));
        // An automatic drill left with nothing goes.
        let words_2: i64 = t::course_lessons::table
            .inner_join(t::lessons::table)
            .filter(t::course_lessons::course_id.eq(colours))
            .filter(t::lessons::title.eq("Words 2"))
            .count()
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(words_2, 0);
        let words_1 = lesson(&mut conn, "topic/1/words-1");
        assert_eq!(lessons::load(&mut conn, words_1).unwrap().due_day, Some(105), "schedules survive");

        // Every lesson is listed in a course; programmes are courses now.
        let listed: std::collections::HashSet<LessonId> =
            t::course_lessons::table.select(t::course_lessons::lesson_id).load::<LessonId>(&mut conn).unwrap().into_iter().collect();
        let all: Vec<LessonId> = t::lessons::table.select(t::lessons::id).load(&mut conn).unwrap();
        assert!(all.iter().all(|id| listed.contains(id)), "every block is in a course");
        let at_gå = titled(&mut conn, "At gå");
        assert_eq!(lessons::load(&mut conn, at_gå).unwrap().course_title.as_deref(), Some("Other lessons"));
        assert_eq!(lessons::detail(&mut conn, at_gå).unwrap().last_feedback.as_deref(), Some("Good."));
        let kinds: i64 =
            t::courses::table.filter(t::courses::kind.ne("course")).count().get_result(&mut conn).unwrap();
        assert_eq!(kinds, 0);

        // Existing migrated content survives without adding the old defaults.
        let basics: CourseId =
            t::courses::table.filter(t::courses::slug.eq("basics")).select(t::courses::id).first(&mut conn).unwrap();
        let hej: i64 = t::course_words::table
            .filter(t::course_words::course_id.eq(basics))
            .filter(t::course_words::da.eq("hej"))
            .count()
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(hej, 1);
        let dialogues = lessons::for_course(&mut conn, basics).unwrap().iter().filter(|b| b.lesson_type == "dialogue").count();
        assert_eq!(dialogues, 1, "the existing conversation is kept, not doubled");
        assert_eq!(t::courses::table.filter(t::courses::slug.eq("grammar-essentials")).count().get_result::<i64>(&mut conn).unwrap(), 0);

        // A word taken out goes with its history.
        crate::courses::remove_items(&mut conn, colours, &["rød".into()], &[]).unwrap();
        assert!(srs::load(&mut conn, "word", &red_id).unwrap().is_none());
        drop(conn);
        db::open(&path).expect("a second open is a no-op");
    }

    #[test]
    fn finishing_a_lesson_schedules_its_next_check_in() {
        let mut conn = scratch_db("schedule");
        let today = db::today(&mut conn);
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let words = lessons::for_course(&mut conn, basics)
            .unwrap()
            .into_iter()
            .find(|b| b.lesson_type == "words")
            .unwrap();
        let plan = session::build_lesson(&mut conn, words.id).unwrap();
        for exercise in plan.exercises.iter().filter(|e| e.item_kind == "word") {
            let state = srs::load(&mut conn, "word", &exercise.item_id)
                .unwrap()
                .unwrap_or_else(|| srs::ReviewState::fresh(today));
            srs::save(&mut conn, "word", &exercise.item_id, &srs::grade(state, srs::GOOD, today)).unwrap();
            stats::record_answer(&mut conn, today, 6, true).unwrap();
        }

        let done = lessons::record(&mut conn, words.id, lessons::result_from_score(9, 0, 10)).unwrap();
        assert_eq!(done.due_day, Some(today + words.period_days), "first pass: back after its period");
        // The day is done once the rest of today's lessons are.
        for id in plan::plan(&mut conn, today).unwrap().left {
            lessons::record(&mut conn, id, "pass").unwrap();
        }
        assert!(stats::mark_practised(&mut conn, today).unwrap(), "today's lessons done");
        assert!(!stats::mark_practised(&mut conn, today).unwrap(), "not counted twice");

        let summary = stats::collect(&mut conn).unwrap();
        assert_eq!(summary.current_streak, 1);
        assert!(summary.words_seen > 0);
        assert!(summary.achievements.iter().any(|a| a.code == "first_day" && a.unlocked));

        // The next lesson to start is a new one, not the one just done.
        let overview = lessons::overview(&mut conn).unwrap();
        assert!(overview.due.iter().all(|b| b.id != words.id));
        assert!(overview.done_today >= 1);

        // Failing brings it back tomorrow; passing again spreads it out.
        let failed = lessons::record(&mut conn, words.id, "fail").unwrap();
        assert_eq!(failed.due_day, Some(today + 1));
        let passed = lessons::record(&mut conn, words.id, "pass").unwrap();
        assert!(passed.interval_days >= words.period_days);
    }

    #[test]
    fn the_old_built_in_folders_merge_into_other_lessons() {
        let mut conn = scratch_db("other-lessons");
        let real = db::require_course_id(&mut conn, 1).unwrap();
        let kept = lessons::for_course(&mut conn, real).unwrap()[0].id;
        // The three folders of before, each with a lesson.
        let mut moved = Vec::new();
        for (n, slug) in courses::OLD_FOLDERS.iter().enumerate() {
            let folder = courses::ensure(&mut conn, slug, slug, slug, "").unwrap();
            let lesson = lessons::create_study(&mut conn, serde_json::from_value(serde_json::json!({
                "title": format!("Lesson {n}"), "complexity": "low", "content": "x", "objectives": ["y"], "course_id": folder
            })).unwrap(), "tutor").unwrap();
            moved.push(lesson.id);
        }
        courses::merge_old_folders(&mut conn).unwrap();
        courses::merge_old_folders(&mut conn).unwrap();
        let other = courses::other_course(&mut conn).unwrap();
        let in_other: Vec<LessonId> = lessons::for_course(&mut conn, other).unwrap().iter().map(|l| l.id).collect();
        assert!(moved.iter().all(|id| in_other.contains(id)), "every lesson moves, in order: {in_other:?}");
        assert_eq!(lessons::load(&mut conn, kept).unwrap().course_ids, vec![real], "a real course keeps its lessons");
        let left: i64 = t::courses::table.filter(t::courses::slug.eq_any(courses::OLD_FOLDERS)).count().get_result(&mut conn).unwrap();
        assert_eq!(left, 0, "the old folders are gone");
        assert!(courses::status(&mut conn, other).unwrap().open_ended, "a folder that never ends");
    }

    #[test]
    fn the_xp_log_says_what_earned_every_point() {
        let mut conn = scratch_db("xp-log");
        let today = db::today(&mut conn);
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|l| l.lesson_type == "words").unwrap();
        let word: WordId =
            t::words::table.filter(t::words::lesson_id.eq(drill.id)).filter(t::words::da.eq("hej")).select(t::words::id).first(&mut conn).unwrap();
        let answer = |kind: &str, given: &str| practice::SubmitAnswer {
            kind: kind.into(),
            item_kind: "word".into(),
            item_id: word.to_string(),
            expected: "hej".into(),
            given: given.into(),
            lesson_id: Some(drill.id),
            run: "run-1".into(),
            ..Default::default()
        };
        practice::submit_answer_in(&mut conn, answer("intro", "hej")).unwrap();
        practice::submit_answer_in(&mut conn, answer("type", "Hej")).unwrap();
        practice::submit_answer_in(&mut conn, answer("recognise", "nej")).unwrap();

        // A check-in is its own run.
        let starter = starter_lesson(&mut conn);
        let checkin = CheckinId::new();
        diesel::insert_into(t::checkins::table)
            .values((t::checkins::id.eq(checkin), t::checkins::lesson_id.eq(starter.id), t::checkins::started_at.eq(0)))
            .execute(&mut conn)
            .unwrap();
        examiner::record(&mut conn, checkin, serde_json::from_str(r#"{"result":"pass","summary":"Good.","to_review":[]}"#).unwrap()).unwrap();

        let day = xplog::day(&mut conn, today).unwrap();
        assert!(day.detailed);
        assert_eq!(day.runs.len(), 2, "the drill and the check-in");
        assert_eq!(day.runs.iter().map(|r| r.xp).sum::<i64>(), day.total, "the log adds up to the day's total");
        let run = &day.runs[0];
        assert_eq!((run.lesson_id, run.lesson_title.as_str()), (Some(drill.id), drill.title.as_str()));
        let labels: Vec<(&str, i64, &str)> = run.entries.iter().map(|e| (e.label.as_str(), e.xp, e.note.as_str())).collect();
        assert_eq!(labels[0], ("Met “hej”", 0, "read: no XP"));
        assert!(labels[1].0 == "Typed “hej”" && labels[1].1 > 0);
        assert_eq!((labels[2].1, labels[2].2), (0, "a mistake: no XP"));
        assert!(day.runs[1].entries[0].label.starts_with("Check-in: passed"));

        // A day from before the log has its total only.
        stats::add_xp(&mut conn, today - 3, 40).unwrap();
        let old = xplog::day(&mut conn, today - 3).unwrap();
        assert_eq!((old.total, old.detailed), (40, false));
        let history = xplog::history(&mut conn, 30).unwrap();
        assert!(history.iter().any(|d| d.day == today - 3 && d.xp == 40));
        assert!(history.iter().any(|d| d.day == today && d.xp == day.total));
    }

    #[test]
    fn lessons_done_before_the_list_is_chosen_count_toward_it() {
        let mut conn = scratch_db("plan-done-first");
        let today = db::today(&mut conn);
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let first = lessons::for_course(&mut conn, basics).unwrap()[0].id;
        lessons::record(&mut conn, first, "pass").unwrap();
        db::delete_setting(&mut conn, "daily_day").unwrap();
        let plan = plan::plan(&mut conn, today).unwrap();
        assert_eq!(plan.done, 1, "the lesson already done counts");
        assert!(!plan.left.contains(&first));
    }

    #[test]
    fn the_examiner_s_verdict_schedules_the_study_lesson() {
        let mut conn = scratch_db("examiner");
        let starter = starter_lesson(&mut conn);
        // Today asks for this lesson alone: its check-in finishes the day.
        let today = db::today(&mut conn);
        db::set_setting(&mut conn, "daily_day", &today.to_string()).unwrap();
        db::set_setting(&mut conn, "daily_ids", &serde_json::to_string(&[starter.id]).unwrap()).unwrap();
        let checkin = CheckinId::new();
        diesel::insert_into(t::checkins::table)
            .values((t::checkins::id.eq(checkin), t::checkins::lesson_id.eq(starter.id), t::checkins::started_at.eq(0)))
            .execute(&mut conn)
            .unwrap();
        let (assessment, first) = examiner::record(
            &mut conn,
            checkin,
            serde_json::from_str(r#"{"result":"partial","summary":"Mostly right.","to_review":["de er"]}"#).unwrap(),
        )
        .unwrap();
        assert!(first);
        assert!(assessment.xp > 0, "a partial pass still earns something");
        let lesson = lessons::load(&mut conn, starter.id).unwrap();
        assert_eq!(lesson.last_result.as_deref(), Some("partial"));
        assert_eq!(lesson.due_day, Some(db::today(&mut conn) + lesson.period_days));
        let detail = lessons::detail(&mut conn, starter.id).unwrap();
        assert!(detail.last_feedback.unwrap().contains("de er"), "what to review is kept for next time");
    }

    #[test]
    fn the_chat_model_is_never_guessed() {
        let mut conn = scratch_db("model");
        // Model ids go stale, so nothing is assumed on a fresh install.
        assert_eq!(ai::stored_model(&mut conn, "ai_model"), "");
        let error = ai::require_model(&mut conn).unwrap_err().to_string();
        assert!(error.contains("Settings"), "unhelpful message: {error}");

        db::set_setting(&mut conn, "ai_model", "anthropic/claude-haiku-4.5").unwrap();
        assert_eq!(ai::require_model(&mut conn).unwrap(), "anthropic/claude-haiku-4.5");
    }

    #[test]
    fn voices_are_chosen_by_speaker_gender() {
        let mut conn = scratch_db("voices");
        // Speech is pinned, so there are always working voices out of the box.
        assert_eq!(ai::voice_for(&mut conn, "f"), ai::DEFAULT_FEMALE_VOICE);
        assert_eq!(ai::voice_for(&mut conn, "m"), ai::DEFAULT_MALE_VOICE);
        // Anything that is not a dialogue line reads in the female voice.
        assert_eq!(ai::voice_for(&mut conn, ""), ai::DEFAULT_FEMALE_VOICE);

        db::set_setting(&mut conn, "tts_voice_female", "Leda").unwrap();
        db::set_setting(&mut conn, "tts_voice_male", "Charon").unwrap();
        assert_eq!(ai::voice_for(&mut conn, "f"), "Leda");
        assert_eq!(ai::voice_for(&mut conn, "m"), "Charon");
        assert_eq!(ai::voice_for(&mut conn, ""), "Leda", "words and sentences use the chosen voice too");

        // A voice of the wrong gender, or one Gemini does not have, is ignored
        // rather than sent to the API to be rejected.
        db::set_setting(&mut conn, "tts_voice_female", "Charon").unwrap();
        assert_eq!(ai::voice_for(&mut conn, "f"), ai::DEFAULT_FEMALE_VOICE);
        db::set_setting(&mut conn, "tts_voice_male", "NotAVoice").unwrap();
        assert_eq!(ai::voice_for(&mut conn, "m"), ai::DEFAULT_MALE_VOICE);
    }

    #[test]
    fn dialogue_lines_carry_the_speaker_gender() {
        let mut conn = scratch_db("gender");
        let course = db::require_course_id(&mut conn, 4).unwrap(); // the clothes shop
        let lines = session::dialogue_lines(&mut conn, course).unwrap();
        assert!(!lines.is_empty());
        for line in &lines {
            assert!(line.gender == "f" || line.gender == "m", "{:?}", line);
        }
        // The shop dialogue includes both speaker genders.
        let genders: std::collections::HashSet<_> =
            lines.iter().map(|l| l.gender.clone()).collect();
        assert_eq!(genders.len(), 2, "the shop dialogue should use both voices");
    }

    #[test]
    fn song_files_and_speech_move_into_the_files_table() {
        let dir = std::env::temp_dir().join("learn-danish-tests").join(format!("to-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        let on_disk = dir.join("song-1-1.mp3");
        std::fs::write(&on_disk, b"from disk").unwrap();
        {
            // A version-4 database: takes carry their bytes or only a path,
            // and speech sits in audio_cache.
            let mut conn = Conn::establish(&path.to_string_lossy()).unwrap();
            conn.batch_execute(include_str!("../test-fixtures/schema-v5.sql")).unwrap();
            conn.batch_execute(&format!(
                "DROP TABLE playlist_tracks; DROP TABLE song_tracks; DROP TABLE speech_clips; DROP TABLE files;
                 CREATE TABLE song_tracks (
                     id INTEGER PRIMARY KEY, song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
                     ord INTEGER NOT NULL, remote_id TEXT NOT NULL, audio_path TEXT NOT NULL, image_path TEXT,
                     duration REAL NOT NULL DEFAULT 0, play_count INTEGER NOT NULL DEFAULT 0,
                     last_played_at INTEGER, aligned_words TEXT, audio_bytes BLOB, image_bytes BLOB,
                     UNIQUE (song_id, remote_id));
                 CREATE TABLE playlist_tracks (
                     id INTEGER PRIMARY KEY, playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                     track_id INTEGER NOT NULL REFERENCES song_tracks(id) ON DELETE CASCADE, ord INTEGER NOT NULL);
                 CREATE TABLE audio_cache (cache_key TEXT PRIMARY KEY, model TEXT NOT NULL, voice TEXT NOT NULL,
                     text TEXT NOT NULL, mime TEXT NOT NULL, bytes BLOB NOT NULL, created_at INTEGER NOT NULL);
                 INSERT INTO audio_cache VALUES ('k', 'm', 'Kore', 'Hej', 'audio/mpeg', x'01', 0);
                 INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
                     VALUES (1, 'Kaffe', 'Kaffe', 'Coffee', 'pop', '[]', '', 'V5', 'ready', 0);
                 INSERT INTO song_tracks (id, song_id, ord, remote_id, audio_path, image_path, audio_bytes, image_bytes)
                     VALUES (10, 1, 0, 'a', '/gone/song-1-0.mp3', '/gone/song-1-0.png', x'4142', x'89'),
                            (11, 1, 1, 'b', '{disk}', NULL, NULL, NULL),
                            (12, 1, 2, 'c', '/gone/song-1-2.mp3', NULL, NULL, NULL);
                 INSERT INTO playlists (id, name, created_at) VALUES (1, 'Run', 0);
                 INSERT INTO playlist_tracks (playlist_id, track_id, ord) VALUES (1, 10, 0), (1, 11, 1);
                 PRAGMA user_version = 4;",
                disk = on_disk.display()
            ))
            .unwrap();
        }
        let mut conn = db::open(&path).unwrap();
        let tracks: Vec<(TrackId, Option<FileId>, Option<FileId>)> = t::song_tracks::table
            .order(t::song_tracks::id)
            .select((t::song_tracks::id, t::song_tracks::audio_file_id, t::song_tracks::image_file_id))
            .load(&mut conn)
            .unwrap();
        assert_eq!(tracks.len(), 2, "a take with no audio anywhere is not kept");
        let (_, audio, image) = tracks[0];
        let (mime, bytes) = files::read(&mut conn, audio.unwrap()).unwrap();
        assert_eq!((mime.as_str(), bytes.as_slice()), ("audio/mpeg", &b"AB"[..]));
        assert_eq!(files::info(&mut conn, image.unwrap()).unwrap().mime, "image/png");
        let from_disk = files::info(&mut conn, tracks[1].1.unwrap()).unwrap();
        assert_eq!(files::read(&mut conn, from_disk.id).unwrap().1, b"from disk", "a take only on disk is read in");
        assert_eq!(from_disk.path.as_deref(), Some(on_disk.to_str().unwrap()), "its copy stays where it was");

        let in_playlist: i64 = t::playlist_tracks::table.count().get_result(&mut conn).unwrap();
        assert_eq!(in_playlist, 2, "playlists keep their takes");
        assert_eq!(songs::list_songs(&mut conn, false).unwrap()[0].tracks.len(), 2);
        assert_eq!(speech::audio_cache_stats_in(&mut conn).unwrap().clips, 0, "speech is made again");

        // Deleting the song deletes its files.
        let kawa: SongId = t::songs::table.filter(t::songs::title.eq("Kaffe")).select(t::songs::id).first(&mut conn).unwrap();
        songs::delete_song(&mut conn, kawa).unwrap();
        let left: i64 = t::files::table.count().get_result(&mut conn).unwrap();
        assert_eq!(left, 0);
        assert!(!on_disk.exists(), "and their copies on disk");
    }

    #[test]
    fn an_interrupted_tutor_reply_is_marked_for_asking_again() {
        let mut conn = scratch_db("tutor-jobs");
        let id = tutor::create_conversation(&mut conn, "Teach me numbers").unwrap();
        diesel::update(t::tutor_conversations::table.find(id))
            .set(t::tutor_conversations::job_started_at.eq(Some(1)))
            .execute(&mut conn)
            .unwrap();
        let listed = tutor::list_conversations(&mut conn).unwrap();
        assert!(listed[0].working && listed[0].error.is_none());

        tutor::recover_interrupted(&mut conn).unwrap();
        let listed = tutor::list_conversations(&mut conn).unwrap();
        assert!(!listed[0].working);
        assert!(listed[0].error.as_deref().unwrap().contains("Ask again"));
    }

    #[test]
    fn course_pictures_are_prompted_from_the_course_and_go_with_it() {
        let mut conn = scratch_db("pictures");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let prompt = pictures::prompt(&mut conn, basics).unwrap();
        let basics_content: content::Conversation = serde_json::from_str(content::COURSES[0].conversation).unwrap();
        assert!(prompt.contains(&basics_content.title_en) && prompt.contains("No text"), "{prompt}");

        let file = files::store(&mut conn, files::COURSE_IMAGE, "c.png", "image/png", b"png").unwrap();
        diesel::update(t::courses::table.find(basics))
            .set(t::courses::image_file_id.eq(Some(file)))
            .execute(&mut conn)
            .unwrap();
        courses::delete(&mut conn, basics).unwrap();
        assert!(files::info(&mut conn, file).is_err(), "the picture goes with its course");
    }

    /// A tutor turn that makes a course leaves the database free: the tool
    /// runs under the lock and lets go before the picture is drawn (taking
    /// the lock again while holding it waited forever, and the tutor's list
    /// with it). Meanwhile the next model round, a long wait on the network,
    /// holds nothing: the list opens at once throughout.
    #[test]
    fn the_tutor_list_opens_while_a_turn_makes_a_course() {
        let db = std::sync::Arc::new(Db::new(scratch_db("tutor-turn-lock")));
        let turn = {
            let db = db.clone();
            std::thread::spawn(move || {
                let args = serde_json::json!({
                    "title": "Colours", "title_da": "Farver", "summary": "The basic colours.",
                    "words": [{ "da": "rød", "en": "red", "pos": "adj" }]
                });
                let (_, wanted) = tutor::run_app_tool(&db, "create_course", &args.to_string());
                // The next round: the model takes its time.
                std::thread::sleep(std::time::Duration::from_millis(300));
                wanted
            })
        };
        let mut opened = 0;
        while !turn.is_finished() {
            let asked = std::time::Instant::now();
            tutor::list_conversations(&mut db.lock()).unwrap();
            assert!(asked.elapsed() < std::time::Duration::from_millis(250), "the list waited {:?}", asked.elapsed());
            opened += 1;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(opened > 3);
        let wanted = turn.join().unwrap();
        assert_eq!(wanted.len(), 1, "the new course wants a picture");
        // Recorded, so a run that ends before it is drawn draws it next time.
        let conn = &mut *db.lock();
        assert_eq!(pictures::wanted(conn), wanted);
        // The learner deletes it (here: before it was drawn): never drawn again.
        pictures::delete_picture(conn, wanted[0]).unwrap();
        assert!(pictures::wanted(conn).is_empty());
        // Existing courses without a picture are not marked, nor is Other lessons.
        let other = courses::other_course(conn).unwrap();
        assert!(!pictures::want(conn, other).unwrap());
        assert!(pictures::wanted(conn).is_empty());
    }

    // Spec: OFF-3, OFF-5, OFF-6, OFF-11, OFF-15
    #[test]
    fn a_course_kept_offline_knows_what_it_lacks() {
        let mut conn = scratch_db("offline-course");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let before = offline::status(&mut conn, basics).unwrap();
        assert!(!before.enabled && !before.ready, "nothing is kept offline until asked: {before:?}");
        assert!(before.clips > 0 && before.clips_stored == 0);
        assert!(!before.can_download, "no key in a fresh database");
        assert!(offline::kept(&mut conn).unwrap().is_empty());

        offline::set(&mut conn, basics, true).unwrap();
        assert_eq!(offline::kept(&mut conn).unwrap(), vec![basics]);
        // Switching it on again keeps the time it was first asked for.
        let asked: Option<i64> = t::courses::table.find(basics).select(t::courses::offline_at).first(&mut conn).unwrap();
        offline::set(&mut conn, basics, true).unwrap();
        assert_eq!(t::courses::table.find(basics).select(t::courses::offline_at).first::<Option<i64>>(&mut conn).unwrap(), asked);

        // Each word's example sentences: what they say is audio to have too.
        let course_words: Vec<WordId> =
            t::course_words::table.filter(t::course_words::course_id.eq(basics)).select(t::course_words::id).load(&mut conn).unwrap();
        assert_eq!((before.examples, before.examples_stored), (convert::count(course_words.len()), 0));
        assert!(before.needs_model, "no chat model to write them with");
        for (n, word) in course_words.iter().enumerate() {
            diesel::insert_into(t::word_examples::table)
                .values((
                    t::word_examples::word_id.eq(word),
                    t::word_examples::examples.eq(format!(r#"[{{"da":"Eksempel nummer {n}.","en":"Example number {n}."}}]"#)),
                    t::word_examples::created_at.eq(0),
                ))
                .execute(&mut conn)
                .unwrap();
        }
        let with_examples = offline::status(&mut conn, basics).unwrap();
        assert_eq!(with_examples.examples_stored, with_examples.examples);
        assert!(!with_examples.needs_model, "nothing left to write");
        let lines = speech::course_audio_lines_in(&mut conn, basics).unwrap();
        assert!(lines.iter().any(|line| line.text == "Eksempel nummer 0."), "the examples are said too");
        // Every line counts twice: at normal speed and slowly.
        assert_eq!(with_examples.clips, 2 * convert::count(lines.len()));
        assert_eq!(with_examples.clips, before.clips + 2 * convert::count(course_words.len()));

        // Every line stored at normal speed, in the voice that reads it: half.
        let store = |conn: &mut Conn, slow: bool| {
            for line in &lines {
                let voice = ai::voice_for(conn, &line.gender);
                let voice = if slow { format!("{voice}/slow") } else { voice };
                let key = speech::audio_key(ai::SPEECH_MODEL, &voice, &line.text);
                speech::store_clip(conn, &key, ai::SPEECH_MODEL, &voice, &line.text, "audio/mpeg", b"mp3").unwrap();
            }
        };
        store(&mut conn, false);
        let half = offline::status(&mut conn, basics).unwrap();
        assert_eq!(half.clips_stored * 2, half.clips);
        assert!(!half.ready, "the slow versions are part of it");
        store(&mut conn, true);
        let after = offline::status(&mut conn, basics).unwrap();
        assert!(after.enabled && after.ready, "{after:?}");
        assert_eq!((after.clips_stored, after.pictures), (after.clips, 0));

        // A word with a neighbour by sound adds that word to what is said,
        // and a picture lesson's words count as pictures until drawn.
        let (word, lesson): (WordId, LessonId) = t::course_words::table
            .filter(t::course_words::course_id.eq(basics))
            .select((t::course_words::id, t::course_words::lesson_id))
            .first(&mut conn)
            .unwrap();
        diesel::update(t::words::table.find(word)).set(t::words::near_da.eq("zzznabo")).execute(&mut conn).unwrap();
        let grown = offline::status(&mut conn, basics).unwrap();
        assert_eq!(grown.clips, after.clips + 2, "the neighbour, at both speeds");
        assert!(!grown.ready, "a course that grew is not ready until the new audio is there");
        diesel::update(t::lessons::table.find(lesson)).set(t::lessons::lesson_type.eq("pictures")).execute(&mut conn).unwrap();
        let words_in_lesson: i64 = t::words::table.filter(t::words::lesson_id.eq(lesson)).count().get_result(&mut conn).unwrap();
        let pictured = offline::status(&mut conn, basics).unwrap();
        assert_eq!((pictured.pictures, pictured.pictures_stored), (words_in_lesson, 0));
        let picture = files::store(&mut conn, files::WORD_IMAGE, "w.jpg", "image/jpeg", b"jpg").unwrap();
        diesel::update(t::words::table.find(word)).set(t::words::image_file_id.eq(Some(picture))).execute(&mut conn).unwrap();
        assert_eq!(offline::status(&mut conn, basics).unwrap().pictures_stored, 1);

        // Off again: nothing downloaded is deleted.
        offline::set(&mut conn, basics, false).unwrap();
        let off = offline::status(&mut conn, basics).unwrap();
        assert!(!off.enabled);
        assert_eq!(off.clips_stored, after.clips_stored);
    }

    // Spec: SPEECH-9, SPEECH-11, SPEECH-12
    #[test]
    fn speech_stored_as_wav_becomes_mp3() {
        let mut conn = scratch_db("speech-mp3");
        let wav = audio::tests::speech_wav();
        // A new clip is encoded as it is stored.
        let key = speech::audio_key("m", "Kore", "Hej");
        speech::store_clip(&mut conn, &key, "m", "Kore", "Hej", "audio/wav", &wav).unwrap();
        let (mime, size): (String, i64) = t::speech_clips::table
            .inner_join(t::files::table)
            .select((t::files::mime, t::files::size))
            .first(&mut conn)
            .unwrap();
        assert_eq!(mime, "audio/mpeg");
        assert!(size * 7 < convert::count(wav.len()));

        // From before: a clip and a podcast episode stored as WAV, the
        // episode with its copy on disk; and a song, which is MP3 already.
        let dir = std::env::temp_dir().join("learn-danish-tests").join(format!("speech-mp3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old_clip = files::store(&mut conn, files::SPEECH, "speech-1.wav", "audio/wav", &wav).unwrap();
        let old_key = speech::audio_key("m", "Kore/pcm", "Goddag");
        diesel::insert_into(t::speech_clips::table)
            .values((
                t::speech_clips::cache_key.eq(&old_key),
                t::speech_clips::model.eq("m"),
                t::speech_clips::voice.eq("Kore/pcm"),
                t::speech_clips::text.eq("Goddag"),
                t::speech_clips::file_id.eq(old_clip),
                t::speech_clips::created_at.eq(0),
            ))
            .execute(&mut conn)
            .unwrap();
        let show = SongId::derived("test", "show");
        conn.batch_execute(&format!(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
             VALUES ('{show}', 'Show', '', '', '', '[]', '', '', 'ready', 0);",
        ))
        .unwrap();
        let old_episode = files::store(&mut conn, files::SONG_AUDIO, "podcast-1.wav", "audio/wav", &wav).unwrap();
        let song = files::store(&mut conn, files::SONG_AUDIO, "song-1-0.mp3", "audio/mpeg", b"mp3").unwrap();
        for (remote, file) in [("episode", old_episode), ("r1", song)] {
            diesel::insert_into(t::song_tracks::table)
                .values((
                    t::song_tracks::id.eq(TrackId::new()),
                    t::song_tracks::song_id.eq(show),
                    t::song_tracks::ord.eq(0),
                    t::song_tracks::remote_id.eq(remote),
                    t::song_tracks::audio_file_id.eq(file),
                ))
                .execute(&mut conn)
                .unwrap();
        }
        let old_copy = files::materialize(&mut conn, &dir, old_episode).unwrap();
        // One that is not audio at all stays as it is, and stops nothing.
        let odd = files::store(&mut conn, files::SPEECH, "odd.wav", "audio/wav", b"not a wav file").unwrap();

        let db = db::Db::new(conn);
        assert_eq!(audio::compress_in(&db, &dir).unwrap(), 2);
        assert_eq!(audio::compress_in(&db, &dir).unwrap(), 0, "nothing left to do");
        let conn = &mut *db.lock();

        // The clip and the take point at new, smaller files; the old are gone.
        let clip: FileId = t::speech_clips::table.find(&old_key).select(t::speech_clips::file_id).first(conn).unwrap();
        let episode: FileId = t::song_tracks::table
            .filter(t::song_tracks::remote_id.eq("episode"))
            .select(t::song_tracks::audio_file_id.assume_not_null())
            .first(conn)
            .unwrap();
        for (new, old, name) in [(clip, old_clip, "speech-1.mp3"), (episode, old_episode, "podcast-1.mp3")] {
            assert_ne!(new, old);
            assert!(files::info(conn, old).is_err());
            let file = files::info(conn, new).unwrap();
            assert_eq!((file.name.as_str(), file.mime.as_str()), (name, "audio/mpeg"));
            assert!(file.size * 7 < convert::count(wav.len()));
            assert!(audio::wav_from_mp3(&files::read(conn, new).unwrap().1).is_ok(), "it plays");
        }
        // The episode's copy on disk is the new one.
        assert!(!std::path::Path::new(&old_copy).exists());
        let copy = files::info(conn, episode).unwrap().path.unwrap();
        assert!(copy.ends_with("podcast-1.mp3") && std::path::Path::new(&copy).exists());
        // The song and the odd file are untouched.
        assert_eq!(files::read(conn, song).unwrap().1, b"mp3");
        assert_eq!(files::read(conn, odd).unwrap().1, b"not a wav file");
        assert_eq!(db::foreign_key_violations(conn).unwrap(), 0);
    }

    // Spec: DATA-7, DATA-8
    #[test]
    fn files_keep_referential_integrity() {
        let mut conn = scratch_db("file-integrity");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        // A picture, a song take with art, a speech clip, and a stray file.
        let picture = files::store(&mut conn, files::COURSE_IMAGE, "c.png", "image/png", b"p").unwrap();
        diesel::update(t::courses::table.find(basics)).set(t::courses::image_file_id.eq(Some(picture))).execute(&mut conn).unwrap();
        let kawa = SongId::derived("test", "kawa");
        conn.batch_execute(&format!(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
             VALUES ('{kawa}', 'Kaffe', 'Kaffe', 'Coffee', 'pop', '[]', '', 'V6', 'ready', 0);",
        ))
        .unwrap();
        let audio = files::store(&mut conn, files::SONG_AUDIO, "a.mp3", "audio/mpeg", b"a").unwrap();
        let art = files::store(&mut conn, files::SONG_IMAGE, "a.png", "image/png", b"i").unwrap();
        diesel::insert_into(t::song_tracks::table)
            .values((
                t::song_tracks::id.eq(TrackId::new()),
                t::song_tracks::song_id.eq(kawa),
                t::song_tracks::ord.eq(0),
                t::song_tracks::remote_id.eq("r"),
                t::song_tracks::audio_file_id.eq(audio),
                t::song_tracks::image_file_id.eq(art),
            ))
            .execute(&mut conn)
            .unwrap();
        let key = speech::audio_key("m", "Kore", "Hej");
        speech::store_clip(&mut conn, &key, "m", "Kore", "Hej", "audio/mpeg", b"s").unwrap();
        let stray = files::store(&mut conn, files::SPEECH, "x.mp3", "audio/mpeg", b"x").unwrap();

        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
        // A file a take still plays cannot be deleted from under it.
        assert!(files::delete(&mut conn, &[audio]).is_err(), "the take's audio is protected");
        assert!(files::info(&mut conn, audio).is_ok(), "and it is all still there");
        // The stray file is the only orphan, and the sweep takes it.
        assert_eq!(files::collect_orphans(&mut conn).unwrap(), 1);
        assert!(files::info(&mut conn, stray).is_err());

        // Deleting what owns files deletes them; nothing dangles afterwards.
        songs::delete_song(&mut conn, kawa).unwrap();
        courses::delete(&mut conn, basics).unwrap();
        diesel::delete(t::speech_clips::table).execute(&mut conn).unwrap();
        assert_eq!(files::collect_orphans(&mut conn).unwrap(), 1, "the clip's file, once its clip was removed by hand");
        let left: i64 = t::files::table.count().get_result(&mut conn).unwrap();
        assert_eq!(left, 0);
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
    }

    #[test]
    fn a_typed_answer_one_letter_off_is_almost() {
        let mut conn = scratch_db("almost");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        let words: Vec<WordId> = t::words::table.filter(t::words::lesson_id.eq(drill.id)).select(t::words::id).limit(4).load(&mut conn).unwrap();
        let answer = |word: WordId, kind: &str, expected: &str, given: &str, extra: bool| practice::SubmitAnswer {
            kind: kind.into(),
            item_kind: "word".into(),
            item_id: word.to_string(),
            expected: expected.into(),
            given: given.into(),
            extra,
            lesson_id: Some(drill.id),
            ..Default::default()
        };
        let lapses = |conn: &mut db::Conn, word: WordId| srs::load(conn, "word", &word.to_string()).unwrap().unwrap().lapses;

        // "spiser" for "spise": wrong, but almost — half XP, back sooner, not over.
        let slip = practice::submit_answer_in(&mut conn, answer(words[0], "type", "spise", "spiser", false)).unwrap();
        assert!(!slip.correct && slip.almost);
        assert_eq!(slip.xp_gained, (stats::xp_for_answer("type", true, false) + 1) / 2);
        assert_eq!(lapses(&mut conn, words[0]), 0, "graded HARD, not AGAIN");
        let note: String = t::xp_log::table.order(t::xp_log::at.desc()).select(t::xp_log::note).first(&mut conn).unwrap();
        assert_eq!(note, "almost: half XP");
        let logged: i64 = t::attempts::table.filter(t::attempts::correct.eq(0)).count().get_result(&mut conn).unwrap();
        assert_eq!(logged, 1, "still a mistake in the learning log");
        // Extra practice halves it again; an alternative gloss counts too.
        let extra = practice::submit_answer_in(&mut conn, answer(words[0], "type", "eat, spise", "spis", true)).unwrap();
        assert!(extra.almost);
        assert_eq!(extra.xp_gained, ((stats::xp_for_answer("type", true, false) + 1) / 2 + 1) / 2);

        // A missing å is one letter off too (it folds to "aa"), but only in
        // a word long enough: "bla" for "blå" is not counted.
        assert!(practice::submit_answer_in(&mut conn, answer(words[1], "type", "dårlig", "darlig", false)).unwrap().almost);
        assert!(!practice::submit_answer_in(&mut conn, answer(words[2], "type", "blå", "bla", false)).unwrap().almost);
        // Too short: one letter off "hus" is another word.
        let short = practice::submit_answer_in(&mut conn, answer(words[1], "type", "hus", "hos", false)).unwrap();
        assert!(!short.correct && !short.almost);
        assert_eq!(short.xp_gained, 0);
        assert_eq!(lapses(&mut conn, words[1]), 1, "started over");
        // A pick is never a slip, however close.
        let picked = practice::submit_answer_in(&mut conn, answer(words[2], "recognise", "spise", "spiser", false)).unwrap();
        assert!(!picked.correct && !picked.almost);
        assert_eq!(picked.xp_gained, 0);
        // A blank is not almost anything.
        let blank = practice::submit_answer_in(&mut conn, answer(words[3], "type", "spise", "", false)).unwrap();
        assert!(!blank.correct && !blank.almost);
        assert_eq!(lapses(&mut conn, words[3]), 1);

        // The drill's score counts an almost as half right: 8 right and 2
        // almost of 10 is 90%, a pass; 8 of 10 alone would be a partial.
        let score = |almost| practice::Score { correct: 8, almost, total: 10 };
        let finished = practice::lesson_finish_with(&mut conn, drill.id, score(2), None).unwrap();
        assert_eq!((finished.result.as_str(), finished.almost), ("pass", 2));
        let other = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.reps == 0 && b.id != drill.id).unwrap();
        let partial = practice::lesson_finish_with(&mut conn, other.id, score(0), None).unwrap();
        assert_eq!(partial.result, "partial");
        let details: String = t::learning_events::table
            .filter(t::learning_events::kind.eq("lesson_finished"))
            .filter(t::learning_events::lesson_id.eq(drill.id))
            .select(t::learning_events::details)
            .first(&mut conn)
            .unwrap();
        assert!(details.contains("\"almost\":2"), "{details}");
    }

    #[test]
    fn a_drill_with_nothing_scored_is_not_graded() {
        let mut conn = scratch_db("ungraded");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        let empty = practice::lesson_finish_in(&mut conn, drill.id, 0, 0).unwrap();
        assert!(!empty.graded && empty.mastery.is_none());
        assert_eq!((empty.lesson.mastery, empty.lesson.passed, empty.lesson.due_day, empty.lesson.reps), (0, false, None, 0));
        assert!(crate::masterylog::for_lesson(&mut conn, drill.id).unwrap().entries.is_empty());
        // More right than scored is capped: 12 of 10 is 10 of 10.
        let capped = practice::lesson_finish_in(&mut conn, drill.id, 12, 10).unwrap();
        assert_eq!((capped.correct, capped.total, capped.result.as_str()), (10, 10, "pass"));
        assert_eq!(capped.mastery.unwrap().text, "Ace 10 of 10, first go: +38 (0 → 38)");
    }

    #[test]
    fn a_fail_on_extra_practice_costs_no_mastery() {
        let mut conn = scratch_db("extra-fail");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
        let failed = practice::lesson_finish_in(&mut conn, drill.id, 2, 10).unwrap();
        assert!(failed.extra);
        assert_eq!((failed.result.as_str(), failed.lesson.mastery), ("fail", 30), "no change");
        let row = failed.mastery.unwrap();
        assert_eq!((row.reason.as_str(), row.delta), ("fail_extra", 0));
        assert_eq!(row.text, "Fail 2 of 10, same day on extra practice: no change");
        // A real check-in's fail still costs: the next day it is not extra.
        conn.batch_execute("UPDATE lessons SET last_day = last_day - 1; UPDATE mastery_log SET day = day - 1;").unwrap();
        let real = practice::lesson_finish_in(&mut conn, drill.id, 2, 10).unwrap();
        assert!(!real.extra);
        assert_eq!(real.lesson.mastery, 10);
    }

    #[test]
    fn nearly_right_dictation_and_speech_count_as_almost() {
        let mut conn = scratch_db("almost-sentences");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let sentence: SentenceId = t::course_sentences::table
            .filter(t::course_sentences::course_id.eq(basics))
            .select(t::course_sentences::id)
            .first(&mut conn)
            .unwrap();
        let answer = |kind: &str, given: &str| practice::SubmitAnswer {
            kind: kind.into(),
            item_kind: "sentence".into(),
            item_id: sentence.to_string(),
            expected: "Jeg vil gerne have kaffe.".into(),
            given: given.into(),
            ..Default::default()
        };
        let lapses = |conn: &mut Conn| srs::load(conn, "sentence", &sentence.to_string()).unwrap().map_or(0, |r| r.lapses);
        let written = practice::submit_answer_in(&mut conn, answer("dictation", "jeg vil gerne have kafe")).unwrap();
        assert!(!written.correct && written.almost, "one word off");
        assert_eq!(written.xp_gained, 7, "half of 14");
        assert_eq!(lapses(&mut conn), 0, "nearly: not started over");
        let said = practice::submit_answer_in(&mut conn, answer("sayIt", "jeg vil have kaffe")).unwrap();
        assert!(!said.correct && said.almost, "one word not heard");
        assert_eq!(said.xp_gained, 7);
        let off = practice::submit_answer_in(&mut conn, answer("dictation", "jeg vil gerne drikke te")).unwrap();
        assert!(!off.correct && !off.almost && off.xp_gained == 0, "two words off is a miss");
        assert_eq!(lapses(&mut conn), 1);
    }

    #[test]
    fn a_day_is_done_by_doing_the_work_not_by_xp() {
        let mut conn = scratch_db("work-day");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let today = db::today(&mut conn);
        let lesson = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        // Today's list is that one lesson.
        db::set_setting(&mut conn, "daily_ids", &serde_json::to_string(&[lesson.id]).unwrap()).unwrap();
        db::set_setting(&mut conn, "daily_day", &today.to_string()).unwrap();
        assert!(!stats::practised_on(&mut conn, today).unwrap());
        // A failed go earns no XP, but the work was done: the day is.
        lessons::record(&mut conn, lesson.id, "fail").unwrap();
        assert_eq!(stats::day_xp(&mut conn, today).unwrap(), 0);
        assert!(stats::day_is_done(&mut conn, today).unwrap());
        assert!(stats::mark_practised(&mut conn, today).unwrap(), "done the moment it is marked");

        // A quick game marks the day like a drill, even past the day's game XP.
        let mut conn = scratch_db("work-day-game");
        let today = db::today(&mut conn);
        let lesson = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        db::set_setting(&mut conn, "daily_ids", &serde_json::to_string(&[lesson.id]).unwrap()).unwrap();
        db::set_setting(&mut conn, "daily_day", &today.to_string()).unwrap();
        diesel::update(t::lessons::table.find(lesson.id)).set(t::lessons::last_day.eq(Some(today))).execute(&mut conn).unwrap();
        let completed = |conn: &mut Conn| t::day_totals::table.find(today).select(t::day_totals::completed).first::<i64>(conn).optional().unwrap();
        assert_eq!(completed(&mut conn), None);
        for _ in 0..4 {
            crate::games::finish(&mut conn, "match", 8, 40_000, 0).unwrap();
        }
        assert_eq!(completed(&mut conn), Some(1));
        assert_eq!(stats::day_xp(&mut conn, today).unwrap(), 30, "the fourth game paid nothing, and still counted");
    }

    // Spec: DRILL-8
    #[test]
    fn an_ace_is_every_answer_right() {
        let mut conn = scratch_db("ace");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drills: Vec<_> = lessons::for_course(&mut conn, basics).unwrap().into_iter().filter(|b| b.lesson_type == "words").take(2).collect();
        // One miss in twenty passes, and is no ace (DRILL-8): a first pass earns 30.
        practice::lesson_finish_in(&mut conn, drills[0].id, 19, 20).unwrap();
        assert_eq!(lessons::load(&mut conn, drills[0].id).unwrap().mastery, 30);
        // Every answer right is: a quarter more.
        practice::lesson_finish_in(&mut conn, drills[1].id, 20, 20).unwrap();
        assert_eq!(lessons::load(&mut conn, drills[1].id).unwrap().mastery, 38);
    }

    // Spec: TODAY-6, TODAY-7
    #[test]
    fn a_mastered_lesson_with_due_words_comes_back_as_a_light_review() {
        let mut conn = scratch_db("review-slot");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let today = db::today(&mut conn);
        db::set_setting(&mut conn, "daily_lessons", "20").unwrap();
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        diesel::update(t::lessons::table.find(drill.id))
            .set((t::lessons::passed.eq(1), t::lessons::mastery.eq(100), t::lessons::last_day.eq(Some(today - 30)), t::lessons::due_day.eq(Some(today + 300))))
            .execute(&mut conn)
            .unwrap();
        lessons::stamp_mastered(&mut conn, drill.id).unwrap();
        let words: Vec<WordId> = t::words::table.filter(t::words::lesson_id.eq(drill.id)).select(t::words::id).load(&mut conn).unwrap();
        let mut state = srs::ReviewState::fresh(today - 40);
        state.reps = 4;
        state.interval = 20;
        state.due_day = today + 5;
        for id in &words {
            srs::save(&mut conn, "word", &id.to_string(), &state).unwrap();
        }
        // No word due: it stays out of the day.
        assert!(!plan::plan(&mut conn, today).unwrap().left.contains(&drill.id));
        // One due: a spare place takes it — once.
        db::set_setting(&mut conn, "daily_day", "-1").unwrap();
        srs::save(&mut conn, "word", &words[0].to_string(), &srs::ReviewState { due_day: today - 1, ..state }).unwrap();
        let day = plan::plan(&mut conn, today).unwrap();
        assert_eq!(day.left.iter().filter(|id| **id == drill.id).count(), 1, "{day:?}");
        // A miss there costs nothing: mastery is for good, the word just comes back.
        let done = practice::lesson_finish_in(&mut conn, drill.id, 0, 10).unwrap();
        assert_eq!((done.lesson.mastered, done.lesson.mastery), (true, 100));
        assert_eq!(done.mastery.unwrap().reason, "after_mastered");
        assert!(!plan::plan(&mut conn, today).unwrap().left.contains(&drill.id), "done for today");
        // No room, no review.
        db::set_setting(&mut conn, "daily_lessons", "1").unwrap();
        db::set_setting(&mut conn, "daily_day", "-1").unwrap();
        diesel::update(t::lessons::table.find(drill.id)).set(t::lessons::last_day.eq(Some(today - 1))).execute(&mut conn).unwrap();
        let tight = plan::plan(&mut conn, today).unwrap();
        assert!(tight.total <= 1);
    }

    #[test]
    fn a_version_15_database_upgrades_to_the_current_version_keeping_every_row() {
        let path = std::env::temp_dir().join("learn-danish-tests").join(format!("v15-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let tables = ["lessons", "words", "sentences", "reviews", "day_log", "xp_log", "attempts", "learning_events", "settings", "courses"];
        let counts = |conn: &mut Conn| -> Vec<i64> {
            tables
                .iter()
                .map(|t| {
                    #[derive(diesel::QueryableByName)]
                    struct N {
                        #[diesel(sql_type = diesel::sql_types::BigInt)]
                        n: i64,
                    }
                    diesel::sql_query(format!("SELECT count(*) AS n FROM {t}")).get_result::<N>(conn).unwrap().n
                })
                .collect()
        };
        let before = {
            let mut conn = db::open(&path).unwrap();
            db::seed_test_courses(&mut conn).unwrap();
            db::unify_courses(&mut conn).unwrap();
            let basics = db::require_course_id(&mut conn, 1).unwrap();
            let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
            let word: WordId = t::words::table.filter(t::words::lesson_id.eq(drill.id)).select(t::words::id).first(&mut conn).unwrap();
            practice::submit_answer_in(
                &mut conn,
                practice::SubmitAnswer { kind: "type".into(), item_kind: "word".into(), item_id: word.to_string(), expected: "a".into(), given: "a".into(), lesson_id: Some(drill.id), ..Default::default() },
            )
            .unwrap();
            practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
            // Back to version 15: no ledger yet, and the lesson's 30 points
            // from before it.
            conn.batch_execute("DROP TABLE mastery_log; DELETE FROM sync_rows WHERE tbl = 'mastery_log'; PRAGMA user_version = 15;").unwrap();
            (counts(&mut conn), drill.id)
        };
        let mut conn = db::open(&path).unwrap();
        #[derive(diesel::QueryableByName)]
        struct V {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            user_version: i64,
        }
        assert_eq!(diesel::sql_query("PRAGMA user_version").get_result::<V>(&mut conn).unwrap().user_version, db::SCHEMA_VERSION);
        assert_eq!(counts(&mut conn), before.0, "every row kept");
        // Version 17: no course is kept offline until the learner asks.
        assert!(offline::kept(&mut conn).unwrap().is_empty());
        let log = crate::masterylog::for_lesson(&mut conn, before.1).unwrap();
        assert!(log.entries.is_empty(), "no backfill");
        assert_eq!(log.before_log, Some(30), "its points from before the log");
        // The new table syncs like the rest: writing to it is recorded.
        practice::lesson_finish_in(&mut conn, before.1, 9, 10).unwrap();
        #[derive(diesel::QueryableByName)]
        struct N {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            n: i64,
        }
        let synced = diesel::sql_query("SELECT count(*) AS n FROM sync_rows WHERE tbl = 'mastery_log'").get_result::<N>(&mut conn).unwrap().n;
        assert!(synced >= 1, "{synced}");
        let backups = std::fs::read_dir(path.parent().unwrap().join("backups")).unwrap().filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().starts_with(&format!("before-v{}", db::SCHEMA_VERSION)));
        assert!(backups, "backed up before upgrading");
    }

    #[test]
    fn an_install_at_version_17_keeps_a_shared_lesson_in_the_course_it_was_first_added_to() {
        #[derive(diesel::QueryableByName)]
        struct N {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            n: i64,
        }
        let count = |conn: &mut Conn, sql: &str| diesel::sql_query(sql).get_result::<N>(conn).unwrap().n;
        let path = std::env::temp_dir().join("learn-danish-tests").join(format!("v17-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let tables = ["lessons", "words", "sentences", "reviews", "xp_log", "mastery_log", "attempts", "courses", "checkins", "settings"];
        let counts = |conn: &mut Conn| -> Vec<i64> { tables.iter().map(|t| count(conn, &format!("SELECT count(*) AS n FROM {t}"))).collect() };
        let (before, listings, lesson, first, later, progress) = {
            let mut conn = db::open(&path).unwrap();
            db::seed_test_courses(&mut conn).unwrap();
            db::unify_courses(&mut conn).unwrap();
            // Three courses, made at 100, 200 and 300; the lesson made at 250
            // in the second, then listed in the first and the third too, as
            // version 17 allowed.
            let made: Vec<CourseId> = [("Older", 100), ("Its own", 200), ("Newer", 300)]
                .iter()
                .map(|(title, at)| {
                    let id = courses::create_empty(&mut conn, title, "").unwrap();
                    diesel::update(t::courses::table.find(id)).set(t::courses::created_at.eq(*at)).execute(&mut conn).unwrap();
                    id
                })
                .collect();
            let items = courses::Items {
                words: vec![courses::NewWord { da: "godmorgen".into(), en: "good morning".into(), pos: "phr".into(), ..Default::default() }],
                ..Default::default()
            };
            let (lesson, _) = lessons::create_drill(&mut conn, made[1], "Shared", "", "words", "low", &items, None, "tutor").unwrap();
            diesel::update(t::lessons::table.find(lesson.id)).set(t::lessons::created_at.eq(250)).execute(&mut conn).unwrap();
            conn.batch_execute("DROP INDEX course_lessons_one_course; PRAGMA user_version = 17;").unwrap();
            for (n, course) in [made[0], made[2]].iter().enumerate() {
                diesel::insert_into(t::course_lessons::table)
                    .values((t::course_lessons::course_id.eq(course), t::course_lessons::lesson_id.eq(lesson.id), t::course_lessons::ord.eq(crate::convert::count(n) + 1)))
                    .execute(&mut conn)
                    .unwrap();
            }
            // Practised: a schedule, mastery points and their ledger.
            practice::lesson_finish_in(&mut conn, lesson.id, 10, 10).unwrap();
            let progress: (i64, i64) = t::lessons::table.find(lesson.id).select((t::lessons::reps, t::lessons::mastery)).first(&mut conn).unwrap();
            assert!(progress.1 > 0);
            let listings = count(&mut conn, "SELECT count(*) AS n FROM course_lessons");
            (counts(&mut conn), listings, lesson.id, made[1], [made[0], made[2]], progress)
        };
        let mut conn = db::open(&path).unwrap();
        #[derive(diesel::QueryableByName)]
        struct V {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            user_version: i64,
        }
        assert_eq!(diesel::sql_query("PRAGMA user_version").get_result::<V>(&mut conn).unwrap().user_version, db::SCHEMA_VERSION);
        // Only the course it was first added to keeps it.
        assert_eq!(lessons::load(&mut conn, lesson).unwrap().course_ids, vec![first]);
        assert_eq!(count(&mut conn, "SELECT count(*) AS n FROM course_lessons"), listings - 2, "only the extra listings go");
        assert_eq!(counts(&mut conn), before, "every other row kept, the ledger too");
        let after: (i64, i64) = t::lessons::table.find(lesson).select((t::lessons::reps, t::lessons::mastery)).first(&mut conn).unwrap();
        assert_eq!(after, progress, "its schedule and mastery stay");
        for course in later {
            assert!(lessons::for_course(&mut conn, course).unwrap().is_empty());
        }
        // The listings that went are deletes other devices hear of.
        let tombstones = count(
            &mut conn,
            &format!("SELECT count(*) AS n FROM sync_rows WHERE tbl = 'course_lessons' AND deleted = 1 AND pk LIKE '%{lesson}'"),
        );
        assert_eq!(tombstones, 2);
        // And it stays so.
        assert!(courses::add_lesson(&mut conn, later[0], lesson).is_err());
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);
        let backups = std::fs::read_dir(path.parent().unwrap().join("backups")).unwrap().filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().starts_with(&format!("before-v{}", db::SCHEMA_VERSION)));
        assert!(backups, "backed up before upgrading");
    }

    #[test]
    fn a_partial_earns_half_a_pass_of_mastery() {
        let mut conn = scratch_db("partial-mastery");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();
        // A partial moves mastery, without passing the lesson.
        let first = practice::lesson_finish_in(&mut conn, drill.id, 7, 10).unwrap();
        assert_eq!(first.result, "partial");
        assert!(!first.lesson.passed, "a partial does not pass the lesson");
        assert_eq!(first.lesson.mastery, 5, "half a repeat pass the same day (10), never more than a pass");
        let again = practice::lesson_finish_in(&mut conn, drill.id, 7, 10).unwrap();
        assert_eq!(again.lesson.mastery, 10, "partials in a row don't race ahead");
        let pass = practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
        assert!(pass.lesson.passed);
        assert_eq!(pass.lesson.mastery, 40);
        // Five partials and a pass no longer master a lesson (they once made 105).
        for _ in 0..5 {
            practice::lesson_finish_in(&mut conn, drill.id, 7, 10).unwrap();
        }
        let lesson = lessons::load(&mut conn, drill.id).unwrap();
        assert!(!lesson.mastered && lesson.mastery <= 90, "{}", lesson.mastery);
        let log = crate::masterylog::for_lesson(&mut conn, drill.id).unwrap();
        assert_eq!(log.entries.len(), 8, "every go has its row");
        assert_eq!(log.entries.last().unwrap().reason, "partial");
        assert!(log.entries.iter().any(|e| e.reason == "first_pass" && e.delta == 30));
        assert_eq!(log.before_log, None);
    }

    #[test]
    fn gamification_counts_only_what_was_really_done() {
        let mut conn = scratch_db("game-integrity");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics)
            .unwrap()
            .into_iter()
            .find(|b| b.lesson_type == "words")
            .unwrap();
        let word: WordId = t::words::table.filter(t::words::lesson_id.eq(drill.id)).select(t::words::id).first(&mut conn).unwrap();
        let answer = |kind: &str, extra: bool| practice::SubmitAnswer {
            kind: kind.into(),
            item_kind: "word".into(),
            item_id: word.to_string(),
            is_new: true,
            expected: "kat".into(),
            given: "kat".into(),
            grade: None,
            extra,
            ..Default::default()
        };
        let reviews_of_word = |conn: &mut db::Conn| -> i64 {
            t::reviews::table.filter(t::reviews::kind.eq("word")).filter(t::reviews::item_id.eq(word.to_string())).count().get_result(conn).unwrap()
        };

        // Reading a new word's card earns nothing and is not a review.
        let read = practice::submit_answer_in(&mut conn, answer("intro", false)).unwrap();
        assert_eq!(read.xp_gained, 0);
        assert_eq!(reviews_of_word(&mut conn), 0, "seeing a word is not reviewing it");
        assert_eq!(stats::collect(&mut conn).unwrap().accuracy, 0.0, "nor an answer");

        let answered = practice::submit_answer_in(&mut conn, answer("recognise", false)).unwrap();
        assert_eq!(answered.xp_gained, 8);
        assert_eq!(reviews_of_word(&mut conn), 1);

        // The first time schedules the lesson; doing it again the same day
        // does not push it further out, and pays half.
        let first = practice::lesson_finish_in(&mut conn, drill.id, 10, 10).unwrap();
        assert!(!first.extra);
        let due = first.lesson.due_day;
        let again = practice::lesson_finish_in(&mut conn, drill.id, 10, 10).unwrap();
        assert!(again.extra);
        assert_eq!(again.lesson.due_day, due, "extra practice leaves the schedule alone");
        let half = practice::submit_answer_in(&mut conn, answer("recognise", true)).unwrap();
        assert_eq!(half.xp_gained, 4);

        // A word takes its history with it.
        lessons::delete(&mut conn, drill.id).unwrap();
        assert_eq!(reviews_of_word(&mut conn), 0, "no orphaned reviews");

        // A check-in's summary lists only what unlocked since it began.
        let study = lessons::for_course(&mut conn, basics)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let now = db::now_secs();
        diesel::insert_into(t::checkins::table)
            .values((t::checkins::id.eq(CheckinId::derived("test", "77")), t::checkins::lesson_id.eq(study.id), t::checkins::started_at.eq(now - 10)))
            .execute(&mut conn)
            .unwrap();
        diesel::insert_into(t::achievements::table)
            .values(vec![
                (t::achievements::code.eq("first_day"), t::achievements::unlocked_day.eq(0), t::achievements::unlocked_at.eq(now - 100)),
                (t::achievements::code.eq("streak_3"), t::achievements::unlocked_day.eq(0), t::achievements::unlocked_at.eq(now)),
            ])
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .unwrap();
        // The drill above unlocked first_day just now; it belongs to before.
        diesel::update(t::achievements::table.filter(t::achievements::code.eq("first_day")))
            .set(t::achievements::unlocked_at.eq(now - 100))
            .execute(&mut conn)
            .unwrap();
        let summary = practice::checkin_summary_in(&mut conn, CheckinId::derived("test", "77"), false).unwrap();
        let new: Vec<&str> = summary.newly_unlocked.iter().map(|a| a.code.as_str()).collect();
        assert!(new.contains(&"streak_3") && !new.contains(&"first_day"), "{new:?}");
    }

    #[test]
    fn a_day_is_done_when_todays_lessons_are() {
        let mut conn = scratch_db("day-done");
        let today = db::today(&mut conn);
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let lesson = lessons::for_course(&mut conn, basics).unwrap()[0].clone();
        lessons::due_now(&mut conn, lesson.id).unwrap();
        db::delete_setting(&mut conn, "daily_day").unwrap();

        // XP alone does not finish the day: there is no goal to reach.
        stats::add_xp(&mut conn, today, 1000).unwrap();
        assert!(!stats::day_is_done(&mut conn, today).unwrap(), "today's lessons are still to do");
        assert!(!stats::mark_practised(&mut conn, today).unwrap());
        for id in plan::plan(&mut conn, today).unwrap().left {
            lessons::record(&mut conn, id, "pass").unwrap();
        }
        assert!(stats::mark_practised(&mut conn, today).unwrap(), "every one of today's lessons done");
        assert!(stats::collect(&mut conn).unwrap().completed_today);

        // Effort pays: a check-in like several drills; writing goals add.
        assert_eq!(stats::xp_for_checkin("medium", "pass", 0), 80);
        assert_eq!(stats::xp_for_checkin("medium", "pass", 2), 100);
        assert_eq!(stats::xp_for_checkin("high", "partial", 0), 80);
        assert_eq!(stats::xp_for_checkin("low", "fail", 3), 0);
    }

    #[test]
    fn today_is_the_five_most_important_lessons() {
        use diesel::prelude::*;
        let mut conn = scratch_db("plan");
        let today = db::today(&mut conn);
        // Every lesson has been done before and is due today.
        let all: Vec<LessonId> = schema::lessons::table.select(schema::lessons::id).load(&mut conn).unwrap();
        assert!(all.len() > 6);
        diesel::update(schema::lessons::table)
            .set((
                schema::lessons::due_day.eq(today),
                schema::lessons::last_day.eq(today - 4),
                schema::lessons::interval_days.eq(4),
                schema::lessons::reps.eq(1),
                schema::lessons::last_result.eq("pass"),
                schema::lessons::passed.eq(1),
            ))
            .execute(&mut conn)
            .unwrap();
        // One slipped last time; one is mastered, so it never comes back.
        let (slipping, mastered) = (all[all.len() - 1], all[0]);
        diesel::update(schema::lessons::table.find(slipping)).set(schema::lessons::last_result.eq("partial")).execute(&mut conn).unwrap();
        diesel::update(schema::lessons::table.find(mastered))
            .set((schema::lessons::mastery.eq(100), schema::lessons::support_level.eq(3)))
            .execute(&mut conn)
            .unwrap();
        assert!(lessons::stamp_mastered(&mut conn, mastered).unwrap());

        let first = plan::plan(&mut conn, today).unwrap();
        assert_eq!(first.left.len(), crate::convert::index(plan::DEFAULT_DAILY), "five, not everything due");
        assert_eq!(first.left[0], slipping, "what slipped comes first");
        assert!(!first.left.contains(&mastered), "a mastered lesson is out of play");
        assert_eq!(plan::plan(&mut conn, today).unwrap().left, first.left, "kept for the day");

        // Clearing the list clears the day — and nothing more is offered.
        stats::add_xp(&mut conn, today, 10).unwrap();
        assert!(!stats::day_is_done(&mut conn, today).unwrap());
        for id in &first.left {
            lessons::record_graded(&mut conn, *id, "pass", true).unwrap();
        }
        let after = plan::plan(&mut conn, today).unwrap();
        assert!(after.left.is_empty());
        assert_eq!(after.done, first.total);
        assert!(stats::day_is_done(&mut conn, today).unwrap(), "today's lessons are done");

        // A course's page lists what matters most first, mastered last.
        let course = lessons::load(&mut conn, slipping).unwrap().course_id.unwrap();
        let listed = lessons::for_course(&mut conn, course).unwrap();
        let ordered = plan::order_for_course(&mut conn, listed).unwrap();
        if ordered.iter().any(|l| l.id == mastered) {
            assert_eq!(ordered.last().unwrap().id, mastered);
        }

        // An aced lesson went well out; doing it again today is extra practice.
        let aced = lessons::load(&mut conn, first.left[0]).unwrap();
        assert!(aced.due_day.unwrap() >= today + 10, "{:?}", aced.due_day);
        assert!(lessons::is_extra_practice(&aced, today));

        // Failed today, then done again: a real retry, and its pass counts.
        let retried = *all.iter().filter(|id| !first.left.contains(id)).nth(1).unwrap();
        diesel::update(schema::lessons::table.find(retried))
            .set((schema::lessons::passed.eq(0), schema::lessons::due_day.eq(today)))
            .execute(&mut conn)
            .unwrap();
        let failed = practice::lesson_finish_in(&mut conn, retried, 2, 10).unwrap();
        assert_eq!((failed.result.as_str(), failed.extra, failed.lesson.passed), ("fail", false, false));
        let again = practice::lesson_finish_in(&mut conn, retried, 10, 10).unwrap();
        assert_eq!((again.result.as_str(), again.extra), ("pass", false), "a retry after a fail is recorded");
        assert!(again.lesson.passed, "and passing it counts");
        let third = practice::lesson_finish_in(&mut conn, retried, 10, 10).unwrap();
        assert!(third.extra, "passing it once more the same day is extra practice");
        assert_eq!(third.lesson.due_day, again.lesson.due_day, "and does not move it further out");
        diesel::update(schema::lessons::table.find(retried))
            .set((schema::lessons::due_day.eq(None::<i64>), schema::lessons::reps.eq(0), schema::lessons::last_day.eq(None::<i64>)))
            .execute(&mut conn)
            .unwrap();

        // Pulling one forward from next week is a real check-in, counted ahead.
        let later = *all.iter().find(|id| !first.left.contains(id)).unwrap();
        diesel::update(schema::lessons::table.find(later))
            .set((schema::lessons::due_day.eq(today + 2), schema::lessons::last_day.eq(today - 8), schema::lessons::interval_days.eq(10)))
            .execute(&mut conn)
            .unwrap();
        let moved = lessons::record_graded(&mut conn, later, "pass", false).unwrap();
        assert!(moved.due_day.unwrap() > today + 2, "an early pass still moves it out");

        // A new check-in lands on the lightest nearby day.
        let crowded = today + 20;
        diesel::update(schema::lessons::table.filter(schema::lessons::id.ne(later)))
            .set(schema::lessons::due_day.eq(crowded))
            .execute(&mut conn)
            .unwrap();
        let day = plan::spread(&mut conn, today, 20, later).unwrap();
        assert_ne!(day, crowded);
        assert!((day - crowded).abs() <= 2);
    }

    #[test]
    fn new_lessons_take_at_most_two_places_a_day() {
        let mut conn = scratch_db("plan-new");
        let today = db::today(&mut conn);
        db::delete_setting(&mut conn, "daily_day").unwrap();
        // Nothing has been started: only new lessons, and at most two.
        let list = plan::plan(&mut conn, today).unwrap();
        assert_eq!(list.left.len(), plan::MAX_NEW, "{list:?}");
        let first = lessons::load(&mut conn, list.left[0]).unwrap();
        assert!(first.due_day.is_none(), "new");
        // A smaller day setting is honoured.
        db::set_setting(&mut conn, "daily_lessons", "1").unwrap();
        db::delete_setting(&mut conn, "daily_day").unwrap();
        assert_eq!(plan::plan(&mut conn, today).unwrap().left.len(), 1);
    }


    #[test]
    fn challenges_come_once_the_rest_is_passed_and_closed_courses_stay_put() {
        let mut conn = scratch_db("challenges");
        let today = db::today(&mut conn);
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let ids: Vec<LessonId> = lessons::for_course(&mut conn, course).unwrap().iter().map(|l| l.id).collect();
        let (challenge, core) = ids.split_last().unwrap();
        diesel::update(t::lessons::table.find(*challenge)).set(t::lessons::challenge.eq(1)).execute(&mut conn).unwrap();
        // Every core lesson but one passed, and not due for a while.
        for id in &core[1..] {
            diesel::update(t::lessons::table.find(*id))
                .set((t::lessons::passed.eq(1), t::lessons::due_day.eq(Some(today + 30)), t::lessons::last_day.eq(Some(today - 1))))
                .execute(&mut conn)
                .unwrap();
        }
        db::delete_setting(&mut conn, "daily_day").unwrap();
        assert!(!plan::plan(&mut conn, today).unwrap().left.contains(challenge), "not before the rest is passed");
        diesel::update(t::lessons::table.find(core[0]))
            .set((t::lessons::passed.eq(1), t::lessons::due_day.eq(Some(today + 30)), t::lessons::last_day.eq(Some(today - 1))))
            .execute(&mut conn)
            .unwrap();
        db::delete_setting(&mut conn, "daily_day").unwrap();
        assert!(plan::plan(&mut conn, today).unwrap().left.contains(challenge), "offered once the rest is passed");

        // A course of challenges only can be completed.
        let hard = courses::create_empty(&mut conn, "Hard things", "").unwrap();
        let tough = lessons::create_study(&mut conn, serde_json::from_value(serde_json::json!({
            "title": "Aspect", "complexity": "high", "content": "x", "objectives": ["y"], "course_id": hard
        })).unwrap(), "tutor").unwrap().id;
        diesel::update(t::lessons::table.find(tough)).set(t::lessons::challenge.eq(1)).execute(&mut conn).unwrap();
        assert_eq!(courses::status(&mut conn, hard).unwrap().status, "active");
        diesel::update(t::lessons::table.find(tough)).set(t::lessons::passed.eq(1)).execute(&mut conn).unwrap();
        assert_eq!(courses::status(&mut conn, hard).unwrap().status, "completed");

        // Closed is closed: nothing about it changes.
        diesel::update(t::courses::table.find(hard)).set(t::courses::closed_at.eq(Some(db::now_secs()))).execute(&mut conn).unwrap();
        assert!(courses::rename(&mut conn, hard, Some("New name"), None, None).is_err());
        assert!(levels::set_course_level(&mut conn, hard, "B2").is_err());
        assert!(courses::remove_lesson(&mut conn, hard, tough).is_err());
        assert!(lessons::detail(&mut conn, tough).is_ok());
    }

    #[test]
    fn courses_complete_then_master_and_challenges_only_count_for_mastery() {
        let mut conn = scratch_db("course-status");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let ids: Vec<LessonId> = lessons::for_course(&mut conn, course).unwrap().iter().map(|b| b.id).collect();
        assert!(ids.len() >= 2);
        let set = |conn: &mut db::Conn, id: LessonId, passed: i64, mastery: i64| {
            diesel::update(t::lessons::table.find(id))
                .set((t::lessons::passed.eq(passed), t::lessons::mastery.eq(mastery), t::lessons::support_level.eq(3)))
                .execute(conn)
                .unwrap();
            lessons::stamp_mastered(conn, id).unwrap();
        };
        courses::milestones(&mut conn).unwrap();
        let fresh = courses::status(&mut conn, course).unwrap();
        assert_eq!(fresh.status, "active");
        let first_title = lessons::load(&mut conn, ids[0]).unwrap().title;
        assert_eq!(fresh.next_lesson.as_deref(), Some(first_title.as_str()), "the next is the first not passed");
        set(&mut conn, ids[0], 1, 30);
        let second_title = lessons::load(&mut conn, ids[1]).unwrap().title;
        assert_eq!(courses::status(&mut conn, course).unwrap().next_lesson, Some(second_title));

        // The last lesson is a challenge: not passed, and still the course completes.
        let (challenge, core) = ids.split_last().unwrap();
        lessons::update(&mut conn, *challenge, lessons::LessonChanges { challenge: Some(true), ..Default::default() }).unwrap();
        for id in core {
            set(&mut conn, *id, 1, 30);
        }
        let status = courses::status(&mut conn, course).unwrap();
        assert_eq!(status.status, "completed");
        assert_eq!((status.passed_core, status.challenges), (crate::convert::count(core.len()), 1));
        assert_eq!((status.mastered_lessons, status.next_lesson), (0, None), "passed, but not mastered yet");
        assert_eq!((status.mastery_points, status.mastery_max), (30 * crate::convert::count(core.len()), 100 * crate::convert::count(ids.len())));
        let first = courses::milestones(&mut conn).unwrap();
        assert!(first.iter().any(|m| m.course_id == course && !m.mastered), "{first:?}");
        assert!(courses::milestones(&mut conn).unwrap().iter().all(|m| m.course_id != course), "celebrated once");

        // Mastered: everything, challenge too, passed with full mastery points.
        for id in core {
            set(&mut conn, *id, 1, 100);
        }
        let status = courses::status(&mut conn, course).unwrap();
        assert_eq!(status.status, "completed", "the challenge is not done");
        assert_eq!(status.mastered_lessons, crate::convert::count(core.len()));
        set(&mut conn, *challenge, 1, 100);
        assert_eq!(courses::status(&mut conn, course).unwrap().status, "mastered");
        assert!(courses::milestones(&mut conn).unwrap().iter().any(|m| m.course_id == course && m.mastered));
        assert!(courses::status(&mut conn, course).unwrap().closed_at.is_none(), "mastered: ready to close, by the learner");
        let closed = courses::close(&mut conn, course).unwrap();
        assert_eq!(closed.lessons, crate::convert::count(ids.len()));
        assert!(closed.days >= 1);
        assert!(courses::status(&mut conn, course).unwrap().closed_at.is_some());

        // Passing a lesson marks it passed.
        diesel::update(t::lessons::table.find(core[0])).set(t::lessons::passed.eq(0)).execute(&mut conn).unwrap();
        lessons::record(&mut conn, core[0], "pass").unwrap();
        assert!(lessons::load(&mut conn, core[0]).unwrap().passed);
    }

    #[test]
    fn mastery_is_earned_by_passing_without_waiting() {
        let mut conn = scratch_db("mastery-now");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, course).unwrap().into_iter().find(|l| l.lesson_type != "writing").unwrap();
        // All in one day: the first pass, then same-day passes — extra
        // practice, which leaves the schedule alone but still counts — up
        // to 90, where the points are held until another day.
        let first = practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
        assert_eq!(first.lesson.mastery, 30);
        assert_eq!(first.lesson.passes_left.now, 7, "six more today to 90, then one another day");
        let due = first.lesson.due_day;
        let mut passes = 1;
        let mut lesson = first.lesson;
        for _ in 0..10 {
            let done = practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
            assert!(done.extra, "same-day replays are extra practice");
            lesson = done.lesson;
            passes += 1;
        }
        assert_eq!((lesson.mastery, lesson.mastered, lesson.due_day), (90, false, due), "crammed: held at 90, schedule untouched");
        assert_eq!(lesson.passes_left, lessons::PassesLeft { spaced: 1, now: 1 });
        let log = crate::masterylog::for_lesson(&mut conn, drill.id).unwrap();
        assert_eq!(log.entries.len(), passes);
        assert_eq!(log.entries[0].reason, "capped");
        assert_eq!(log.entries[0].delta, 0);
        assert_eq!(log.entries.iter().filter(|e| e.reason == "pass_same_day").count(), 6, "40 to 90");
        // The next day — any later day, not the check-in's — one pass masters it.
        conn.batch_execute("UPDATE lessons SET last_day = last_day - 1; UPDATE mastery_log SET day = day - 1;").unwrap();
        let next = practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
        assert!(!next.extra);
        assert_eq!((next.lesson.mastery, next.lesson.mastered), (100, true), "90 + 16");
        let log = crate::masterylog::for_lesson(&mut conn, drill.id).unwrap();
        assert_eq!((log.entries[0].reason.as_str(), log.entries[1].reason.as_str()), ("mastered", "pass_spaced"));
        assert_eq!(log.entries[1].text, "Pass 9 of 10, a day after the last go: +10 (90 → 100)");

        // Mastery is for good: a fail afterwards takes nothing away.
        let failed = practice::lesson_finish_in(&mut conn, drill.id, 0, 10).unwrap();
        assert_eq!((failed.lesson.mastery, failed.lesson.mastered), (100, true));
        let failed = lessons::record(&mut conn, drill.id, "fail").unwrap();
        assert_eq!((failed.mastery, failed.mastered), (100, true), "not on a real check-in either");
    }

    #[test]
    fn a_mastered_course_closes_into_a_trophy_and_leaves_the_check_ins() {
        let mut conn = scratch_db("course-close");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let ids: Vec<LessonId> = lessons::for_course(&mut conn, course).unwrap().iter().map(|l| l.id).collect();
        let today = db::today(&mut conn);
        let hold = |conn: &mut db::Conn, id: LessonId, gap: i64| {
            diesel::update(t::lessons::table.find(id))
                .set((
                    t::lessons::passed.eq(1),
                    t::lessons::interval_days.eq(gap),
                    t::lessons::support_level.eq(3),
                    t::lessons::mastery.eq(if gap >= 30 { 100 } else { 50 }),
                    t::lessons::due_day.eq(Some(today)),
                    t::lessons::last_day.eq(Some(today - gap)),
                ))
                .execute(conn)
                .unwrap();
            lessons::stamp_mastered(conn, id).unwrap();
        };
        for id in &ids {
            hold(&mut conn, *id, 3);
        }
        let err = courses::close(&mut conn, course).unwrap_err().to_string();
        assert!(err.contains("not mastered"), "{err}");

        for id in &ids {
            hold(&mut conn, *id, 40);
        }
        // Mastered lessons are out of play at once, before the course is closed.
        let shared = ids[0];
        db::delete_setting(&mut conn, "daily_day").unwrap();
        assert_eq!(plan::remaining(&mut conn, today + 1).unwrap(), 0, "mastered lessons never come back");
        assert!(plan::out_of_play(&mut conn).unwrap().contains(&shared));

        courses::close(&mut conn, course).unwrap();
        let status = courses::status(&mut conn, course).unwrap();
        assert!(status.closed_at.is_some());
        let retired = courses::retired_lessons(&mut conn).unwrap();
        assert!(ids.iter().all(|id| retired.contains(id)), "a closed course's lessons are retired");
        let after = plan::plan(&mut conn, today).unwrap();
        assert!(ids.iter().all(|id| !after.left.contains(id)), "none of them is asked for");

        // Finished: nothing is added to it; its lessons still open.
        let err = courses::add_lesson(&mut conn, course, ids[1]).unwrap_err().to_string();
        assert!(err.contains("mastered and closed"), "{err}");
        assert!(lessons::detail(&mut conn, ids[1]).is_ok());
        assert!(courses::close(&mut conn, course).is_err(), "closed once");

        let summary = stats::mastered_summary(&mut conn).unwrap();
        assert_eq!((summary.courses, summary.lessons), (1, crate::convert::count(ids.len())));
        assert_eq!(stats::collect(&mut conn).unwrap().mastered, summary);


        // The built-in courses that never end are never completed or closed.
        let grammar = courses::other_course(&mut conn).unwrap();
        let study = lessons::create_study(&mut conn, serde_json::from_value(serde_json::json!({
            "title": "A one-off", "complexity": "low", "content": "x", "objectives": ["y"]
        })).unwrap(), "tutor").unwrap().id;
        hold(&mut conn, study, 40);
        let open = courses::status(&mut conn, grammar).unwrap();
        assert!(open.open_ended);
        assert_eq!(open.status, "active");
        assert!(courses::close(&mut conn, grammar).unwrap_err().to_string().contains("never closed"));
    }

    #[test]
    fn an_install_from_before_the_daily_plan_upgrades_and_starts() {
        let Ok(old) = std::env::var("OLD_DB") else { return };
        let path = std::env::temp_dir().join("learn-danish-tests").join(format!("old-{}.db", std::process::id()));
        std::fs::copy(old, &path).unwrap();
        let mut conn = db::open(&path).expect("upgrade");
        let today = db::today(&mut conn);
        diesel::update(t::lessons::table)
            .set((t::lessons::due_day.eq(today - 1), t::lessons::last_day.eq(today - 5), t::lessons::reps.eq(2)))
            .execute(&mut conn)
            .unwrap();
        lessons::overview(&mut conn).expect("overview");
        stats::collect(&mut conn).expect("stats");
        stats::course_progress(&mut conn).expect("courses");
    }

    /// Today's list is kept for the day, but an empty one (chosen before
    /// there was anything to choose) or one naming only lessons this device
    /// doesn't have (from another device, mid-sync) is chosen again; and a
    /// device still catching up after joining a vault doesn't keep one.
    #[test]
    fn todays_list_is_chosen_again_when_it_names_nothing_here() {
        let mut conn = scratch_db("daily-rechosen");
        let today = db::today(&mut conn);
        let fresh = plan::plan(&mut conn, today).unwrap();
        assert!(fresh.total > 0, "the demo course gives a list");

        // Kept: an empty list from earlier today doesn't stick.
        db::set_setting(&mut conn, "daily_day", &today.to_string()).unwrap();
        db::set_setting(&mut conn, "daily_ids", "[]").unwrap();
        assert_eq!(plan::plan(&mut conn, today).unwrap().total, fresh.total);
        assert_ne!(db::get_setting(&mut conn, "daily_ids").unwrap().as_deref(), Some("[]"));

        // Only lessons that aren't here: chosen again.
        let elsewhere = serde_json::to_string(&[LessonId::new(), LessonId::new()]).unwrap();
        db::set_setting(&mut conn, "daily_ids", &elsewhere).unwrap();
        assert_eq!(plan::plan(&mut conn, today).unwrap().total, fresh.total);

        // Some here, some still to arrive: kept as it is.
        let kept = plan::plan(&mut conn, today).unwrap();
        let mut mixed = kept.left.clone();
        mixed.push(LessonId::new());
        db::set_setting(&mut conn, "daily_ids", &serde_json::to_string(&mixed).unwrap()).unwrap();
        assert_eq!(db::get_setting(&mut conn, "daily_ids").unwrap(), Some(serde_json::to_string(&mixed).unwrap()));
        assert_eq!(plan::plan(&mut conn, today).unwrap().left, kept.left, "the missing one just drops out");

        // Catching up after joining a vault: a list is shown, not kept.
        diesel::sql_query("UPDATE sync_state SET vault_id = 'v' WHERE id = 1").execute(&mut conn).unwrap();
        diesel::sql_query("INSERT INTO sync_peers (device_id, complete) VALUES ('dev_other', 0)").execute(&mut conn).unwrap();
        db::set_setting(&mut conn, "daily_day", &(today - 1).to_string()).unwrap();
        assert!(plan::plan(&mut conn, today).unwrap().total > 0);
        assert_eq!(db::get_setting_i64(&mut conn, "daily_day", -1), today - 1, "not kept while catching up");
        // Once a whole sync has happened, it is.
        diesel::sql_query("UPDATE sync_peers SET complete = 1, last_complete_at = '2026-11-18T00:00:00Z'").execute(&mut conn).unwrap();
        plan::plan(&mut conn, today).unwrap();
        assert_eq!(db::get_setting_i64(&mut conn, "daily_day", -1), today);
    }

    #[test]
    fn streak_freezes_are_earned_by_a_week_and_bridge_a_missed_day() {
        let mut conn = scratch_db("freezes");
        let today = db::today(&mut conn);
        // Nothing to do today (every lesson mastered, so today's list is
        // empty): practising at all finishes the day.
        diesel::update(t::lessons::table).set(t::lessons::mastered_at.eq(Some(0))).execute(&mut conn).unwrap();
        let done = |conn: &mut db::Conn, day: i64| {
            let device = crate::sync::device_id(conn).unwrap();
            diesel::insert_into(t::day_log::table)
                .values((t::day_log::day.eq(day), t::day_log::device.eq(device), t::day_log::xp.eq(50), t::day_log::completed.eq(1)))
                .execute(conn)
                .unwrap();
        };
        // Six done days, and today finishes the week: a freeze is earned.
        for day in today - 6..today {
            done(&mut conn, day);
        }
        stats::add_xp(&mut conn, today, 20).unwrap();
        assert!(stats::mark_practised(&mut conn, today).unwrap());
        assert_eq!(stats::collect(&mut conn).unwrap().streak_freezes, 1);
        assert_eq!(stats::collect(&mut conn).unwrap().current_streak, 7);

        // Two days later, one day missed (yesterday): the freeze covers it.
        let mut later = scratch_db("freezes-later");
        let now = db::today(&mut later);
        for day in now - 5..now - 1 {
            done(&mut later, day);
        }
        db::set_setting(&mut later, "streak_freezes", "1").unwrap();
        let stats = stats::collect(&mut later).unwrap();
        assert_eq!(stats.current_streak, 4, "the run survives; the frozen day adds nothing");
        assert_eq!(stats.streak_freezes, 0, "and the freeze is spent");
        assert!(stats.history.iter().any(|d| d.day == now - 1 && d.frozen));

        // Two days missed with one freeze: the run is gone, the freeze kept.
        let mut gap = scratch_db("freezes-gap");
        let now = db::today(&mut gap);
        for day in now - 6..now - 2 {
            done(&mut gap, day);
        }
        db::set_setting(&mut gap, "streak_freezes", "1").unwrap();
        let stats = stats::collect(&mut gap).unwrap();
        assert_eq!((stats.current_streak, stats.streak_freezes), (0, 1));
    }

    #[test]
    fn achievements_reward_the_deeper_work_and_mastery_needs_spacing() {
        let mut conn = scratch_db("achievements");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let word: WordId = t::course_words::table
            .filter(t::course_words::course_id.eq(basics))
            .select(t::course_words::id)
            .first(&mut conn)
            .unwrap();
        // Real answers, not made-up rows: many right answers in a short span
        // are not mastery; answers spread over weeks are.
        let answer = |conn: &mut Conn| {
            practice::submit_answer_in(
                conn,
                practice::SubmitAnswer {
                    kind: "type".into(),
                    item_kind: "word".into(),
                    item_id: word.to_string(),
                    expected: "kot".into(),
                    given: "kot".into(),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let state = |conn: &mut Conn| srs::load(conn, "word", &word.to_string()).unwrap().unwrap();
        for _ in 0..9 {
            assert!(answer(&mut conn).correct);
        }
        assert_eq!(state(&mut conn).reps, 1, "nine right answers in a day are one review");
        assert_eq!(stats::collect(&mut conn).unwrap().words_mastered, 0);
        // Then each time it comes due, a day at a time back in its history:
        // 1, 3, 8, 20 and then more than three weeks apart.
        while state(&mut conn).interval < 21 {
            let gap = state(&mut conn).interval;
            conn.batch_execute(&format!("UPDATE reviews SET last_day = last_day - {gap}, due_day = due_day - {gap}")).unwrap();
            answer(&mut conn);
        }
        assert_eq!(stats::collect(&mut conn).unwrap().words_mastered, 1);

        // A passed writing lesson unlocks both the check-in and the writing one.
        let writing = lessons::create_writing(
            &mut conn,
            lessons::NewWriting {
                title: "Where do you live?".into(),
                complexity: "low".into(),
                content: "A neighbour asks.".into(),
                objectives: vec!["Say where you live".into()],
                ..Default::default()
            },
            "tutor",
        )
        .unwrap();
        diesel::insert_into(t::checkins::table)
            .values((
                t::checkins::id.eq(CheckinId::new()),
                t::checkins::lesson_id.eq(writing.id),
                t::checkins::started_at.eq(0),
                t::checkins::finished_at.eq(Some(1)),
                t::checkins::result.eq(Some("pass")),
            ))
            .execute(&mut conn)
            .unwrap();
        let stats = stats::collect(&mut conn).unwrap();
        let unlocked = |code: &str| stats.achievements.iter().any(|a| a.code == code && a.unlocked);
        assert!(unlocked("checkin_first") && unlocked("writing_first") && unlocked("tutor_made"));
        assert!(!unlocked("listening_first"));
        assert!(!stats.achievements.iter().any(|a| a.code.starts_with("level_")), "levels are not achievements");
        let every = stats.achievements.iter().find(|a| a.code == "words_all").unwrap();
        assert!(every.current > 0 && every.current < every.target, "measured against the words you have");
        let first = stats.achievements.iter().find(|a| a.code == "checkin_first").unwrap();
        assert_eq!((first.current, first.target, first.unlocked_day), (1, 1, Some(stats.day)), "unlocked today, and full");
        let ten = stats.achievements.iter().find(|a| a.code == "writing_10").unwrap();
        assert_eq!((ten.current, ten.target, ten.unlocked, ten.unlocked_day), (1, 10, false, None), "one of ten");
    }

    #[test]
    fn a_new_course_is_a_varied_path() {
        let mut conn = scratch_db("course-path");
        let words: Vec<serde_json::Value> = (0..24)
            .map(|i| serde_json::json!({ "da": format!("ord{i}"), "en": format!("word {i}"), "pos": "n" }))
            .collect();
        let sentences: Vec<serde_json::Value> = (0..10)
            .map(|i| serde_json::json!({ "da": format!("Dette er sætning {i}."), "en": format!("This is sentence {i}.") }))
            .collect();
        let study = |title: &str| {
            serde_json::json!({
                "title": title, "summary": "s", "complexity": "low",
                "content": "**Jeg har** — I have", "objectives": ["Say what you have"]
            })
        };
        let made = tutor::run_tool_for_test(
            &mut conn,
            "create_course",
            serde_json::json!({
                "title": "At the market", "title_da": "Na targu", "summary": "Buying food.", "level": "a1",
                "words": words, "sentences": sentences,
                "dialogue": [
                    { "speaker": "Ola", "gender": "f", "da": "Godmorgen!", "en": "Good morning!" },
                    { "speaker": "Jan", "gender": "m", "da": "Godmorgen, jeg vil gerne have nogle æbler.", "en": "Good morning, apples please." }
                ],
                "study_lessons": [study("At have — to have"), study("Making polite requests with gerne")],
                "writing": {
                    "title": "Buying apples", "summary": "Buy some apples.", "complexity": "low",
                    "scenario": "You are at a market stall.", "goals": ["Ask for apples", "Ask the price"]
                }
            }),
        );
        assert_eq!((made["study_lessons_made"].as_i64(), made["writing"].as_bool()), (Some(2), Some(true)), "{made}");
        let course = serde_json::from_value(made["created"]["course_id"].clone()).unwrap();
        assert_eq!(session::load_course(&mut conn, course).unwrap().level.as_deref(), Some("A1"), "made at its level");
        let path: Vec<(String, String)> =
            lessons::for_course(&mut conn, course).unwrap().into_iter().map(|b| (b.lesson_type, b.title)).collect();
        let formats: Vec<&str> = path.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(formats.first(), Some(&"dialogue"), "the conversation first: {path:?}");
        assert_eq!(formats.last(), Some(&"writing"), "writing last: {path:?}");
        assert_eq!(formats[1], "words");
        assert_eq!(formats[2], "study", "the first lesson once the first words are in: {path:?}");
        assert!(formats.contains(&"typing"), "every third batch of words is typed: {path:?}");
        assert_eq!(formats.iter().filter(|f| **f == "words" || **f == "typing").count(), 3);
    }

    #[test]
    fn study_lessons_keep_their_wikipedia_sources() {
        let mut conn = scratch_db("wiki-sources");
        let study = lessons::create_study(
            &mut conn,
            lessons::NewStudy {
                title: "København".into(),
                complexity: "low".into(),
                content: "København is Denmark's capital.".into(),
                objectives: vec!["Say what København is known for".into()],
                ..Default::default()
            },
            "tutor",
        )
        .unwrap();
        let article = wiki::Article {
            lang: "en".into(),
            title: "København".into(),
            url: "https://en.wikipedia.org/wiki/Copenhagen".into(),
            pageid: 16815,
            revid: 1,
            summary: "København is the capital of Denmark.".into(),
            content: "København is the capital of Denmark.\n\n## History\nA city with a long history.".into(),
            thumbnail: None,
            danish_title: Some("København".into()),
            html: None,
        };
        let danish = wiki::Article {
            lang: "da".into(),
            title: "København".into(),
            url: "https://da.wikipedia.org/wiki/Copenhagen".into(),
            content: "København er Danmarks hovedstad.".into(),
            ..article.clone()
        };
        let picture = Some(("image/jpeg".to_string(), vec![1u8, 2, 3]));
        // Attached to an existing lesson — and again, which replaces it.
        wiki::store(&mut conn, study.id, &article, Some(&danish), picture.clone()).unwrap();
        let source = wiki::store(&mut conn, study.id, &wiki::Article { revid: 2, ..article.clone() }, Some(&danish), picture).unwrap();
        assert_eq!(wiki::sources_of(&mut conn, study.id).unwrap().len(), 1, "one article, one source");
        assert_eq!(t::files::table.count().get_result::<i64>(&mut conn).unwrap(), 1, "the old picture went");

        // Drills take no sources.
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let drill = lessons::for_course(&mut conn, basics)
            .unwrap()
            .into_iter()
            .find(|b| b.engine == "exercises")
            .unwrap();
        assert!(wiki::store(&mut conn, drill.id, &article, None, None).is_err());

        // The tutor sees it; the examiner reads it.
        let got = tutor::run_tool_for_test(&mut conn, "get_lesson", serde_json::json!({ "lesson_id": study.id }));
        assert_eq!(got["sources"][0]["in_danish_too"], true, "{got}");
        let detail = lessons::detail(&mut conn, study.id).unwrap();
        let prompt = examiner::system_prompt(&mut conn, &detail).unwrap();
        assert!(prompt.contains("A city with a long history") && prompt.contains("learner's language"), "{prompt}");

        // Due for a fact check once three months have passed.
        let now = db::now_secs();
        assert!(wiki::lessons_due(&mut conn, now).unwrap().is_empty());
        diesel::update(t::wiki_sources::table)
            .set(t::wiki_sources::checked_at.eq(now - 91 * 86_400))
            .execute(&mut conn)
            .unwrap();
        assert_eq!(wiki::lessons_due(&mut conn, now).unwrap(), vec![study.id]);
        // Fetched again without a picture, the source keeps its old one.
        let again = wiki::store(&mut conn, study.id, &article, Some(&danish), None).unwrap();
        assert!(again.image_file_id.is_some());

        // Removed by the tutor, or with its lesson.
        tutor::run_tool_for_test(&mut conn, "remove_source", serde_json::json!({ "source_id": source.id }));
        assert!(wiki::sources_of(&mut conn, study.id).unwrap().is_empty());
        wiki::store(&mut conn, study.id, &article, None, None).unwrap();
        lessons::delete(&mut conn, study.id).unwrap();
        assert_eq!(t::wiki_sources::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
    }

    #[test]
    fn podcasts_keep_their_wikipedia_sources_apart_from_lessons() {
        let mut conn = scratch_db("podcast-sources");
        let song_id = songs::insert_song(
            &mut conn,
            &songs::SongDraft { title: "The bridge".into(), kind: "podcast".into(), ..Default::default() },
            "tts",
        )
        .unwrap();
        let article = wiki::Article {
            lang: "en".into(),
            title: "Øresund Bridge".into(),
            url: "https://en.wikipedia.org/wiki/%C3%98resund_Bridge".into(),
            pageid: 9,
            revid: 1,
            summary: "A bridge.".into(),
            content: "A bridge between Denmark and Sweden.".into(),
            thumbnail: None,
            danish_title: Some("Øresundsbroen".into()),
            html: Some("<p>A <b>bridge</b>.</p>".into()),
        };
        let danish = wiki::Article { lang: "da".into(), html: Some("<p>En bro.</p>".into()), ..article.clone() };
        let source = wiki::store_for(&mut conn, wiki::Owner::Song(song_id), &article, Some(&danish), None).unwrap();
        assert_eq!((source.song_id, source.lesson_id), (Some(song_id), None));
        assert_eq!(wiki::sources_of_song(&mut conn, song_id).unwrap().len(), 1);
        let html: (Option<String>, Option<String>) = t::wiki_sources::table
            .find(source.id)
            .select((t::wiki_sources::html, t::wiki_sources::da_html))
            .first(&mut conn)
            .unwrap();
        assert_eq!(html, (Some("<p>A <b>bridge</b>.</p>".into()), Some("<p>En bro.</p>".into())));
        // Stored again without a layout, the layout stays.
        wiki::store_for(&mut conn, wiki::Owner::Song(song_id), &wiki::Article { html: None, ..article.clone() }, None, None).unwrap();
        let kept: Option<String> =
            t::wiki_sources::table.find(source.id).select(t::wiki_sources::html).first(&mut conn).unwrap();
        assert!(kept.is_some());
        // Episodes are not fact-checked; they go with their episode.
        let now = db::now_secs();
        diesel::update(t::wiki_sources::table).set(t::wiki_sources::checked_at.eq(0)).execute(&mut conn).unwrap();
        assert!(wiki::lessons_due(&mut conn, now).unwrap().is_empty());
        songs::delete_song(&mut conn, song_id).unwrap();
        assert_eq!(t::wiki_sources::table.count().get_result::<i64>(&mut conn).unwrap(), 0);
    }

    #[test]
    fn audio_cache_round_trips_and_is_keyed_by_voice() {
        let mut conn = scratch_db("audio");
        for (voice, bytes) in [("Kore", &[1u8, 2, 3][..]), ("Puck", &[9, 9][..])] {
            let key = speech::audio_key("m", voice, "Hej");
            speech::store_clip(&mut conn, &key, "m", voice, "Hej", "audio/mpeg", bytes).unwrap();
        }
        let stored: Vec<u8> = t::speech_clips::table
            .inner_join(t::files::table)
            .filter(t::speech_clips::voice.eq("Kore"))
            .select(t::files::bytes)
            .first(&mut conn)
            .unwrap();
        assert_eq!(stored, vec![1, 2, 3], "blobs must survive the round trip");

        let stats = speech::audio_cache_stats_in(&mut conn).unwrap();
        assert_eq!((stats.clips, stats.bytes), (2, 5), "two voices of the same line are stored separately");

        // Making a clip again replaces it, file and all.
        let key = speech::audio_key("m", "Kore", "Hej");
        speech::store_clip(&mut conn, &key, "m", "Kore", "Hej", "audio/mpeg", &[7]).unwrap();
        let files: i64 = t::files::table.count().get_result(&mut conn).unwrap();
        assert_eq!(files, 2, "the replaced file is gone");
    }

    #[test]
    fn audio_overview_shows_what_each_course_has_stored() {
        let mut conn = scratch_db("audio-overview");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let items = speech::course_audio_in(&mut conn, basics).unwrap();
        assert!(items.iter().any(|i| i.kind == "word") && items.iter().any(|i| i.kind == "sentence"));
        assert!(items.iter().all(|i| !i.stored), "nothing downloaded yet");

        // Store the first item in the voice it is read in, plus a stray clip.
        let first = items[0].clone();
        let voice = ai::voice_for(&mut conn, &first.gender);
        for (voice, text) in [(voice.as_str(), first.text.as_str()), ("OldVoice", "Hej")] {
            let key = speech::audio_key(ai::SPEECH_MODEL, voice, text);
            speech::store_clip(&mut conn, &key, ai::SPEECH_MODEL, voice, text, "audio/mpeg", &[1, 2, 3]).unwrap();
        }
        let overview = speech::audio_overview_in(&mut conn).unwrap();
        let course = overview.courses.iter().find(|c| c.course_id == basics).unwrap();
        assert_eq!((course.stored, course.bytes), (1, 3));
        assert_eq!(course.total, crate::convert::count(items.len()));
        assert_eq!((overview.clips, overview.unused_clips), (2, 1), "the old voice's clip is unused");
        assert!(speech::course_audio_in(&mut conn, basics).unwrap()[0].stored);

        assert_eq!(speech::delete_unused_audio_in(&mut conn).unwrap(), 1);
        let after = speech::audio_overview_in(&mut conn).unwrap();
        assert_eq!((after.clips, after.unused_clips), (1, 0), "the used clip stays");
    }

    /// Every registered lesson type can be made and practised, and every
    /// exercise a drill can produce is one the screens know how to show
    /// (src/lib/types.ts lists them; src/exercises/ must draw each, or the
    /// frontend build fails). A new type without a sample here fails too.
    #[test]
    fn courses_and_lessons_carry_a_cefr_level_that_never_limits() {
        let mut conn = scratch_db("levels");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        assert_eq!(session::load_course(&mut conn, course).unwrap().level, None, "not levelled yet");

        // The tutor sets a course's level; a loosely written one is read.
        let edited = tutor::run_tool_for_test(&mut conn, "edit_course", serde_json::json!({ "course_id": course, "level": "b1" }));
        assert!(edited.get("error").is_none(), "{edited}");
        assert_eq!(session::load_course(&mut conn, course).unwrap().level.as_deref(), Some("B1"));
        let wrong = tutor::run_tool_for_test(&mut conn, "edit_course", serde_json::json!({ "course_id": course, "level": "expert" }));
        assert!(wrong.to_string().contains("not a level"), "{wrong}");

        // A lesson can have its own level, and follow its course again.
        let lesson = lessons::for_course(&mut conn, course).unwrap()[0].id;
        tutor::run_tool_for_test(&mut conn, "update_lesson", serde_json::json!({ "lesson_id": lesson, "level": "B2" }));
        assert_eq!(lessons::load(&mut conn, lesson).unwrap().level.as_deref(), Some("B2"));
        tutor::run_tool_for_test(&mut conn, "update_lesson", serde_json::json!({ "lesson_id": lesson, "level": "" }));
        assert_eq!(lessons::load(&mut conn, lesson).unwrap().level, None);

        // The tutor is told levels never limit, and sees each course's.
        let rules = tutor::rules();
        assert!(rules.contains("never limit") && rules.contains("never refuse"));
        assert!(tutor::learner_state(&mut conn).unwrap().contains(", B1, "));
    }

    #[test]
    fn sentence_tiles_ask_for_the_endings_the_tutor_gave() {
        let mut conn = scratch_db("sentence-tiles");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let made = tutor::run_tool_for_test(
            &mut conn,
            "create_drill_lesson",
            serde_json::json!({
                "title": "At the café",
                "format": "sentences",
                "complexity": "low",
                "course_id": course,
                "sentences": [
                    {
                        "da": "Jeg drikker kaffen med mælk.",
                        "en": "I drink the coffee with milk.",
                        "endings": [
                            { "form": "drikker", "base": "drikke", "wrong": ["drak", "drukket"] },
                            { "form": "kaffen", "base": "kaffe", "wrong": ["kaffer", "kaffens"] },
                            { "form": "teen", "base": "te", "wrong": [] }
                        ]
                    },
                    // Without endings: plain ordering, as before.
                    { "da": "Godmorgen.", "en": "Good morning." }
                ]
            }),
        );
        let lesson_id = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let plan = session::build_lesson(&mut conn, lesson_id).unwrap();
        let orders: Vec<&models::Exercise> = plan.exercises.iter().filter(|e| e.kind == "wordOrder").collect();
        assert_eq!(orders.len(), 2);
        let coffee = orders.iter().find(|e| e.answer.starts_with("Jeg drikker")).unwrap();
        // Only the endings that fit the sentence are kept.
        assert_eq!(coffee.endings.len(), 2);
        for ending in &coffee.endings {
            let token = &coffee.tokens[ending.token];
            assert!(ending.choices.contains(token), "{token} is among {:?}", ending.choices);
            assert_ne!(&ending.shown, token, "the tile shows the dictionary form");
        }
        let plain = orders.iter().find(|e| e.answer.starts_with("Godmorgen")).unwrap();
        assert!(plain.endings.is_empty());

        // The dictionary form left as it is, or the wrong ending, is wrong —
        // Danish letters count on a tile; case and punctuation don't.
        let mut answer = |given: &str| {
            practice::submit_answer_in(
                &mut conn,
                practice::SubmitAnswer {
                    kind: "wordOrder".into(),
                    item_kind: "sentence".into(),
                    item_id: coffee.item_id.clone(),
                    expected: coffee.answer.clone(),
                    given: given.into(),
                    ..Default::default()
                },
            )
            .unwrap()
            .correct
        };
        assert!(!answer("Jeg drikke kaffe med mælk."));
        assert!(!answer("Jeg drikker kaffer med mælk."));
        assert!(!answer("jeg drikker kaffen med maelk"), "a tile's æ is an æ");
        assert!(answer("jeg drikker kaffen med mælk"));
    }

    #[test]
    fn the_ear_is_drilled_by_dictation_and_sound_pairs() {
        let mut conn = scratch_db("ear");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let list = lessons::for_course(&mut conn, basics).unwrap();

        // A dialogue lesson: two gaps and a line to write down, in its
        // speaker's voice — as long as before.
        let dialogue = list.iter().find(|b| b.lesson_type == "dialogue").unwrap();
        let plan = session::build_lesson(&mut conn, dialogue.id).unwrap();
        let kinds: Vec<&str> = plan.exercises.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds.iter().filter(|k| **k == "dialogueGap").count(), session::DIALOGUE_GAPS, "{kinds:?}");
        let dictation = plan.exercises.iter().find(|e| e.kind == "dictation").expect("a line to write down");
        assert_eq!(dictation.context.len(), 1, "the line, for its voice");
        assert!(plan.exercises.iter().filter(|e| e.kind == "dialogueGap").all(|g| g.answer != dictation.answer));

        // Sentences: new ones are built; once met, some are written from hearing.
        let lesson = list.iter().find(|b| b.lesson_type == "sentences").unwrap();
        let plan = session::build_lesson(&mut conn, lesson.id).unwrap();
        assert!(plan.exercises.iter().all(|e| e.kind != "dictation"), "new sentences are built first");
        let count = plan.exercises.len();
        for e in plan.exercises.iter().filter(|e| e.item_kind == "sentence" && e.kind == "wordOrder") {
            let answer = practice::SubmitAnswer { kind: e.kind.clone(), item_kind: e.item_kind.clone(), item_id: e.item_id.clone(), expected: e.answer.clone(), given: e.answer.clone(), ..Default::default() };
            practice::submit_answer_in(&mut conn, answer).unwrap();
        }
        let plan = session::build_lesson(&mut conn, lesson.id).unwrap();
        let written: Vec<&models::Exercise> = plan.exercises.iter().filter(|e| e.kind == "dictation").collect();
        assert!(!written.is_empty() && plan.exercises.iter().any(|e| e.kind == "wordOrder"), "some of each");
        assert!(plan.exercises.len() <= count, "no longer than before");

        // Checking a dictation: forgiving, word by word, and it counts as a review.
        let s = written[0];
        // Lowercase, no punctuation, æ ø å typed the keyboard way.
        let typed: String = s
            .answer
            .to_lowercase()
            .replace('æ', "ae")
            .replace('ø', "oe")
            .replace('å', "aa")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .collect();
        let reply = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "dictation".into(), item_kind: "sentence".into(), item_id: s.item_id.clone(), expected: s.answer.clone(), given: typed, ..Default::default()
        }).unwrap();
        assert!(reply.correct, "{:?}", reply.diff);
        assert_eq!(reply.diff.len(), s.answer.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count());
        assert_eq!(reply.xp_gained, stats::xp_for_answer("dictation", true, false));
        let reply = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "dictation".into(), item_kind: "sentence".into(), item_id: s.item_id.clone(), expected: s.answer.clone(), given: "intet".into(), ..Default::default()
        }).unwrap();
        assert!(!reply.correct && reply.diff.iter().any(|w| w.status != "right"));

        // The tutor gives a word its near neighbour; its words lesson plays the pair.
        let made = tutor::run_tool_for_test(
            &mut conn,
            "create_drill_lesson",
            serde_json::json!({
                "title": "In town", "format": "words", "complexity": "low", "course_id": basics,
                "words": [
                    { "da": "en by", "en": "town", "pos": "n-common", "near": { "da": "bi", "en": "bee" } },
                    { "da": "et hus", "en": "house", "pos": "n-neuter", "near": { "da": "hus", "en": "house" } },
                    { "da": "en kat", "en": "cat", "pos": "n-common" }
                ]
            }),
        );
        let id: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let near: Vec<(String, String)> = t::words::table
            .filter(t::words::lesson_id.eq(id))
            .order(t::words::ord)
            .select((t::words::near_da, t::words::near_en))
            .load(&mut conn)
            .unwrap();
        assert_eq!(near[0], ("bi".to_string(), "bee".to_string()));
        assert_eq!(near[1].0, "", "a word is not its own neighbour");
        let plan = session::build_lesson(&mut conn, id).unwrap();
        let pair = plan.exercises.iter().find(|e| e.kind == "soundPair").expect("a sound pair");
        assert_eq!(pair.options, ["by", "bi"], "the words themselves, without their article");
        assert_eq!(pair.prompt, "y or i");
        // Exact: "lae" is not "læ" here, though typing would let it pass.
        let other = pair.options.iter().find(|o| **o != pair.answer).unwrap().clone();
        let wrong = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "soundPair".into(), item_kind: "set".into(), item_id: id.to_string(), expected: "læ".into(), given: "lae".into(), ..Default::default()
        }).unwrap();
        assert!(!wrong.correct);
        let right = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "soundPair".into(), item_kind: "set".into(), item_id: id.to_string(), expected: pair.answer.clone(), given: pair.answer.clone(), ..Default::default()
        }).unwrap();
        assert!(right.correct && other != pair.answer);

        // The quick games: Dictation over sentences met, Sound pairs from the
        // built-in list; scored, no best kept, no review written.
        let reviews_before: i64 = t::reviews::table.count().get_result(&mut conn).unwrap();
        let game = crate::games::deal(&mut conn, "dictation", &mut rand::rng()).unwrap();
        assert!((crate::games::MIN_SENTENCES..=5).contains(&game.sentences.len()));
        let sounds = crate::games::deal(&mut conn, "sounds", &mut rand::rng()).unwrap();
        assert_eq!(sounds.sounds.len(), 8);
        let labels: std::collections::HashSet<&str> = sounds.sounds.iter().map(|r| r.pair.label.as_str()).collect();
        assert_eq!(labels.len(), 8, "a different contrast each round");
        assert!(sounds.sounds.iter().all(|r| r.said == r.pair.a || r.said == r.pair.b));
        let done = crate::games::finish(&mut conn, "dictation", 5, 60_000, 1).unwrap();
        assert!(!done.record && done.best_ms.is_none());
        assert_eq!(done.xp_gained, 10);
        let today = db::today(&mut conn);
        let day = crate::xplog::day(&mut conn, today).unwrap();
        assert!(day.runs.iter().flat_map(|r| r.entries.iter()).any(|e| e.label == "Dictation: 4 of 5 sentences right"));
        crate::games::finish(&mut conn, "sounds", 8, 30_000, 0).unwrap();
        assert_eq!(t::reviews::table.count().get_result::<i64>(&mut conn).unwrap(), reviews_before);
        let overview = crate::games::overview(&mut conn).unwrap();
        assert!(overview.sentences_met >= overview.min_sentences);
    }

    #[test]
    fn speaking_cards_take_the_place_of_others_where_the_device_can_hear() {
        let mut conn = scratch_db("speaking");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let list = lessons::for_course(&mut conn, basics).unwrap();
        let dialogue = list.iter().find(|b| b.lesson_type == "dialogue").unwrap();
        let sentences = list.iter().find(|b| b.lesson_type == "sentences").unwrap();
        let words = list.iter().find(|b| b.lesson_type == "words").unwrap();
        let kinds = |plan: &models::SessionPlan| plan.exercises.iter().map(|e| e.kind.clone()).collect::<Vec<_>>();
        let said = |plan: &models::SessionPlan| plan.exercises.iter().filter(|e| crate::speaking::KINDS.contains(&e.kind.as_str())).count();

        // No key to hear with: no speaking cards.
        assert_eq!(said(&session::build_lesson(&mut conn, dialogue.id).unwrap()), 0);

        db::set_setting(&mut conn, "openrouter_api_key", "sk-test").unwrap();
        // A dialogue: one of its two lines is said, with the other speaker's
        // answer to follow and its options kept for when speaking is skipped.
        let plan = session::build_lesson(&mut conn, dialogue.id).unwrap();
        let k = kinds(&plan);
        assert_eq!(k.iter().filter(|k| *k == "dialogueSay").count(), 1, "{k:?}");
        assert_eq!(k.iter().filter(|k| *k == "dialogueGap" || *k == "dialogueSay").count(), session::DIALOGUE_GAPS);
        let turn = plan.exercises.iter().find(|e| e.kind == "dialogueSay").unwrap();
        assert!(turn.options.contains(&turn.answer));
        let lines = session::lesson_lines(&mut conn, dialogue.id).unwrap();
        let at = lines.iter().position(|l| l.da == turn.answer).unwrap();
        assert_eq!(turn.reply.as_ref().map(|r| r.da.clone()), lines.get(at + 1).map(|l| l.da.clone()));

        // Sentences: new ones are never said; once met, a couple are, and
        // the lesson is no longer.
        let first = session::build_lesson(&mut conn, sentences.id).unwrap();
        assert_eq!(said(&first), 0, "new sentences are built first");
        for e in first.exercises.iter().filter(|e| e.item_kind == "sentence" && e.kind == "wordOrder") {
            let answer = practice::SubmitAnswer { kind: e.kind.clone(), item_kind: e.item_kind.clone(), item_id: e.item_id.clone(), expected: e.answer.clone(), given: e.answer.clone(), ..Default::default() };
            practice::submit_answer_in(&mut conn, answer).unwrap();
        }
        let plan = session::build_lesson(&mut conn, sentences.id).unwrap();
        let say: Vec<&models::Exercise> = plan.exercises.iter().filter(|e| e.kind == "sayIt").collect();
        assert!((1..=crate::speaking::SENTENCE_CARDS).contains(&say.len()), "{:?}", kinds(&plan));
        assert!(plan.exercises.len() <= first.exercises.len(), "no longer than before");

        // Said right: XP like typing, and a review. Not quite heard: a sound
        // tip where one fits, and the sentence comes back sooner
        // without starting over.
        let s = say[0];
        let right = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "sayIt".into(), item_kind: "sentence".into(), item_id: s.item_id.clone(), expected: s.answer.clone(), given: s.answer.to_lowercase(), ..Default::default()
        }).unwrap();
        assert!(right.correct);
        assert_eq!(right.xp_gained, stats::xp_for_answer("sayIt", true, false));
        assert_eq!(right.xp_gained, 14);
        let before = crate::srs::load(&mut conn, "sentence", &s.item_id).unwrap().unwrap();
        let wrong = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "sayIt".into(), item_kind: "sentence".into(), item_id: s.item_id.clone(), expected: "Der står et træ.".into(), given: "Der står et tre".into(), ..Default::default()
        }).unwrap();
        // One word of four heard as another: almost — half XP.
        assert!(!wrong.correct && wrong.almost);
        assert_eq!(wrong.xp_gained, 7);
        assert_eq!(wrong.tips.len(), 1);
        assert_eq!(wrong.diff[3].status, "wrong");
        let after = crate::srs::load(&mut conn, "sentence", &s.item_id).unwrap().unwrap();
        assert_eq!(after.lapses, before.lapses, "a miss heard is not a lapse");
        assert!(after.reps > 0);

        // Words: once three are met, one is said instead of practised.
        let first = session::build_lesson(&mut conn, words.id).unwrap();
        assert_eq!(said(&first), 0);
        for e in first.exercises.iter().filter(|e| e.item_kind == "word") {
            let answer = practice::SubmitAnswer { kind: "recognise".into(), item_kind: "word".into(), item_id: e.item_id.clone(), expected: e.answer.clone(), given: e.answer.clone(), ..Default::default() };
            practice::submit_answer_in(&mut conn, answer).unwrap();
        }
        let plan = session::build_lesson(&mut conn, words.id).unwrap();
        assert_eq!(said(&plan), crate::speaking::WORD_CARDS, "{:?}", kinds(&plan));
        let card = plan.exercises.iter().find(|e| e.kind == "sayIt").unwrap();
        assert_eq!((card.item_kind.as_str(), card.answer.as_str()), ("word", card.da.as_str()));

        // Speaking turned off on this device: the usual cards again.
        let settings = commands::read_settings(&mut conn).unwrap();
        assert!(settings.speaking);
        db::set_setting(&mut conn, crate::speaking::SETTING, "0").unwrap();
        assert!(!commands::read_settings(&mut conn).unwrap().speaking);
        for id in [dialogue.id, sentences.id, words.id] {
            assert_eq!(said(&session::build_lesson(&mut conn, id).unwrap()), 0);
        }
        // A setting of this device: it does not sync.
        assert!(db::DEVICE_SETTINGS.contains(&crate::speaking::SETTING));

        // The quick game: five sentences met, scored in sentences said right.
        let game = crate::games::deal(&mut conn, "say", &mut rand::rng()).unwrap();
        assert!((crate::games::MIN_SENTENCES..=5).contains(&game.sentences.len()));
        crate::games::finish(&mut conn, "say", 5, 40_000, 2).unwrap();
        let today = db::today(&mut conn);
        let day = crate::xplog::day(&mut conn, today).unwrap();
        assert!(day.runs.iter().flat_map(|r| r.entries.iter()).any(|e| e.label == "Say it: 3 of 5 sentences right"));
    }

    #[test]
    fn a_song_s_words_have_their_meanings() {
        let mut conn = scratch_db("glossary");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        tutor::run_tool_for_test(
            &mut conn,
            "create_drill_lesson",
            serde_json::json!({ "title": "Kaffe", "format": "words", "complexity": "low", "course_id": basics,
                "words": [{ "da": "kaffe", "en": "coffee", "pos": "n-common" }, { "da": "mælk", "en": "milk", "pos": "n-common" }] }),
        );
        let mut draft = songs::SongDraft {
            title: "Kaffe".into(),
            course_id: Some(basics),
            sections: vec![songs::SongSection {
                kind: "chorus".into(),
                lines: vec![songs::SongLine { en: "I drink the coffee with the milk".into(), da: "Jeg drikker kaffen med mælken, tralala!".into(), ..Default::default() }],
            }],
            ..Default::default()
        };
        draft.origin.given.words = vec!["at drikke = to drink".into()];
        let song = songs::insert_song(&mut conn, &draft, "V6").unwrap();
        let glossary = crate::singalong::glossary(&mut conn, song).unwrap();
        let find = |form: &str| glossary.iter().find(|g| g.form == form);
        let coffee = find("kaffen").expect("kaffen is kaffe");
        assert_eq!((coffee.da.as_str(), coffee.en.as_str(), coffee.lesson), ("kaffe", "coffee", true));
        assert!(coffee.word_id.is_some());
        assert_eq!(find("mælken").map(|g| g.en.as_str()), Some("milk"));
        assert_eq!(find("drikker").map(|g| (g.da.as_str(), g.en.as_str())), Some(("at drikke", "to drink")), "the song's own words, without their at");
        assert!(find("tralala").is_none(), "no meaning, no entry: the tutor is offered instead");
    }

    #[test]
    fn quick_games_pay_a_little_and_leave_the_schedule_alone() {
        let mut conn = scratch_db("quick-games");
        let overview = crate::games::overview(&mut conn).unwrap();
        if overview.words_met < overview.min_words {
            assert!(crate::games::deal(&mut conn, "match", &mut rand::rng()).is_err(), "no game without words met");
        }
        // Meet some words.
        let met: Vec<(WordId, String)> = crate::schema::words::table
            .select((crate::schema::words::id, crate::schema::words::da))
            .limit(10)
            .load(&mut conn)
            .unwrap();
        assert!(met.len() >= 8);
        for (id, da) in &met {
            practice::submit_answer_in(
                &mut conn,
                practice::SubmitAnswer {
                    kind: "recognise".into(),
                    item_kind: "word".into(),
                    item_id: id.to_string(),
                    expected: da.clone(),
                    given: da.clone(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let reviews = |conn: &mut Conn| {
            crate::schema::reviews::table
                .select((crate::schema::reviews::item_id, crate::schema::reviews::due_day, crate::schema::reviews::reps))
                .order(crate::schema::reviews::item_id)
                .load::<(String, i64, i64)>(conn)
                .unwrap()
        };
        let before = reviews(&mut conn);
        let game = crate::games::deal(&mut conn, "match", &mut rand::rng()).unwrap();
        assert!(game.pairs.len() >= crate::games::MIN_PAIRS);
        assert!(crate::games::deal(&mut conn, "chess", &mut rand::rng()).is_err());

        // A first game sets the best; a slower one isn't a record; a faster one is.
        let first = crate::games::finish(&mut conn, "match", 8, 40_000, 2).unwrap();
        assert!(!first.record);
        assert_eq!((first.xp_gained, first.best_ms), (10, Some(40_000)));
        let slower = crate::games::finish(&mut conn, "match", 8, 50_000, 0).unwrap();
        assert!(!slower.record);
        let faster = crate::games::finish(&mut conn, "match", 8, 30_000, 0).unwrap();
        assert!(faster.record);
        assert_eq!(faster.previous_ms, Some(40_000));
        // The day's cap: a fourth game earns nothing.
        let capped = crate::games::finish(&mut conn, "flip", 6, 30_000, 9).unwrap();
        assert_eq!((capped.xp_gained, capped.xp_today), (0, crate::games::GAME_XP_PER_DAY));
        assert_eq!(capped.best_moves, Some(9));
        let fewer = crate::games::finish(&mut conn, "flip", 6, 60_000, 7).unwrap();
        assert!(fewer.record, "fewer moves beat a faster game");

        // The day's XP screen names them, and no review moved.
        let today = db::today(&mut conn);
        let day = crate::xplog::day(&mut conn, today).unwrap();
        let games: Vec<_> = day.runs.iter().filter(|r| r.lesson_title == "Quick game").collect();
        assert_eq!(games.len(), 5);
        assert_eq!(games.iter().map(|r| r.xp).sum::<i64>(), crate::games::GAME_XP_PER_DAY);
        assert_eq!(reviews(&mut conn), before);
        let overview = crate::games::overview(&mut conn).unwrap();
        assert_eq!((overview.match_best_ms, overview.flip_best_moves), (Some(30_000), Some(7)));
    }

    // Spec: LSN-11
    #[test]
    fn every_lesson_type_can_be_made_and_practised() {
        let mut conn = scratch_db("lesson-types");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let word = |da: &str, en: &str| courses::NewWord { da: da.into(), en: en.into(), pos: "n-common".into(), ..Default::default() };
        let sentence = |da: &str, en: &str| courses::NewSentence { da: da.into(), en: en.into(), ..Default::default() };
        let line = |speaker: &str, da: &str, en: &str| courses::NewLine { speaker: speaker.into(), gender: "f".into(), da: da.into(), en: en.into() };
        let screens = include_str!("../../src/lib/types.ts");
        let mut ids = std::collections::HashSet::new();
        for (n, kind) in lesson_types::REGISTRY.iter().enumerate() {
            assert!(ids.insert(kind.id), "two types called {}", kind.id);
            assert!(tutor::rules().contains(&format!("- {} (", kind.id)), "the tutor knows {}", kind.id);
            assert!(!kind.sections.is_empty() && !kind.description.is_empty());
            let lesson = if kind.is_drill() {
                let items = courses::Items {
                    words: (0..6).map(|i| word(&format!("ord{n}x{i}"), &format!("word {n}/{i}"))).collect(),
                    sentences: [
                        ("Min bror kan lide kaffe.", "My brother likes coffee."),
                        ("I dag regner det.", "Today it is raining."),
                        ("Hvor er stationen?", "Where is the station?"),
                        ("Jeg køber brød hos bageren.", "I buy bread at the bakery."),
                        ("Vi bor i et lille hus.", "We live in a small house."),
                        ("Jeg læser en spændende bog.", "I am reading an interesting book."),
                    ]
                    .iter()
                    .map(|(da, en)| sentence(&format!("{da} {n}"), en))
                    .collect(),
                    dialogue: (0..4).map(|i| line(if i % 2 == 0 { "Ola" } else { "Jan" }, &format!("Linje {n}x{i} i samtalen."), &format!("Line {i}."))).collect(),
                };
                let (lesson, _) = lessons::create_drill(&mut conn, course, kind.name, "", kind.id, "low", &items, None, "tutor").unwrap();
                if kind.id == "pictures" {
                    for id in t::words::table.filter(t::words::lesson_id.eq(lesson.id)).select(t::words::id).load::<WordId>(&mut conn).unwrap() {
                        let file = files::store(&mut conn, files::WORD_IMAGE, &format!("p{id}.jpg"), "image/jpeg", id.to_string().as_bytes()).unwrap();
                        diesel::update(t::words::table.find(id)).set(t::words::image_file_id.eq(file)).execute(&mut conn).unwrap();
                    }
                }
                let plan = session::build_lesson(&mut conn, lesson.id).unwrap();
                assert!(!plan.exercises.is_empty(), "{} builds exercises", kind.id);
                for exercise in &plan.exercises {
                    assert!(screens.contains(&format!("\"{}\"", exercise.kind)), "the screens know the {} exercise ({})", exercise.kind, kind.id);
                }
                lesson
            } else {
                let lesson = match kind.id {
                    "study" => lessons::create_study(&mut conn, serde_json::from_value(serde_json::json!({
                        "title": "At være", "complexity": "low", "content": "Jeg er, du er.", "objectives": ["Use at være"], "course_id": course
                    })).unwrap(), "tutor").unwrap(),
                    "writing" => lessons::create_writing(&mut conn, serde_json::from_value(serde_json::json!({
                        "title": "Neighbour", "complexity": "low", "content": "Meet a neighbour.", "objectives": ["Say your name"], "course_id": course
                    })).unwrap(), "tutor").unwrap(),
                    "listening" => {
                        let song = songs::insert_song(&mut conn, &songs::SongDraft { title: "Kaffe".into(), ..Default::default() }, "V6").unwrap();
                        lessons::create_listening(&mut conn, song, None, None, Some(course), "tutor").unwrap()
                    }
                    other => panic!("add a sample for the new conversation type '{other}' here"),
                };
                assert!(session::build_lesson(&mut conn, lesson.id).is_err(), "{} is not drilled", kind.id);
                lesson
            };
            assert_eq!((lesson.lesson_type.as_str(), lesson.engine), (kind.id, kind.engine()));
            assert_eq!(lesson.period_days, kind.default_period(&lesson.complexity), "{}", kind.id);
        }
        assert_eq!(lesson_types::lesson_types().len(), lesson_types::REGISTRY.len());
    }

    #[test]
    fn picture_lessons_link_each_word_with_its_picture() {
        let mut conn = scratch_db("pictures");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let word = |da: &str, en: &str| courses::NewWord { da: da.into(), en: en.into(), pos: "n-common".into(), ..Default::default() };
        let items = courses::Items {
            words: vec![word("bord", "table"), word("stol", "chair"), word("krus", "mug"), word("tallerken", "plate"), word("gaffel", "fork")],
            sentences: vec![],
            dialogue: vec![],
        };
        let (lesson, _) = lessons::create_drill(&mut conn, course, "In the kitchen", "", "pictures", "low", &items, None, "tutor").unwrap();
        assert_eq!(pictures::missing(&mut conn, lesson.id).unwrap().len(), 5, "a picture lesson draws every word");

        // Four of the five have their pictures; the fifth is not drawn yet.
        let ids: Vec<WordId> = t::words::table.filter(t::words::lesson_id.eq(lesson.id)).order(t::words::ord).select(t::words::id).load(&mut conn).unwrap();
        for (n, id) in ids.iter().take(4).enumerate() {
            let file = files::store(&mut conn, files::WORD_IMAGE, &format!("w{n}.jpg"), "image/jpeg", &[u8::try_from(n).unwrap()]).unwrap();
            diesel::update(t::words::table.find(id)).set(t::words::image_file_id.eq(file)).execute(&mut conn).unwrap();
        }
        assert_eq!(pictures::missing(&mut conn, lesson.id).unwrap(), [ids[4]]);
        assert!(files::referenced(&mut conn).unwrap().len() >= 4, "word pictures are kept, not swept as orphans");

        let plan = session::build_lesson(&mut conn, lesson.id).unwrap();
        let kinds: Vec<&str> = plan.exercises.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds.iter().filter(|k| **k == "pictureIntro").count(), 4, "{kinds:?}");
        assert!(kinds.iter().all(|k| ["pictureIntro", "picturePick", "wordPick", "memory"].contains(k)), "word and picture only, and the undrawn word waits: {kinds:?}");
        // Known words come back the other way round: the word, pick its picture.
        for id in &ids[..4] {
            for _ in 0..1 {
                practice::submit_answer_in(&mut conn, practice::SubmitAnswer { kind: "picturePick".into(), item_kind: "word".into(), item_id: id.to_string(), expected: "x".into(), given: "x".into(), ..Default::default() }).unwrap();
            }
        }
        let again = session::build_lesson(&mut conn, lesson.id).unwrap();
        let pick = again.exercises.iter().find(|e| e.kind == "wordPick").expect("see the word, pick its picture");
        assert_eq!(pick.options.len(), 4);
        assert_eq!(pick.option_images.iter().flatten().count(), 4, "every option is a picture");
        assert!(pick.options.contains(&pick.answer));
        let intro = plan.exercises.iter().find(|e| e.kind == "pictureIntro").unwrap();
        assert!(intro.image.is_some() && !intro.meaning.is_empty(), "the English is shown with it");
        let memory = plan.exercises.iter().find(|e| e.kind == "memory").expect("a memory round of pictures");
        assert!(memory.pairs.iter().all(|p| p.image.is_some()));

        // Meeting a picture is reading, not answering.
        let met = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "pictureIntro".into(),
            item_kind: "word".into(),
            item_id: ids[0].to_string(),
            is_new: true,
            expected: "bord".into(),
            given: "bord".into(),
            ..Default::default()
        }).unwrap();
        assert_eq!(met.xp_gained, 0);

        // Word drills are as they were: no pictures to draw, no picture exercises.
        let drill_items = courses::Items {
            words: vec![word("brød", "bread"), courses::NewWord { da: "at spise".into(), en: "to eat".into(), pos: "v".into(), ..Default::default() }],
            sentences: vec![],
            dialogue: vec![],
        };
        let (drill, _) = lessons::create_drill(&mut conn, course, "Food", "", "words", "low", &drill_items, None, "tutor").unwrap();
        assert!(pictures::missing(&mut conn, drill.id).unwrap().is_empty());
        let plain = session::build_lesson(&mut conn, drill.id).unwrap();
        assert!(plain.exercises.iter().all(|e| !e.kind.starts_with("picture") && e.kind != "wordPick" && e.image.is_none()));
    }

    /// Stand-ins for the image and vision models: one picture, and the
    /// vision model's replies in turn.
    struct SceneStub {
        picture: Vec<u8>,
        replies: std::sync::Mutex<Vec<String>>,
        draws: std::sync::atomic::AtomicUsize,
        looks: std::sync::atomic::AtomicUsize,
    }

    impl SceneStub {
        fn new(replies: &[&str]) -> Self {
            let mut png = Vec::new();
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(800, 600, image::Rgb([240, 230, 200])))
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            Self {
                picture: png,
                replies: std::sync::Mutex::new(replies.iter().map(|r| r.to_string()).collect()),
                draws: Default::default(),
                looks: Default::default(),
            }
        }
    }

    impl scenes::Models for SceneStub {
        fn draw(&self, prompt: &str) -> impl std::future::Future<Output = error::Result<(String, Vec<u8>, Option<f64>)>> + Send {
            assert!(prompt.contains("table") && prompt.contains("The cat is under the table."), "{prompt}");
            self.draws.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let picture = self.picture.clone();
            async move { Ok(("image/png".to_string(), picture, Some(0.04))) }
        }

        fn look(&self, prompt: &str, mime: &str, image: &[u8]) -> impl std::future::Future<Output = error::Result<ai::Reply>> + Send {
            assert!(prompt.contains("1. table (Danish: et bord)") && mime == "image/jpeg" && !image.is_empty());
            self.looks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut replies = self.replies.lock().unwrap();
            let text = if replies.is_empty() { "no idea".to_string() } else { replies.remove(0) };
            async move { Ok(ai::Reply { text, truncated: false, cost: Some(0.001), tokens: ai::Tokens { input: 1200, output: 300, cached: 0 } }) }
        }

        fn image_model(&self) -> &str {
            "stub/image"
        }

        fn vision_model(&self) -> &str {
            "stub/vision"
        }
    }

    fn make_scene(db: &Db, lesson_id: LessonId, redraw: bool, stub: &SceneStub) -> error::Result<usize> {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        runtime.block_on(scenes::make(db, lesson_id, redraw, stub, &|_| {}))
    }

    #[test]
    fn scene_lessons_are_drawn_found_and_practised() {
        let mut conn = scratch_db("scenes");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let thing = |da: &str, en: &str, pos: &str| serde_json::json!({ "da": da, "en": en, "pos": pos });
        let things = vec![
            thing("et bord", "table", "n-neuter"),
            thing("en kat", "cat", "n-common"),
            thing("en stol", "chair", "n-common"),
            thing("en lampe", "lamp", "n-common"),
            thing("et vindue", "window", "n-neuter"),
            thing("et ur", "clock", "n-neuter"),
            thing("et krus", "mug", "n-neuter"),
        ];
        let under = serde_json::json!({ "da": "Katten er under bordet.", "en": "The cat is under the table.", "subject": "en kat",
            "preposition": "under", "object": "et bord", "phrase": "under bordet", "wrong": ["på bordet", "ved siden af bordet"] });
        let above = serde_json::json!({ "da": "Lampen er over bordet.", "en": "The lamp is above the table.", "subject": "en lampe",
            "preposition": "over", "object": "et bord", "phrase": "over bordet", "wrong": ["under bordet"] });
        let on = serde_json::json!({ "da": "Kruset er på bordet.", "en": "The mug is on the table.", "subject": "et krus",
            "preposition": "på", "object": "et bord", "phrase": "på bordet", "wrong": ["under bordet"] });
        let args = |things: &[serde_json::Value], statements: &[serde_json::Value]| serde_json::json!({
            "title": "I køkkenet", "scene": "A sunny kitchen in the morning", "complexity": "medium",
            "things": things, "statements": statements, "course_id": course,
        });

        // The tutor's mistakes come back to it whole, and nothing is made.
        let lessons_before = t::lessons::table.count().get_result::<i64>(&mut conn).unwrap();
        let few = tutor::run_tool_for_test(&mut conn, "create_scene_lesson", args(&things[..4], &[]));
        assert!(few["error"].as_str().unwrap().contains("6–10 things"), "{few}");
        let mut verb = things.clone();
        verb[6] = thing("at spise", "to eat", "v");
        let verb = tutor::run_tool_for_test(&mut conn, "create_scene_lesson", args(&verb, &[]));
        assert!(verb["error"].as_str().unwrap().contains("not a thing you can point at"), "{verb}");
        let mut stray = under.clone();
        stray["object"] = "en hund".into();
        let stray = tutor::run_tool_for_test(&mut conn, "create_scene_lesson", args(&things, &[stray]));
        assert!(stray["error"].as_str().unwrap().contains("two of the scene's things"), "{stray}");
        let mut unseen = under.clone();
        unseen["preposition"] = "bag".into();
        assert!(tutor::run_tool_for_test(&mut conn, "create_scene_lesson", args(&things, &[unseen]))["error"].is_string());
        let mut misquoted = under.clone();
        misquoted["phrase"] = "under stolen".into();
        assert!(tutor::run_tool_for_test(&mut conn, "create_scene_lesson", args(&things, &[misquoted]))["error"].is_string());
        assert_eq!(t::lessons::table.count().get_result::<i64>(&mut conn).unwrap(), lessons_before);

        let made = tutor::run_tool_for_test(&mut conn, "create_scene_lesson", args(&things, &[under, above, on]));
        assert!(made["error"].is_null(), "{made}");
        let lesson_id: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let lesson = lessons::load(&mut conn, lesson_id).unwrap();
        assert_eq!((lesson.lesson_type.as_str(), lesson.items, lesson.course_id), ("scene", 7, Some(course)));
        assert_eq!(lesson.period_days, lesson_types::get("scene").unwrap().default_period("medium"));
        let detail = lessons::detail(&mut conn, lesson_id).unwrap();
        let scene = detail.scene.expect("a scene lesson has its scene");
        assert_eq!((scene.status.as_str(), scene.card.is_none()), (scenes::DRAWING, true));
        let other = drill_of(&mut conn, course);
        assert!(lessons::detail(&mut conn, other).unwrap().scene.is_none(), "other lessons have none");

        // Until it is drawn, the words are practised as a word drill.
        let waiting = session::build_lesson(&mut conn, lesson_id).unwrap();
        assert!(!waiting.exercises.is_empty() && waiting.exercises.iter().all(|e| !e.kind.starts_with("scene") && e.scene.is_none()));

        // Drawn once; the first look finds too few, the second all but the clock.
        let box_of = |n: usize, b: [f64; 4]| format!(r#"{{"n": {n}, "found": true, "x_min": {}, "y_min": {}, "x_max": {}, "y_max": {}}}"#, b[0], b[1], b[2], b[3]);
        let table = [0.30, 0.45, 0.70, 0.75];
        let boxes = [
            box_of(1, table),
            box_of(2, [0.42, 0.62, 0.55, 0.78]),
            box_of(3, [0.75, 0.40, 0.90, 0.80]),
            box_of(4, [0.05, 0.05, 0.15, 0.30]),
            box_of(5, [0.05, 0.40, 0.25, 0.60]),
            r#"{"n": 6, "found": false, "x_min": 0, "y_min": 0, "x_max": 0, "y_max": 0}"#.to_string(),
            box_of(7, [0.50, 0.38, 0.56, 0.46]),
        ];
        let few = format!(r#"{{"things": [{}]}}"#, boxes[..3].join(","));
        let good = format!(r#"{{"things": [{}]}}"#, boxes.join(","));
        let db = Db::new(conn);
        let stub = SceneStub::new(&[&few, &good]);
        assert_eq!(make_scene(&db, lesson_id, false, &stub).unwrap(), 6);
        assert_eq!((stub.draws.load(std::sync::atomic::Ordering::SeqCst), stub.looks.load(std::sync::atomic::Ordering::SeqCst)), (1, 2));
        let mut guard = db.lock();
        let conn = &mut *guard;
        let lesson = lessons::load(conn, lesson_id).unwrap();
        assert_eq!(lesson.items, 6, "the clock was not found: it left the lesson");
        let scene = lessons::detail(conn, lesson_id).unwrap().scene.unwrap();
        assert_eq!((scene.status.as_str(), scene.error.as_str()), (scenes::READY, ""));
        let card = scene.card.unwrap();
        assert_eq!((card.width, card.height), (800, 600), "kept at its size, as JPEG");
        assert_eq!(card.spots.len(), 6);
        assert!(files::referenced(conn).unwrap().contains(&card.image), "the scene's picture is kept, not swept as an orphan");
        // The lamp is not above the table in this picture: that sentence goes.
        assert_eq!(scene.statements.iter().map(|(da, _)| da.as_str()).collect::<Vec<_>>(), ["Katten er under bordet.", "Kruset er på bordet."]);
        let costs: Vec<(String, String)> =
            t::ai_costs::table.select((t::ai_costs::feature, t::ai_costs::model)).load(conn).unwrap();
        assert!(costs.contains(&(costs::PICTURES.into(), "stub/image".into())));
        assert_eq!(costs.iter().filter(|(f, m)| f == costs::SCENE_SPOTS && m == "stub/vision").count(), 2, "every look is paid for");

        // Practised: explore it, find each thing, name each, say where.
        let screens = include_str!("../../src/lib/types.ts");
        let plan = session::build_lesson(conn, lesson_id).unwrap();
        let kinds: Vec<&str> = plan.exercises.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds[0], "sceneExplore", "{kinds:?}");
        assert_eq!(kinds.iter().filter(|k| **k == "sceneFind").count(), 6, "{kinds:?}");
        assert_eq!(kinds.iter().filter(|k| **k == "sceneName").count(), 6, "new words pick their names: {kinds:?}");
        assert_eq!(kinds.iter().filter(|k| **k == "sceneWhere").count(), 2, "{kinds:?}");
        for exercise in &plan.exercises {
            assert!(screens.contains(&format!("\"{}\"", exercise.kind)), "the screens know {}", exercise.kind);
            assert!(exercise.scene.as_ref().is_some_and(|c| c.image == card.image), "every card shows the scene");
        }
        let find = plan.exercises.iter().find(|e| e.kind == "sceneFind" && e.da == "et bord").unwrap();
        assert_eq!(find.prompt, "Hvor er der et bord?");
        assert!(find.scene.as_ref().unwrap().spots.iter().all(|s| !s.lit), "nothing is pointed out when finding");
        let name = plan.exercises.iter().find(|e| e.kind == "sceneName").unwrap();
        assert_eq!(name.options.len(), 4);
        assert!(name.options.contains(&name.answer));
        let lit: Vec<&str> = name.scene.as_ref().unwrap().spots.iter().filter(|s| s.lit).map(|s| s.da.as_str()).collect();
        assert_eq!(lit, [name.answer.as_str()], "the thing to name lights up");
        for card in plan.exercises.iter().filter(|e| e.kind == "sceneWhere") {
            assert!(card.options.contains(&card.answer), "{card:?}");
            assert_eq!(card.scene.as_ref().unwrap().spots.iter().filter(|s| s.lit).count(), 2, "both things are pointed out");
            if card.options == ["Sandt", "Falsk"] {
                assert_eq!(card.answer == "Sandt", card.prompt == card.da, "{card:?}");
            } else {
                assert!(card.prompt.contains('…') && card.da.contains(&card.answer), "{card:?}");
            }
        }

        // Scored like its drill siblings: XP for finding, reviews for the
        // words, none for exploring or the sentences (not scheduled items).
        let explored = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "sceneExplore".into(), item_kind: "set".into(), item_id: lesson_id.to_string(), is_new: true,
            expected: String::new(), given: String::new(), ..Default::default()
        }).unwrap();
        assert_eq!(explored.xp_gained, 0);
        let found = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "sceneFind".into(), item_kind: "word".into(), item_id: find.item_id.clone(), is_new: true,
            expected: "et bord".into(), given: "et bord".into(), lesson_id: Some(lesson_id), ..Default::default()
        }).unwrap();
        assert!(found.correct && found.xp_gained == 8, "{found:?}");
        let missed = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "sceneFind".into(), item_kind: "word".into(), item_id: find.item_id.clone(),
            expected: "et bord".into(), given: "en stol".into(), ..Default::default()
        }).unwrap();
        assert!(!missed.correct && missed.xp_gained == 0);
        let said = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "sceneWhere".into(), item_kind: "set".into(), item_id: lesson_id.to_string(),
            expected: "Sandt".into(), given: "Sandt".into(), ..Default::default()
        }).unwrap();
        assert_eq!(said.xp_gained, 8);
        let reviewed: Vec<String> = t::reviews::table.select(t::reviews::item_id).load(conn).unwrap();
        assert!(reviewed.contains(&find.item_id) && !reviewed.contains(&lesson_id.to_string()));
        let scored = plan.exercises.iter().filter(|e| e.kind != "sceneExplore").count();
        let finished = practice::lesson_finish_in(conn, lesson_id, i64::try_from(scored).unwrap(), i64::try_from(scored).unwrap()).unwrap();
        assert_eq!(finished.result, "pass");
        assert!(finished.lesson.due_day.is_some() && finished.lesson.mastery > 0, "scheduled and on its way to mastered");

        // Known now: some things come back to be found, others to be named
        // — typed, in a medium lesson, once they have come back twice.
        let words: Vec<WordId> = t::words::table.filter(t::words::lesson_id.eq(lesson_id)).select(t::words::id).load(conn).unwrap();
        // Over three days: a word counts once a day.
        for _ in 0..3 {
            for id in &words {
                practice::submit_answer_in(conn, practice::SubmitAnswer { kind: "sceneFind".into(), item_kind: "word".into(), item_id: id.to_string(), ..Default::default() }).unwrap();
            }
            conn.batch_execute("UPDATE reviews SET last_day = last_day - 1, due_day = due_day - 1").unwrap();
        }
        let again = session::build_lesson(conn, lesson_id).unwrap();
        let kinds: Vec<&str> = again.exercises.iter().map(|e| e.kind.as_str()).collect();
        assert!(!kinds.contains(&"sceneExplore"), "nothing new to explore: {kinds:?}");
        assert!(kinds.contains(&"sceneType") && !kinds.contains(&"sceneName"), "{kinds:?}");
        assert!(again.exercises.iter().filter(|e| e.kind == "sceneType").all(|e| e.options.is_empty()));
        drop(guard);

        // A picture whose things cannot be found: looked at twice, drawn
        // twice, then a clear failure, with a reason, to retry.
        let stub = SceneStub::new(&["nothing", r#"{"things": []}"#, "still nothing", "no"]);
        let failed = make_scene(&db, lesson_id, true, &stub).unwrap_err();
        assert!(failed.to_string().contains("could be found"), "{failed}");
        assert_eq!(stub.draws.load(std::sync::atomic::Ordering::SeqCst), 2);
        let scene = lessons::detail(&mut db.lock(), lesson_id).unwrap().scene.unwrap();
        assert_eq!(scene.status, scenes::FAILED);
        assert!(scene.error.contains("could be found") && scene.card.is_none());
        assert!(!session::build_lesson(&mut db.lock(), lesson_id).unwrap().exercises.iter().any(|e| e.scene.is_some()), "a word drill meanwhile");
        // Retried on the same picture: found this time, nothing redrawn. (The
        // things are numbered afresh: the clock has gone.)
        let six = format!(r#"{{"things": [{}, {}]}}"#, boxes[..5].join(","), box_of(6, [0.50, 0.38, 0.56, 0.46]));
        let stub = SceneStub::new(&[&six]);
        assert_eq!(make_scene(&db, lesson_id, false, &stub).unwrap(), 6);
        assert_eq!(stub.draws.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(lessons::detail(&mut db.lock(), lesson_id).unwrap().scene.unwrap().status, scenes::READY);
    }


    /// A story lesson as the tutor writes one: Ida looks for her dog Bamse,
    /// in five panels.
    fn story_args(course: CourseId, complexity: &str) -> serde_json::Value {
        let q = |prompt: &str, answer: &str, wrong: &[&str]| serde_json::json!({ "prompt": prompt, "answer": answer, "wrong": wrong });
        let line = |da: &str, en: &str| serde_json::json!({ "da": da, "en": en });
        let said = |who: &str, da: &str, en: &str| serde_json::json!({ "da": da, "en": en, "speaker": who });
        serde_json::json!({
            "title": "Hvor er Bamse?", "summary": "Ida looks for her dog.", "complexity": complexity,
            "theme": "A girl looks for her lost dog in the rain and finds him at home.",
            "setting": "a small Danish town in autumn",
            "course_id": course,
            "characters": [
                { "name": "Ida", "look": "a girl of about ten with a red raincoat and two dark plaits", "gender": "f" },
                { "name": "Bamse", "look": "a small brown dog with one white ear", "gender": "m" },
                { "name": "Far", "look": "a tall man with a grey beard and a green jumper", "gender": "m" }
            ],
            "panels": [
                { "picture": "Ida stands at the door of a house in the rain, looking worried.",
                  "lines": [line("Det regner.", "It is raining."), said("Ida", "Hvor er min hund?", "Where is my dog?")],
                  "question": q("Who is looking for the dog?", "Ida", &["Far", "Bamse"]) },
                { "picture": "Ida walks through a park with an umbrella.",
                  "lines": [line("Ida går til parken.", "Ida goes to the park."), said("Ida", "Bamse! Bamse!", "Bamse! Bamse!")],
                  "question": q("Where does Ida go?", "to the park", &["to school", "to the shop"]) },
                { "picture": "Ida asks a baker in a bakery; bread on the shelves.",
                  "lines": [line("Ida spørger bageren.", "Ida asks the baker."), said("Ida", "Har du set en hund?", "Have you seen a dog?")],
                  "question": q("Who does Ida ask?", "the baker", &["a teacher", "her father"]) },
                { "picture": "Ida comes home sad; Far opens the door.",
                  "lines": [line("Ida går hjem igen.", "Ida goes back home."), said("Far", "Kom og se!", "Come and see!")],
                  "question": q("Who opens the door?", "Far", &["Bamse", "the baker"]) },
                { "picture": "Bamse sleeps by the warm stove in the kitchen; Ida laughs.",
                  "lines": [{ "da": "Bamse sover i køkkenet.", "en": "Bamse is sleeping in the kitchen.", "practise": true }, said("Ida", "Min hund er hjemme!", "My dog is at home!")],
                  "question": q("Where is Bamse?", "in the kitchen", &["in the park", "in the bakery"]) }
            ],
            "words": [
                { "da": "en hund", "en": "dog", "pos": "n-common" },
                { "da": "at regne", "en": "to rain", "pos": "v" },
                { "da": "en park", "en": "park", "pos": "n-common" },
                { "da": "en bager", "en": "baker", "pos": "n-common" },
                { "da": "et køkken", "en": "kitchen", "pos": "n-neuter" },
                { "da": "at sove", "en": "to sleep", "pos": "v" }
            ]
        })
    }

    /// A stand-in image model: a picture per call, except the calls listed
    /// in `fail` (counted from 1), which fail.
    struct Painter {
        fail: Vec<usize>,
        prompts: std::sync::Mutex<Vec<String>>,
    }

    impl stories::Painter for Painter {
        fn draw(&self, prompt: &str) -> impl std::future::Future<Output = error::Result<(String, Vec<u8>, Option<f64>)>> + Send {
            let mut prompts = self.prompts.lock().unwrap();
            prompts.push(prompt.to_string());
            let fails = self.fail.contains(&prompts.len());
            let mut png = Vec::new();
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1600, 1200, image::Rgb([200, 220, 240])))
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            async move {
                if fails {
                    Err(error::Error::msg("The image model returned no picture."))
                } else {
                    Ok(("image/png".to_string(), png, Some(0.04)))
                }
            }
        }

        fn model(&self) -> &str {
            "stub/image"
        }
    }

    fn draw_story(db: &Db, lesson_id: LessonId, painter: &Painter) -> stories::Drawn {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let seen = std::sync::Mutex::new(Vec::new());
        let drawn = runtime.block_on(stories::draw_panels(db, lesson_id, painter, &|panel, status| seen.lock().unwrap().push((panel, status.to_string())))).unwrap();
        assert!(seen.lock().unwrap().iter().any(|(_, s)| s == stories::DRAWING), "every step is reported");
        drawn
    }

    #[test]
    fn story_lessons_are_checked_drawn_panel_by_panel_and_scored() {
        let mut conn = scratch_db("stories");
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let lessons_before = t::lessons::table.count().get_result::<i64>(&mut conn).unwrap();

        // The tutor's mistakes come back to it together, and nothing is made.
        let mut bad = story_args(course, "low");
        bad["panels"] = serde_json::Value::Array(bad["panels"].as_array().unwrap()[..3].to_vec());
        bad["panels"][1]["lines"][1]["speaker"] = "Karen".into();
        bad["panels"][2]["question"]["wrong"] = serde_json::json!(["the baker"]);
        bad["words"][0]["da"] = "en kat".into();
        bad["words"][1]["da"] = "en skole".into();
        bad["words"][2]["da"] = "en cykel".into();
        bad["characters"][0]["look"] = "".into();
        let refused = tutor::run_tool_for_test(&mut conn, "create_story_lesson", bad);
        let error = refused["error"].as_str().unwrap();
        for problem in ["4–8 panels (got 3)", "said by Karen", "Panel 3: the question's answers must all differ", "not in the story: en kat, en skole, en cykel", "how they look"] {
            assert!(error.contains(problem), "{problem}: {error}");
        }
        let mut long = story_args(course, "low");
        for panel in long["panels"].as_array_mut().unwrap() {
            panel["lines"][0]["practise"] = true.into();
        }
        assert!(tutor::run_tool_for_test(&mut conn, "create_story_lesson", long)["error"].as_str().unwrap().contains("at most 4 sentences"));
        assert_eq!(t::lessons::table.count().get_result::<i64>(&mut conn).unwrap(), lessons_before);
        assert_eq!(t::story_panels::table.count().get_result::<i64>(&mut conn).unwrap(), 0);

        // Made: six words and one sentence to practise; five panels to read
        // at once, waiting for their pictures.
        let made = tutor::run_tool_for_test(&mut conn, "create_story_lesson", story_args(course, "low"));
        assert!(made["error"].is_null(), "{made}");
        let lesson_id: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let lesson = lessons::load(&mut conn, lesson_id).unwrap();
        assert_eq!((lesson.lesson_type.as_str(), lesson.items, lesson.course_id), ("story", 7, Some(course)));
        assert_eq!(lesson.period_days, lesson_types::get("story").unwrap().default_period("low"));
        let detail = lessons::detail(&mut conn, lesson_id).unwrap();
        assert_eq!(detail.sentences, [("Bamse sover i køkkenet.".to_string(), "Bamse is sleeping in the kitchen.".to_string())]);
        let story = detail.story.expect("a story lesson has its story");
        assert_eq!(story.panels.len(), 5);
        assert!(story.panels.iter().all(|p| p.status == stories::WAITING && p.image.is_none()));
        assert_eq!(story.panels[0].lines[1].gender, "f", "Ida's line in her voice");
        assert_eq!(story.panels[3].lines[1].gender, "m", "Far's in his");
        assert_eq!(story.panels[0].lines[0].gender, "", "narration in the narrator's");
        // Forms find their words: "hund" is "en hund", "køkkenet" is "et køkken", "regner" is "at regne".
        let gloss = |form: &str| story.glossary.iter().find(|g| g.form == form).cloned();
        assert!(gloss("hund").is_some_and(|g| g.lesson && g.word_id.is_some()));
        assert!(gloss("køkkenet").is_some_and(|g| g.da == "et køkken" && g.lesson), "{:?}", gloss("køkkenet"));
        assert!(gloss("regner").is_some_and(|g| g.da == "at regne"), "{:?}", gloss("regner"));
        let other = drill_of(&mut conn, course);
        assert!(lessons::detail(&mut conn, other).unwrap().story.is_none(), "other lessons have none");
        let shown = tutor::run_tool_for_test(&mut conn, "get_lesson", serde_json::json!({ "lesson_id": lesson_id }));
        assert_eq!(shown["story"]["panels"][4]["lines"][0]["da"], "Bamse sover i køkkenet.", "the tutor can read it");

        // Readable before any picture: every panel, then its question — no
        // picture questions yet — the end, then the words.
        let screens = include_str!("../../src/lib/types.ts");
        let plan = session::build_lesson(&mut conn, lesson_id).unwrap();
        let kinds: Vec<&str> = plan.exercises.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(&kinds[..11], ["storyPanel", "storyQuestion", "storyPanel", "storyQuestion", "storyPanel", "storyQuestion",
            "storyPanel", "storyQuestion", "storyPanel", "storyQuestion", "storyEnd"], "{kinds:?}");
        assert_eq!(kinds.iter().filter(|k| **k == "recognise").count(), 6, "the new words, recognised: {kinds:?}");
        assert!(kinds.contains(&"wordOrder"), "the kept sentence is built: {kinds:?}");
        for exercise in &plan.exercises {
            assert!(screens.contains(&format!("\"{}\"", exercise.kind)), "the screens know {}", exercise.kind);
        }
        let questions: Vec<&models::Exercise> = plan.exercises.iter().filter(|e| e.kind == "storyQuestion").collect();
        assert!(questions.iter().all(|q| q.story.as_ref().unwrap().ask != "picture"), "no pictures to choose from yet");
        assert!(questions.iter().all(|q| q.options.contains(&q.answer) && q.options.len() >= 2), "{questions:?}");
        let first = plan.exercises[0].story.as_ref().unwrap();
        assert_eq!((first.total, first.panel.as_ref().unwrap().number), (5, 1));
        assert!(first.glossary.iter().any(|g| g.form == "hund") && !first.glossary.iter().any(|g| g.form == "køkkenet"), "the panel's own words");
        let end = plan.exercises.iter().find(|e| e.kind == "storyEnd").unwrap().story.as_ref().unwrap();
        assert_eq!(end.words.len(), 6);
        assert!(end.words.iter().all(|w| w.new));

        // Drawn panel by panel: the third fails, the rest are drawn, each
        // with every character's look and the one style in its prompt.
        let db = Db::new(conn);
        let painter = Painter { fail: vec![3], prompts: Default::default() };
        assert_eq!(draw_story(&db, lesson_id, &painter), stories::Drawn { drawn: 4, failed: 1 });
        let prompts = painter.prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 5);
        for (n, prompt) in prompts.iter().enumerate() {
            assert!(prompt.contains(&format!("picture {} of 5", n + 1)), "{prompt}");
            assert!(prompt.contains("Ida — a girl of about ten with a red raincoat") && prompt.contains("Bamse — a small brown dog"), "{prompt}");
            assert!(prompt.contains(stories::STORY_STYLE) && prompt.contains("a small Danish town in autumn"), "{prompt}");
        }
        let mut guard = db.lock();
        let conn = &mut *guard;
        let story = lessons::detail(conn, lesson_id).unwrap().story.unwrap();
        let statuses: Vec<&str> = story.panels.iter().map(|p| p.status.as_str()).collect();
        assert_eq!(statuses, ["ready", "ready", "failed", "ready", "ready"]);
        let failed = &story.panels[2];
        assert!(failed.image.is_none() && failed.error.contains("no picture"), "{failed:?}");
        let drawn = &story.panels[0];
        assert_eq!((drawn.width, drawn.height), (1024, 768), "kept at a sensible size, as JPEG");
        assert!(files::referenced(conn).unwrap().contains(&drawn.image.unwrap()), "a panel's picture is kept, not swept as an orphan");
        let costs: Vec<(String, String)> = t::ai_costs::table.select((t::ai_costs::feature, t::ai_costs::model)).load(conn).unwrap();
        assert_eq!(costs.iter().filter(|(f, m)| f == costs::PICTURES && m == "stub/image").count(), 4, "every picture is paid for");

        // With pictures: one question asks which picture shows a sentence
        // (two pictures from the pages read so far), one which came first.
        let plan = session::build_lesson(conn, lesson_id).unwrap();
        let asks: Vec<&str> = plan.exercises.iter().filter_map(|e| e.story.as_ref()).filter(|s| !s.ask.is_empty()).map(|s| s.ask.as_str()).collect();
        assert_eq!(asks.len(), 5);
        assert_eq!(asks.iter().filter(|a| **a == "picture").count(), 1, "{asks:?}");
        assert_eq!(asks.iter().filter(|a| **a == "order").count(), 1, "{asks:?}");
        let picture = plan.exercises.iter().find(|e| e.story.as_ref().is_some_and(|s| s.ask == "picture")).unwrap();
        assert_eq!(picture.options.len(), 2);
        assert!(picture.options.contains(&picture.answer) && picture.option_images.iter().all(Option::is_some), "{picture:?}");
        let order = plan.exercises.iter().find(|e| e.story.as_ref().is_some_and(|s| s.ask == "order")).unwrap();
        let place = |da: &str| story.panels.iter().position(|p| p.lines.iter().any(|l| l.da == da)).unwrap();
        let other = order.options.iter().find(|o| **o != order.answer).unwrap();
        assert!(place(&order.answer) < place(other), "the answer came first: {order:?}");
        // A panel without its picture still reads, with a placeholder.
        let third = plan.exercises.iter().filter(|e| e.kind == "storyPanel").nth(2).unwrap();
        assert_eq!(third.story.as_ref().unwrap().panel.as_ref().unwrap().status, "failed");

        // Scored: reading pays nothing, a right answer 8 XP, a wrong none;
        // the words are reviewed, the story's questions are not scheduled.
        let read = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "storyPanel".into(), item_kind: "set".into(), item_id: lesson_id.to_string(), is_new: true, ..Default::default()
        }).unwrap();
        assert_eq!((read.correct, read.xp_gained), (true, 0));
        let q = questions[0];
        let right = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "storyQuestion".into(), item_kind: "set".into(), item_id: lesson_id.to_string(),
            expected: q.answer.clone(), given: q.answer.clone(), lesson_id: Some(lesson_id), ..Default::default()
        }).unwrap();
        assert!(right.correct && right.xp_gained == 8, "{right:?}");
        let wrong = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "storyQuestion".into(), item_kind: "set".into(), item_id: lesson_id.to_string(),
            expected: q.answer.clone(), given: q.options.iter().find(|o| **o != q.answer).unwrap().clone(), ..Default::default()
        }).unwrap();
        assert!(!wrong.correct && wrong.xp_gained == 0);
        let chose = practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "storyQuestion".into(), item_kind: "set".into(), item_id: lesson_id.to_string(),
            expected: picture.answer.clone(), given: picture.answer.clone(), ..Default::default()
        }).unwrap();
        assert!(chose.correct, "a picture is chosen by its panel's id");
        let word = plan.exercises.iter().find(|e| e.kind == "recognise").unwrap();
        practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: "recognise".into(), item_kind: "word".into(), item_id: word.item_id.clone(), is_new: true,
            expected: word.answer.clone(), given: word.answer.clone(), ..Default::default()
        }).unwrap();
        let reviewed: Vec<String> = t::reviews::table.select(t::reviews::item_id).load(conn).unwrap();
        assert!(reviewed.contains(&word.item_id) && !reviewed.contains(&lesson_id.to_string()));
        let scored = plan.exercises.iter().filter(|e| e.kind != "storyPanel" && e.kind != "storyEnd").count();
        let finished = practice::lesson_finish_in(conn, lesson_id, i64::try_from(scored).unwrap(), i64::try_from(scored).unwrap()).unwrap();
        assert_eq!(finished.result, "pass");
        assert!(finished.lesson.due_day.is_some() && finished.lesson.mastery > 0, "scheduled and on its way to mastered");
        // Met now: the end lists the words as known, and they come back harder.
        let again = session::build_lesson(conn, lesson_id).unwrap();
        let end = again.exercises.iter().find(|e| e.kind == "storyEnd").unwrap().story.as_ref().unwrap();
        assert!(end.words.iter().any(|w| !w.new));

        // Only this device draws what it wrote: a panel another device
        // wrote is left to that device.
        stories::redraw(conn, lesson_id, None).unwrap();
        let third_id = story.panels[2].id;
        assert_eq!(stories::pending(conn, Some(lesson_id)), [(lesson_id, third_id)]);
        diesel::update(t::sync_rows::table.filter(t::sync_rows::pk.eq(third_id.to_string())))
            .set(t::sync_rows::origin.eq(DeviceId::new().to_string()))
            .execute(conn)
            .unwrap();
        assert!(stories::pending(conn, Some(lesson_id)).is_empty(), "another device's panel");
        drop(guard);

        // Tried again (here: it is ours again): only that panel is drawn.
        stories::redraw(&mut db.lock(), lesson_id, Some(third_id)).unwrap();
        assert!(stories::redraw(&mut db.lock(), lesson_id, Some(PanelId::new())).is_err(), "a panel of another story");
        let painter = Painter { fail: vec![], prompts: Default::default() };
        assert_eq!(draw_story(&db, lesson_id, &painter), stories::Drawn { drawn: 1, failed: 0 });
        assert!(painter.prompts.lock().unwrap()[0].contains("picture 3 of 5"));
        let story = lessons::detail(&mut db.lock(), lesson_id).unwrap().story.unwrap();
        assert!(story.panels.iter().all(|p| p.status == stories::READY && p.image.is_some() && p.error.is_empty()));
    }

    /// The first word drill of a course.
    fn drill_of(conn: &mut Conn, course: CourseId) -> LessonId {
        lessons::for_course(conn, course).unwrap().into_iter().find(|l| l.lesson_type == "words").unwrap().id
    }

    #[test]
    fn word_pictures_are_shrunk_to_a_small_jpeg() {
        let mut big = image::RgbaImage::new(1024, 768);
        for pixel in big.pixels_mut() {
            *pixel = image::Rgba([200, 30, 30, 128]);
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(big).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let small = pictures::shrink(&png).unwrap();
        let back = image::load_from_memory(&small).unwrap();
        assert_eq!((back.width(), back.height()), (512, 384), "fits 512, keeps its shape");
        assert_eq!(image::guess_format(&small).unwrap(), image::ImageFormat::Jpeg);
        assert!(pictures::word_prompt("stol", "chair").contains("No text"));
    }

    #[test]
    fn drafts_stay_out_of_the_library_and_a_made_song_is_never_changed() {
        let mut conn = scratch_db("song-drafts");
        let draft = songs::normalize_draft(songs::SongDraft {
            title: "Kaffe".into(),
            chorus_da: "En kaffe, tak".into(),
            chorus_en: "A coffee, please".into(),
            vocal_gender: "f".into(),
            origin: songs::SongOrigin {
                brief: "Ordering coffee".into(),
                made_by: "tutor".into(),
                requirements: "Only the words of course 1.".into(),
                given: songs::WriterInput { words: vec!["kaffe = coffee".into()], ..Default::default() },
                instructions_version: "v1-test".into(),
                ..Default::default()
            },
            sections: vec![songs::SongSection {
                kind: "chorus".into(),
                lines: vec![songs::SongLine { en: "A coffee, please".into(), da: "En kaffe, tak".into(), ..Default::default() }],
            }],
            ..Default::default()
        })
        .unwrap();

        // A draft is the tutor's to look over: not in the Music tab yet.
        let id = songs::insert_draft(&mut conn, &draft).unwrap();
        assert!(songs::list_songs(&mut conn, false).unwrap().is_empty());
        assert_eq!(songs::list_songs(&mut conn, true).unwrap()[0].status, "draft");
        let stored = songs::load_song(&mut conn, id).unwrap();
        assert_eq!(stored.origin.requirements, "Only the words of course 1.", "what it was made with is kept");
        assert_eq!(stored.origin.given.words, ["kaffe = coffee"]);

        // While it is a draft, it changes in place.
        let (same, mut changed) = songs::editable(&mut conn, id, "tutor").unwrap();
        assert_eq!(same, Some(id));
        changed.title = "Kaffe med mælk".into();
        assert_eq!(songs::store_draft(&mut conn, same, &changed).unwrap(), id);
        assert_eq!(songs::load_song(&mut conn, id).unwrap().title, "Kaffe med mælk");

        // Once made, changing it gives a new draft based on it.
        diesel::update(t::songs::table.find(id)).set(t::songs::status.eq("ready")).execute(&mut conn).unwrap();
        let (new, mut variant) = songs::editable(&mut conn, id, "tutor").unwrap();
        assert_eq!(new, None, "saved only once the change is made");
        assert_eq!(variant.origin.based_on, Some(id));
        assert_eq!(variant.origin.requirements, "Only the words of course 1.", "its requirements still apply");
        variant.origin.steps.push(songs::OriginStep { kind: "edit".into(), by: "tutor".into(), text: "Edited the style".into(), before: songs::bare(&variant), ..Default::default() });
        variant.style = "slow piano ballad".into();
        let new_id = songs::store_draft(&mut conn, new, &variant).unwrap();
        assert_ne!(new_id, id);
        let (old, new) = (songs::load_song(&mut conn, id).unwrap(), songs::load_song(&mut conn, new_id).unwrap());
        assert_eq!((old.status.as_str(), new.status.as_str()), ("ready", "draft"));
        assert_ne!(old.style, new.style, "the made one is untouched");
        let step = &new.origin.steps[0];
        assert_eq!(step.before.as_ref().unwrap().style, old.style, "a step keeps the draft before it");
        assert_eq!(step.before.as_ref().unwrap().origin, songs::SongOrigin::default(), "without nesting origins");
    }

    #[test]
    fn songs_round_trip_with_their_lyrics_and_takes() {
        let mut conn = scratch_db("songs");
        let draft = songs::normalize_draft(songs::SongDraft {
            title: "Kaffe".into(),
            chorus_da: "En kaffe, tak".into(),
            chorus_en: "A coffee, please".into(),
            style: String::new(),
            vocal_gender: "f".into(),
            course_id: None,
            origin: songs::SongOrigin { brief: "Ordering coffee".into(), ..Default::default() },
            kind: String::new(),
            podcast: None,
            sections: vec![songs::SongSection {
                kind: "chorus".into(),
                lines: vec![songs::SongLine { en: "A coffee, please".into(), da: "En kaffe, tak".into(), ..Default::default() }],
            }],
        })
        .unwrap();
        assert!(!draft.style.is_empty(), "a default style is filled in");

        assert_eq!(songs::suno_model(&mut conn), songs::DEFAULT_SUNO_MODEL);
        let id = songs::insert_song(&mut conn, &draft, "V6").unwrap();
        let audio = files::store(&mut conn, files::SONG_AUDIO, "song-1-0.mp3", "audio/mpeg", b"mp3").unwrap();
        diesel::insert_into(t::song_tracks::table)
            .values((
                t::song_tracks::id.eq(TrackId::new()),
                t::song_tracks::song_id.eq(id),
                t::song_tracks::ord.eq(0),
                t::song_tracks::remote_id.eq("r1"),
                t::song_tracks::audio_file_id.eq(audio),
                t::song_tracks::duration.eq(120.5),
            ))
            .execute(&mut conn)
            .unwrap();

        let listed = songs::list_songs(&mut conn, false).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, "generating");
        assert_eq!(listed[0].sections, draft.sections, "lyrics survive storage");
        assert_eq!(listed[0].origin.brief, "Ordering coffee", "how it was made is kept");
        assert_eq!(listed[0].suno_model, "V6");
        assert_eq!(listed[0].tracks.len(), 1);
        assert_eq!(listed[0].tracks[0].duration, 120.5);

        // The songwriter falls back to the tutor model until it has its own.
        db::set_setting(&mut conn, "ai_model", "tutor/model").unwrap();
        assert_eq!(songs::songwriter_model(&mut conn).unwrap(), "tutor/model");
        db::set_setting(&mut conn, "songwriter_model", "writer/model").unwrap();
        assert_eq!(songs::songwriter_model(&mut conn).unwrap(), "writer/model");
    }

    #[test]
    fn the_tutor_builds_a_course_that_can_be_practised() {
        let mut conn = scratch_db("course");

        // "Can you build me a programme for learning the colours?" — a course.
        let result = tutor::run_tool_for_test(
            &mut conn,
            "create_course",
            serde_json::json!({
                "title": "Colours",
                "title_da": "Farver",
                "summary": "The basic colours.",
                "words": [
                    { "da": "rød", "en": "red", "pos": "adj" },
                    { "da": "blå", "en": "blue", "pos": "adj" },
                    { "da": "grøn", "en": "green", "pos": "Adjective" },
                    { "da": "farve", "en": "colour", "pos": "n-common" },
                    // A duplicate is skipped, not an error.
                    { "da": "rød", "en": "red", "pos": "adj" }
                ],
                "sentences": [
                    { "da": "Min bil er rød.", "en": "My car is red." },
                    { "da": "Himlen er blå.", "en": "The sky is blue." }
                ],
                "dialogue": [
                    { "speaker": "Ola", "gender": "f", "da": "Hvad er din yndlingsfarve?", "en": "What is your favourite colour?" },
                    { "speaker": "Tomek", "gender": "m", "da": "Grøn.", "en": "Green." }
                ]
            }),
        );
        assert!(result.get("error").is_none(), "{result}");
        let created = &result["created"];
        assert_eq!(created["words"], 4);
        assert_eq!(created["skipped_duplicates"], 1);
        assert_eq!(result["_action"]["kind"], "courseCreated");
        let course_id = serde_json::from_value(created["course_id"].clone()).unwrap();

        // It is cut into drills, and they are offered first.
        let list = lessons::for_course(&mut conn, course_id).unwrap();
        let formats: Vec<&str> = list.iter().map(|b| b.lesson_type.as_str()).collect();
        assert_eq!(formats, vec!["dialogue", "words", "sentences"]);
        db::delete_setting(&mut conn, "daily_day").unwrap();
        assert!(lessons::overview(&mut conn).unwrap().due.iter().any(|l| l.course_id == Some(course_id)));
        let plan = session::build_lesson(&mut conn, list[1].id).unwrap();
        assert_eq!(plan.course.kind, "course");
        assert_eq!(plan.new_words, 4);

        // Adding words makes a new drill; the existing ones keep theirs, and a
        // word the course already teaches is not added twice.
        let added = tutor::run_tool_for_test(
            &mut conn,
            "add_to_course",
            serde_json::json!({ "course_id": course_id, "words": [
                { "da": "gul", "en": "yellow", "pos": "adj" },
                { "da": "rød", "en": "red", "pos": "adj" }
            ] }),
        );
        assert_eq!(added["updated"]["skipped_duplicates"], 1, "{added}");
        assert_eq!(lessons::load(&mut conn, list[1].id).unwrap().items, 4);
        let words_2 = lessons::for_course(&mut conn, course_id).unwrap().into_iter().last().unwrap();
        assert_eq!((words_2.title.as_str(), words_2.items), ("Words 2", 1));

        // Starter courses are ordinary courses: they can be extended too.
        let course = db::require_course_id(&mut conn, 1).unwrap();
        let before = lessons::for_course(&mut conn, course).unwrap().len();
        let grown = tutor::run_tool_for_test(
            &mut conn,
            "add_to_course",
            serde_json::json!({ "course_id": course, "words": [{ "da": "nabolag", "en": "neighbourhood", "pos": "n-neuter" }] }),
        );
        assert!(grown.get("error").is_none(), "{grown}");
        assert!(lessons::for_course(&mut conn, course).unwrap().len() >= before);

        // Any course can be deleted with its lessons.
        let deleted = tutor::run_tool_for_test(&mut conn, "delete_course", serde_json::json!({ "course_id": course_id }));
        assert_eq!(deleted["deleted"], "Colours");
        assert!(lessons::for_course(&mut conn, course_id).unwrap().is_empty());
    }

    #[test]
    fn a_lesson_is_in_one_course_and_leaves_it_only_by_being_deleted() {
        let mut conn = scratch_db("membership");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let family = db::require_course_id(&mut conn, 2).unwrap();
        let lesson = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();

        // A second course is refused, naming the rule, and nothing changes.
        let err = courses::add_lesson(&mut conn, family, lesson.id).unwrap_err().to_string();
        assert!(err.contains("one course only"), "{err}");
        assert_eq!(lessons::load(&mut conn, lesson.id).unwrap().course_ids, vec![basics]);
        // Listing it again where it is changes nothing.
        courses::add_lesson(&mut conn, basics, lesson.id).unwrap();
        // The database refuses it too, whatever writes it.
        let raw = diesel::sql_query(format!("INSERT INTO course_lessons (course_id, lesson_id, ord) VALUES ('{family}', '{}', 99)", lesson.id))
            .execute(&mut conn);
        assert!(raw.is_err(), "the unique index holds");
        // The tools to share or move a lesson are gone.
        for gone in ["add_lesson_to_course", "move_lesson"] {
            let out = tutor::run_tool_for_test(
                &mut conn,
                gone,
                serde_json::json!({ "lesson_id": lesson.id, "course_id": family, "from_course_id": basics, "to_course_id": family }),
            );
            assert!(out.get("error").is_some(), "{gone}: {out}");
        }

        // Out of its course, it goes with its words.
        let word_id: WordId =
            t::words::table.filter(t::words::lesson_id.eq(lesson.id)).select(t::words::id).first(&mut conn).unwrap();
        let gone = tutor::run_tool_for_test(
            &mut conn,
            "remove_lesson_from_course",
            serde_json::json!({ "lesson_id": lesson.id, "course_id": basics }),
        );
        assert_eq!(gone["lesson_deleted"], true);
        assert!(lessons::load(&mut conn, lesson.id).is_err());
        let left: i64 = t::words::table.find(word_id).count().get_result(&mut conn).unwrap();
        assert_eq!(left, 0, "its words went with it");

        // Reordering puts a lesson first.
        let last = lessons::for_course(&mut conn, basics).unwrap().last().unwrap().id;
        tutor::run_tool_for_test(
            &mut conn,
            "order_course_lessons",
            serde_json::json!({ "course_id": basics, "lesson_ids": [last] }),
        );
        assert_eq!(lessons::for_course(&mut conn, basics).unwrap()[0].id, last);

        // Deleting a course deletes its lessons.
        let kept = lessons::for_course(&mut conn, family).unwrap()[0].id;
        crate::courses::delete(&mut conn, family).unwrap();
        assert!(lessons::load(&mut conn, kept).is_err());
    }

    #[test]
    fn the_tutor_writes_study_lessons_and_single_format_drills() {
        let mut conn = scratch_db("tutor-lessons");
        let study = tutor::run_tool_for_test(
            &mut conn,
            "create_study_lesson",
            serde_json::json!({
                "title": "At gå — to walk",
                "summary": "The present tense of at gå.",
                "complexity": "low",
                "content": "**At gå**: jeg går, du går, han går, vi går, I går, de går.",
                "objectives": ["Use går in the present tense", "Say where you are going"]
            }),
        );
        assert_eq!(study["_action"]["kind"], "lessonCreated", "{study}");
        let study_id = serde_json::from_value(study["_action"]["lessonId"].clone()).unwrap();
        let lesson = lessons::load(&mut conn, study_id).unwrap();
        assert_eq!((lesson.lesson_type.as_str(), lesson.complexity.as_str(), lesson.period_days), ("study", "low", 5));
        // Without a course it is filed in "Other lessons".
        assert_eq!(lesson.course_title.as_deref(), Some("Other lessons"));

        // A study lesson without objectives is refused: the examiner needs them.
        let refused = tutor::run_tool_for_test(
            &mut conn,
            "create_study_lesson",
            serde_json::json!({ "title": "x", "summary": "x", "complexity": "low", "content": "x", "objectives": [] }),
        );
        assert!(refused["error"].is_string());

        // "I meant memory cards": one drill, memory format, and nothing else made.
        let drill = tutor::run_tool_for_test(
            &mut conn,
            "create_drill_lesson",
            serde_json::json!({
                "title": "Colours — memory",
                "format": "memory",
                "complexity": "low",
                "words": [
                    { "da": "rød", "en": "red", "pos": "adj" },
                    { "da": "hvid", "en": "white", "pos": "adj" },
                    { "da": "czarny", "en": "black", "pos": "adj" },
                    { "da": "grøn", "en": "green", "pos": "adj" }
                ]
            }),
        );
        let drill_id = serde_json::from_value(drill["_action"]["lessonId"].clone()).unwrap();
        let lesson = lessons::load(&mut conn, drill_id).unwrap();
        assert_eq!((lesson.lesson_type.as_str(), lesson.items), ("memory", 4));
        // Without a course, a one-off goes in Other lessons — no new course.
        assert_eq!(lesson.course_title.as_deref(), Some("Other lessons"));
        let plan = session::build_lesson(&mut conn, drill_id).unwrap();
        assert!(plan.exercises.iter().all(|e| e.kind == "memory"));

        // Starter lessons are ordinary lessons: they can be edited and deleted.
        let starter = starter_lesson(&mut conn).id;
        let edit = tutor::run_tool_for_test(&mut conn, "update_lesson", serde_json::json!({ "lesson_id": starter, "title": "Mine now" }));
        assert!(edit.get("error").is_none(), "{edit}");
        assert_eq!(lessons::load(&mut conn, starter).unwrap().title, "Mine now");
        let gone = tutor::run_tool_for_test(&mut conn, "delete_lesson", serde_json::json!({ "lesson_id": starter }));
        assert_eq!(gone["_action"]["kind"], "lessonDeleted");

        // Scheduling makes it due now.
        tutor::run_tool_for_test(&mut conn, "schedule_lesson", serde_json::json!({ "lesson_id": study_id }));
        assert!(lessons::overview(&mut conn).unwrap().due.iter().any(|b| b.id == study_id));
        let gone = tutor::run_tool_for_test(&mut conn, "delete_lesson", serde_json::json!({ "lesson_id": study_id }));
        assert_eq!(gone["_action"]["kind"], "lessonDeleted");
    }

    #[test]
    fn playlists_hold_takes_in_order_and_lose_deleted_ones() {
        let mut conn = scratch_db("playlists");
        conn.batch_execute(&fixture(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
             VALUES ($s1, 'Kaffe', 'En kaffe, tak', 'A coffee, please', 'pop', '[]', '', 'V6', 'ready', 0);
INSERT INTO files (id, kind, name, mime, size, sha256, bytes, created_at) VALUES ($f100, 'song_audio', 'f100.mp3', 'audio/mpeg', 1, '', x'00', 0), ($f101, 'song_audio', 'f101.mp3', 'audio/mpeg', 1, '', x'00', 0);
             INSERT INTO song_tracks (id, song_id, ord, remote_id, audio_file_id) VALUES
                 ($t10, $s1, 0, 'a', $f100), ($t11, $s1, 1, 'b', $f101);",
        ))
        .unwrap();

        let (t10, t11) = (track(10), track(11));
        let list = playlists::create(&mut conn, "  Morning   run ", &[t10, t11, t10]).unwrap();
        assert_eq!(list.name, "Morning run");
        let order = |p: &playlists::Playlist| p.entries.iter().map(|e| e.track_id).collect::<Vec<_>>();
        assert_eq!(order(&list), vec![t10, t11, t10], "the same take may appear twice");

        let moved = playlists::move_entry(&mut conn, list.id, list.entries[2].id, -2).unwrap();
        assert_eq!(order(&moved), vec![t10, t10, t11]);
        // Moving past either end stays in bounds.
        let moved = playlists::move_entry(&mut conn, list.id, moved.entries[0].id, -5).unwrap();
        assert_eq!(order(&moved), vec![t10, t10, t11]);

        // Deleting a take takes it out of every playlist.
        diesel::delete(t::song_tracks::table.find(t10)).execute(&mut conn).unwrap();
        assert_eq!(order(&playlists::list(&mut conn).unwrap()[0]), vec![t11]);
        assert!(playlists::create(&mut conn, "   ", &[]).is_err());
    }

    /// Every query an agent prompt, a tutor tool or a screen runs, on a database
    /// with a bit of everything in it — so a column renamed in the schema
    /// fails here, not on the phone.
    #[test]
    fn writing_lessons_are_filed_and_prompt_a_danish_only_partner() {
        let mut conn = scratch_db("writing");
        let made = tutor::run_tool_for_test(
            &mut conn,
            "create_writing_lesson",
            serde_json::json!({
                "title": "Where do you live?",
                "summary": "Tell a new neighbour where you live.",
                "complexity": "low",
                "scenario": "You've just moved in. Your neighbour Pani Ewa stops you on the stairs.",
                "goals": ["Say which city you live in", "Say who you live with"]
            }),
        );
        let id: LessonId = serde_json::from_value(made["created"]["lesson_id"].clone()).unwrap_or_else(|e| panic!("{e}: {made}"));
        let detail = lessons::detail(&mut conn, id).unwrap();
        assert_eq!((detail.lesson.lesson_type.as_str(), detail.lesson.engine), ("writing", "conversation"));
        assert_eq!(detail.objectives.len(), 2);
        let writing = courses::other_course(&mut conn).unwrap();
        assert_eq!(detail.lesson.course_ids, vec![writing], "filed in Writing by default");

        let prompt = examiner::writing_prompt(&mut conn, &detail).unwrap();
        assert!(prompt.contains("Say who you live with") && prompt.contains("Pani Ewa"));
        assert!(prompt.contains("understand only Danish"));
        assert!(prompt.contains("A1"), "a low lesson gets simple Danish");

        let missing = tutor::run_tool_for_test(
            &mut conn,
            "create_writing_lesson",
            serde_json::json!({ "title": "x", "summary": "", "complexity": "low", "scenario": "y", "goals": [] }),
        );
        assert!(missing.get("error").is_some(), "goals are required");
    }

    #[test]
    fn every_read_path_runs_on_a_full_database() {
        let mut conn = scratch_db("read-paths");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        crate::memories::add(&mut conn, "Wants to order food in København", "goal", "tutor").unwrap();
        conn.batch_execute(&format!(
            r#"INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at, course_id)
               VALUES ('{s1}', 'Kaffe', 'En kaffe, tak', 'A coffee, please', 'pop',
                       '[{{"kind":"chorus","lines":[{{"en":"A coffee, please","da":"En kaffe, tak"}}]}}]',
                       '', 'V6', 'ready', 0, '{basics}'),
                      ('{s2}', 'Dom', 'Dom', 'Home', 'pop', '[]', '', 'V6', 'ready', 0, NULL);"#,
            s1 = song(1),
            s2 = song(2),
        ))
        .unwrap();
        let filed = lessons::create_listening(&mut conn, song(1), None, None, None, "tutor").unwrap();
        let loose = lessons::create_listening(&mut conn, song(2), None, None, None, "tutor").unwrap();
        crate::playlists::create(&mut conn, "Morgen", &[]).unwrap();
        let starter = starter_lesson(&mut conn);
        let drill = lessons::for_course(&mut conn, basics).unwrap().into_iter().find(|b| b.lesson_type == "words").unwrap();

        let prompt = tutor::system_prompt(&mut conn).unwrap();
        let basics_content: content::Conversation = serde_json::from_str(content::COURSES[0].conversation).unwrap();
        assert!(prompt.contains(&basics_content.title_da) && prompt.contains("Wants to order food"));
        assert!(prompt.contains("in \"Other lessons\" with no course"), "lessons without a course are pointed out");
        for lesson in [&starter, &filed, &loose] {
            let detail = lessons::detail(&mut conn, lesson.id).unwrap();
            assert!(!examiner::system_prompt(&mut conn, &detail).unwrap().is_empty());
        }

        for (tool, args) in [
            ("list_courses", serde_json::json!({})),
            ("get_course", serde_json::json!({ "course_id": basics })),
            ("list_lessons", serde_json::json!({ "which": "due" })),
            ("list_lessons", serde_json::json!({ "which": "course", "course_id": basics })),
            ("list_lessons", serde_json::json!({ "which": "study" })),
            ("get_lesson", serde_json::json!({ "lesson_id": drill.id })),
            ("get_progress", serde_json::json!({})),
            ("search_vocabulary", serde_json::json!({ "query": "hej" })),
            ("list_songs", serde_json::json!({})),
            ("get_song", serde_json::json!({ "song_id": song(1) })),
            ("list_playlists", serde_json::json!({})),
        ] {
            let result = tutor::run_tool_for_test(&mut conn, tool, args);
            assert!(result.get("error").is_none(), "{tool}: {result}");
        }

        assert!(!commands::browse_words_in(&mut conn, Some(basics), String::new(), 50).unwrap().is_empty());
        assert!(!commands::browse_words_in(&mut conn, None, "hej".into(), 50).unwrap().is_empty());
        assert!(commands::browse_catalogue_in(&mut conn, None, String::new(), 10).unwrap().total_words > 0);
        assert!(!commands::browse_sentences_in(&mut conn, basics).unwrap().is_empty());
        assert!(!speech::course_audio_lines_in(&mut conn, basics).unwrap().is_empty());
        speech::audio_cache_stats_in(&mut conn).unwrap();
        stats::collect(&mut conn).unwrap();
        assert!(stats::course_progress(&mut conn).unwrap().iter().any(|p| p.total_lessons > 0));
        lessons::overview(&mut conn).unwrap();
        songs::gather_context(&mut conn).unwrap();
        session::build_lesson(&mut conn, drill.id).unwrap();
    }

    /// Writes what the core sends the screens, for each shape, to
    /// target/shapes: `bun run check:shapes` parses every file with its zod
    /// schema (src/lib/types.ts), so the schemas cannot drift from the structs.
    #[test]
    fn shapes_for_the_screens() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shapes");
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, value: &dyn erased::Shape| {
            std::fs::write(dir.join(format!("{name}.json")), value.json()).unwrap();
        };
        mod erased {
            pub trait Shape {
                fn json(&self) -> String;
            }
            impl<T: serde::Serialize> Shape for T {
                fn json(&self) -> String {
                    serde_json::to_string_pretty(self).unwrap()
                }
            }
        }

        let mut conn = scratch_db("shapes");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        crate::memories::add(&mut conn, "Wants to order food in København", "goal", "tutor").unwrap();
        conn.batch_execute(&format!(
            r#"INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at, course_id)
               VALUES ('{s1}', 'Kaffe', 'En kaffe, tak', 'A coffee, please', 'pop',
                       '[{{"kind":"chorus","lines":[{{"en":"A coffee, please","da":"En kaffe, tak"}}]}}]',
                       '', 'V6', 'ready', 0, '{basics}');"#,
            s1 = song(1),
        ))
        .unwrap();
        lessons::create_listening(&mut conn, song(1), None, None, None, "tutor").unwrap();
        crate::playlists::create(&mut conn, "Morgen", &[]).unwrap();
        let all = lessons::for_course(&mut conn, basics).unwrap();
        let drill = all.iter().find(|l| l.lesson_type == "words").unwrap().clone();
        let finished = practice::lesson_finish_in(&mut conn, drill.id, 9, 10).unwrap();
        let today = db::today(&mut conn);

        write("Settings", &commands::read_settings(&mut conn).unwrap());
        let kept_offline = db::require_course_id(&mut conn, 1).unwrap();
        write("OfflineStatus", &offline::status(&mut conn, kept_offline).unwrap());
        write("LessonOverview", &lessons::overview(&mut conn).unwrap());
        write("Lesson.list", &all);
        write("LessonDetail", &lessons::detail(&mut conn, drill.id).unwrap());
        write("LessonFinished", &finished);
        // The mastery ledger, with a check-in's row and the mastered stamp too.
        lessons::record(&mut conn, drill.id, "fail").unwrap();
        write("MasteryLog", &crate::masterylog::for_lesson(&mut conn, drill.id).unwrap());
        let empty = all.iter().find(|l| l.id != drill.id).unwrap().id;
        write("MasteryLog.empty", &crate::masterylog::for_lesson(&mut conn, empty).unwrap());
        write("LessonFinished.ungraded", &practice::lesson_finish_in(&mut conn, empty, 0, 0).unwrap());
        let word: WordId = t::words::table.filter(t::words::lesson_id.eq(drill.id)).select(t::words::id).first(&mut conn).unwrap();
        let almost = practice::submit_answer_in(
            &mut conn,
            practice::SubmitAnswer {
                kind: "type".into(),
                item_kind: "word".into(),
                item_id: word.to_string(),
                expected: "spise".into(),
                given: "spiser".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(almost.almost);
        write("AnswerResult.almost", &almost);
        write("CourseProgress.list", &stats::course_progress(&mut conn).unwrap());
        write("CoursePictureStatus", &pictures::PictureStatus { course_id: basics, drawing: false, error: Some("No API key".into()) });
        let collected = stats::collect(&mut conn).unwrap();
        write("Stats", &collected);
        // The Progress screen's achievements: some earned, some part-way.
        assert!(collected.achievements.iter().any(|a| a.unlocked) && collected.achievements.iter().any(|a| !a.unlocked));
        write("Achievement.list", &collected.achievements);
        write("Word.list", &commands::browse_words_in(&mut conn, Some(basics), String::new(), 50).unwrap());
        write("Sentence.list", &commands::browse_sentences_in(&mut conn, basics).unwrap());
        write("CataloguePage", &commands::browse_catalogue_in(&mut conn, None, String::new(), 10).unwrap());
        write("SessionPlan", &session::build_lesson(&mut conn, drill.id).unwrap());
        // A sentence with endings: its word-order card asks for them.
        let tiles = tutor::run_tool_for_test(
            &mut conn,
            "create_drill_lesson",
            serde_json::json!({
                "title": "Tiles", "format": "sentences", "complexity": "low", "course_id": basics,
                "sentences": [{ "da": "Jeg drikker kaffen.", "en": "I drink the coffee.",
                                "endings": [{ "form": "kaffen", "base": "kaffe", "wrong": ["kaffer", "kaffens"] }],
                                "grammar": ["definite-singular", "present", "v2"] }]
            }),
        );
        let tiles = serde_json::from_value(tiles["_action"]["lessonId"].clone()).unwrap();
        let tile_plan = session::build_lesson(&mut conn, tiles).unwrap();
        assert!(tile_plan.exercises.iter().any(|e| !e.endings.is_empty()));
        // And the ear's: a dialogue line to write down, a sound pair; and
        // the voice's, with a key to hear with: a line said, a sentence said.
        let mut exercises = tile_plan.exercises.clone();
        let dialogue = all.iter().find(|l| l.lesson_type == "dialogue").unwrap();
        db::set_setting(&mut conn, "openrouter_api_key", "sk-shapes").unwrap();
        exercises.extend(session::build_lesson(&mut conn, dialogue.id).unwrap().exercises);
        let sentence = session::lesson_sentences(&mut conn, tiles).unwrap().remove(0).0;
        exercises.push(session::say_sentence(&sentence));
        let pairs = tutor::run_tool_for_test(
            &mut conn,
            "create_drill_lesson",
            serde_json::json!({ "title": "Pairs", "format": "words", "complexity": "low", "course_id": basics,
                "words": [{ "da": "et træ", "en": "tree", "pos": "n-neuter", "near": { "da": "tre", "en": "three" }, "grammar": ["neuter"] }] }),
        );
        let pairs = serde_json::from_value(pairs["_action"]["lessonId"].clone()).unwrap();
        exercises.extend(session::build_lesson(&mut conn, pairs).unwrap().exercises);
        // A scene, drawn and found: explore, find, name and say where.
        let things = [("et bord", "table", "n-neuter"), ("en kat", "cat", "n-common"), ("en stol", "chair", "n-common"), ("en lampe", "lamp", "n-common"), ("et vindue", "window", "n-neuter"), ("et krus", "mug", "n-neuter")]
            .map(|(da, en, pos)| serde_json::json!({ "da": da, "en": en, "pos": pos }));
        let scene = tutor::run_tool_for_test(
            &mut conn,
            "create_scene_lesson",
            serde_json::json!({ "title": "I køkkenet", "scene": "A kitchen", "course_id": basics, "complexity": "medium",
                "things": things,
                "statements": [{ "da": "Katten er under bordet.", "en": "The cat is under the table.", "subject": "en kat", "preposition": "under",
                                 "object": "et bord", "phrase": "under bordet", "wrong": ["på bordet", "ved siden af bordet"] }] }),
        );
        let scene: LessonId = serde_json::from_value(scene["_action"]["lessonId"].clone()).unwrap();
        crate::scenes::ready_for_test(&mut conn, scene, &[
            [0.30, 0.45, 0.70, 0.75], [0.42, 0.62, 0.55, 0.78], [0.75, 0.40, 0.90, 0.80],
            [0.05, 0.05, 0.15, 0.30], [0.05, 0.40, 0.25, 0.60], [0.50, 0.38, 0.56, 0.46],
        ]).unwrap();
        write("SceneInfo", &crate::scenes::info(&mut conn, scene).unwrap().unwrap());
        exercises.extend(session::build_lesson(&mut conn, scene).unwrap().exercises);
        // A story, its panels drawn but one: read it, answer, its words.
        let made = tutor::run_tool_for_test(&mut conn, "create_story_lesson", story_args(basics, "low"));
        let story: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        crate::stories::ready_for_test(&mut conn, story).unwrap();
        let last: PanelId = t::story_panels::table.filter(t::story_panels::lesson_id.eq(story)).order(t::story_panels::ord.desc())
            .select(t::story_panels::id).first(&mut conn).unwrap();
        diesel::update(t::story_panels::table.find(last)).set((t::story_panels::status.eq("failed"), t::story_panels::error.eq("The model drew nothing."))).execute(&mut conn).unwrap();
        write("StoryInfo", &crate::stories::info(&mut conn, story).unwrap().unwrap());
        write("LessonDetail.list", &[lessons::detail(&mut conn, scene).unwrap(), lessons::detail(&mut conn, story).unwrap()]);
        exercises.extend(session::build_lesson(&mut conn, story).unwrap().exercises);
        for kind in ["dictation", "soundPair", "sayIt", "dialogueSay", "wordOrder", "sceneExplore", "sceneFind", "sceneName", "sceneWhere", "storyPanel", "storyQuestion", "storyEnd"] {
            assert!(exercises.iter().any(|e| e.kind == kind), "a {kind} card");
        }
        write("Exercise.list", &exercises);
        // Every card that grades writes its answer to the learning log with
        // its grade (one transaction), mistake and all; a card only read
        // writes an event instead.
        let attempts_before: i64 = t::attempts::table.count().get_result(&mut conn).unwrap();
        let events_before: i64 = t::learning_events::table.count().get_result(&mut conn).unwrap();
        let (mut graded, mut read) = (0i64, 0i64);
        let mut kinds_logged = std::collections::HashSet::new();
        for e in &exercises {
            let reading = practice::READING_KINDS.contains(&e.kind.as_str());
            let given = e.options.iter().find(|o| **o != e.answer).cloned().unwrap_or_else(|| "ved ikke".into());
            practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
                kind: e.kind.clone(), item_kind: e.item_kind.clone(), item_id: e.item_id.clone(), expected: e.answer.clone(), given,
                options: e.options.clone(), prompt: e.prompt.clone(), da: e.da.clone(), run: "shapes".into(),
                signals: Some(crate::learnlog::Signals { ms_first: Some(900), ms_answer: Some(4200), plays: 2, slow_plays: 1, tries: 1, ..Default::default() }),
                ..Default::default()
            }).unwrap();
            if reading {
                read += 1;
            } else {
                graded += 1;
                let (exercise, correct, mistake, plays): (String, i64, String, i64) = t::attempts::table
                    .order(t::attempts::id.desc())
                    .select((t::attempts::exercise, t::attempts::correct, t::attempts::mistake, t::attempts::plays))
                    .first(&mut conn).unwrap();
                assert_eq!((exercise.as_str(), plays), (e.kind.as_str(), 2), "{} logs its answer and signals", e.kind);
                if correct == 0 && e.kind != "memory" {
                    assert!(!mistake.is_empty(), "{}: a miss has a class", e.kind);
                }
                kinds_logged.insert(e.kind.clone());
            }
        }
        assert_eq!(t::attempts::table.count().get_result::<i64>(&mut conn).unwrap() - attempts_before, graded);
        assert!(t::learning_events::table.count().get_result::<i64>(&mut conn).unwrap() - events_before >= read);
        for kind in ["dictation", "soundPair", "sayIt", "dialogueSay", "wordOrder", "sceneFind", "sceneName", "sceneWhere", "storyQuestion"] {
            assert!(kinds_logged.contains(kind), "{kind} logged");
        }
        let dictated = exercises.iter().find(|e| e.kind == "dictation").unwrap();
        write(
            "AnswerResult",
            &practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
                kind: "dictation".into(), item_kind: dictated.item_kind.clone(), item_id: dictated.item_id.clone(),
                expected: dictated.answer.clone(), given: "noget andet".into(), ..Default::default()
            }).unwrap(),
        );
        write("DictationChecked", &crate::dictation::check("Jeg drikker kaffen.", "jeg drikker kaffe"));
        let said = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "sayIt".into(), item_kind: "sentence".into(), item_id: sentence.id.to_string(),
            expected: "Der står et træ.".into(), given: "der står et tre".into(), ..Default::default()
        }).unwrap();
        assert!(!said.tips.is_empty());
        write("AnswerResult.list", &[said]);
        let spoken = crate::speaking::check("Der står et træ i haven.", "Der står et tre i haven");
        write("Spoken", &spoken);
        write("Heard.list", &[
            crate::speaking::Heard { text: "Der står et tre i haven".into(), spoken: Some(spoken.clone()) },
            crate::speaking::Heard { text: "How do I say this?".into(), spoken: None },
        ]);
        write("GlossWord.list", &crate::singalong::glossary(&mut conn, song(1)).unwrap());
        // Quick games, once a few words are met.
        let met: Vec<WordId> = crate::schema::words::table.select(crate::schema::words::id).limit(8).load(&mut conn).unwrap();
        for id in &met {
            let answer = practice::SubmitAnswer { kind: "recognise".into(), item_kind: "word".into(), item_id: id.to_string(), ..Default::default() };
            practice::submit_answer_in(&mut conn, answer).unwrap();
        }
        write("QuickGame", &crate::games::deal(&mut conn, "flip", &mut rand::rng()).unwrap());
        let sentences: Vec<SentenceId> = crate::schema::sentences::table.select(crate::schema::sentences::id).limit(5).load(&mut conn).unwrap();
        for id in &sentences {
            let answer = practice::SubmitAnswer { kind: "wordOrder".into(), item_kind: "sentence".into(), item_id: id.to_string(), ..Default::default() };
            practice::submit_answer_in(&mut conn, answer).unwrap();
        }
        write(
            "QuickGame.list",
            &["dictation", "sounds", "say"].map(|game| crate::games::deal(&mut conn, game, &mut rand::rng()).unwrap()),
        );
        write("GameFinished", &crate::games::finish(&mut conn, "match", 8, 42_000, 1).unwrap());
        write("GameAnswered", &crate::games::answer(&mut conn, crate::games::GameAnswer {
            game: "dictation".into(), item_id: Some(sentences[0].to_string()), expected: "Jeg drikker kaffen.".into(), given: "jeg drikker kaffe".into(), ..Default::default()
        }).unwrap());
        // The learning log's pages, on what this test answered.
        let all_time = crate::analytics::Filter::default();
        write("Overview", &crate::analytics::overview(&mut conn, &all_time).unwrap());
        for kind in crate::analytics::HARD_KINDS {
            let hard = crate::analytics::hard_items(&mut conn, kind, &all_time, 10).unwrap();
            assert!(!hard.is_empty(), "something {kind} is hard");
            if *kind == "word" {
                write("HardItem.list", &hard);
            }
        }
        let met_words = crate::analytics::met_words(&mut conn).unwrap();
        write("MetWord.list", &met_words);
        write("MetWordDetail", &crate::analytics::met_word(&mut conn, met_words[0].id).unwrap());
        write("ItemHistory", &crate::analytics::item_history(&mut conn, &sentence.id.to_string()).unwrap());
        let structures = crate::analytics::structures(&mut conn, &all_time).unwrap();
        assert!(structures.tags.iter().any(|t| t.tag.key == "definite-singular" && t.attempts > 0), "the tagged sentence's answers count");
        write("Structures", &structures);
        write("StructureDetail", &crate::analytics::structure(&mut conn, "definite-singular", &all_time).unwrap());
        let sounds = crate::analytics::sounds(&mut conn, &all_time).unwrap();
        assert!(!sounds.is_empty());
        write("SoundRow.list", &sounds);
        let page = crate::analytics::mistakes_of(&mut conn, "unknown_word", &all_time, 20).unwrap();
        write("MistakePage", &page);
        write("Comparison", &crate::analytics::compare(&mut conn, &crate::analytics::Filter { to_day: Some(today - 1), ..all_time }, &crate::analytics::Filter { from_day: Some(today), ..all_time }).unwrap());
        write("Activity", &crate::analytics::activity(&mut conn, &all_time).unwrap());
        // Schema 15: the raw log, contexts and topics.
        let raw = crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter::default(), None, 200).unwrap();
        assert!(raw.rows.iter().any(|r| r.kind == "answer") && raw.rows.iter().any(|r| r.kind == "event"));
        write("RawPage", &raw);
        write("ContextStats", &crate::contexts::stats(&mut conn, &crate::contexts::Subject::default(), "exercise", &all_time).unwrap());
        for id in &met {
            crate::topics::set_word(&mut conn, *id, &crate::topics::validate(&["food.drinks".into()]).unwrap()).unwrap();
        }
        let topics_page = crate::contexts::topics_page(&mut conn, &all_time).unwrap();
        assert!(!topics_page.domains.is_empty());
        write("TopicsPage", &topics_page);
        write("TopicDetail", &crate::contexts::topic_detail(&mut conn, "food", &all_time).unwrap());
        write("QuickGames", &crate::games::overview(&mut conn).unwrap());
        write("LessonTypeInfo.list", &crate::lesson_types::lesson_types());
        write("Song.list", &songs::list_songs(&mut conn, false).unwrap());
        write("Playlist.list", &playlists::list(&mut conn).unwrap());
        write("MemoryOverview", &crate::memories::commands::overview(&mut conn).unwrap());
        write("XpDay", &crate::xplog::day(&mut conn, today).unwrap());
        write("XpHistoryDay.list", &crate::xplog::history(&mut conn, 30).unwrap());
        write("CostSummary", &crate::costs::summary(&mut conn, 30).unwrap());
        write("AudioOverview", &speech::audio_overview_in(&mut conn).unwrap());
        write("AudioItem.list", &speech::course_audio_in(&mut conn, basics).unwrap());
        write("AudioCacheStats", &speech::audio_cache_stats_in(&mut conn).unwrap());
        write("Conversation.list", &tutor::list_conversations(&mut conn).unwrap());
        write("DiagnosticEntry.list", &crate::diagnostics::entries());

        // Sync (the Devices screen): this device in a vault of its own.
        use crate::sync::{service, session as sync_session};
        sync_session::ensure_vault(&mut conn).unwrap();
        let activity = service::Activity { syncing: false, last_error: Some("no answer".into()), last_round_at: Some(crate::sync::now_iso()) };
        write("SyncActivity", &activity);
        write("SyncStatus", &service::status(&mut conn, true, activity).unwrap());
        write("SyncHeld.list", &service::held(&mut conn).unwrap());
        let received = sync_session::Received { applied: 3, conflicts: 1, unique_merges: 0, removed: 0 };
        write("SyncReceived", &received);
        write(
            "JoinResult",
            &service::JoinResult {
                outcome: sync_session::Outcome {
                    peer_id: crate::ids::DeviceId::new().to_string(),
                    peer_name: "Phone".into(),
                    received,
                    complete: false,
                    differences: vec!["memories".into()],
                    digest_skipped: false,
                },
                backup: Some("/data/backups/before-join-learn-danish-1.sqlite".into()),
            },
        );
        write(
            "SyncOffer",
            &service::SyncOffer { code: "dansk-pair:1:abc".into(), svg: "<svg/>".into(), expires_at: crate::sync::now_iso() },
        );
    }

    #[test]
    fn the_tutor_manages_playlists_by_song() {
        let mut conn = scratch_db("tutor-playlists");
        conn.batch_execute(&fixture(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
             VALUES ($s1, 'Kaffe', 'En kaffe, tak', 'A coffee, please', 'pop', '[]', '', 'V6', 'ready', 0),
                    ($s2, 'Farver', 'Himmel', 'Sky', 'pop', '[]', '', 'V6', 'ready', 0),
                    ($s3, 'Na dworcu', 'x', 'x', 'pop', '[]', '', 'V6', 'generating', 0);
INSERT INTO files (id, kind, name, mime, size, sha256, bytes, created_at) VALUES ($f100, 'song_audio', 'f100.mp3', 'audio/mpeg', 1, '', x'00', 0), ($f101, 'song_audio', 'f101.mp3', 'audio/mpeg', 1, '', x'00', 0), ($f102, 'song_audio', 'f102.mp3', 'audio/mpeg', 1, '', x'00', 0);
             INSERT INTO song_tracks (id, song_id, ord, remote_id, audio_file_id, play_count) VALUES
                 ($t10, $s1, 0, 'a', $f100, 1), ($t11, $s1, 1, 'b', $f101, 5),
                 ($t20, $s2, 0, 'c', $f102, 0);",
        ))
        .unwrap();

        let made = tutor::run_tool_for_test(
            &mut conn,
            "create_playlist",
            serde_json::json!({ "name": "Morning", "song_ids": [song(1)] }),
        );
        assert_eq!(made["_action"]["kind"], "playlistChanged", "{made}");
        let id: PlaylistId = serde_json::from_value(made["playlist_id"].clone()).unwrap();
        // The most-played take goes in.
        assert_eq!(made["songs"][0]["take"], 2);
        assert_eq!(playlists::list(&mut conn).unwrap()[0].entries[0].track_id, track(11));

        // "Add Farver to my morning playlist" — and Kaffe again is skipped.
        let added = tutor::run_tool_for_test(
            &mut conn,
            "add_to_playlist",
            serde_json::json!({ "playlist_id": id, "song_ids": [song(2), song(1)] }),
        );
        assert_eq!(added["songs"].as_array().unwrap().len(), 2, "{added}");
        assert_eq!(added["already_in_it"], serde_json::json!(["Kaffe"]));

        // A song still being made cannot go in, and nothing changes.
        let refused = tutor::run_tool_for_test(
            &mut conn,
            "add_to_playlist",
            serde_json::json!({ "playlist_id": id, "song_ids": [song(3)] }),
        );
        assert!(refused["error"].as_str().unwrap().contains("no audio yet"));

        let listed = tutor::run_tool_for_test(&mut conn, "list_playlists", serde_json::json!({}));
        assert_eq!(listed[0]["songs"][1]["title"], "Farver");

        tutor::run_tool_for_test(&mut conn, "remove_from_playlist", serde_json::json!({ "playlist_id": id, "song_ids": [song(1)] }));
        tutor::run_tool_for_test(&mut conn, "rename_playlist", serde_json::json!({ "playlist_id": id, "name": "Morgen" }));
        let list = playlists::list(&mut conn).unwrap();
        assert_eq!((list[0].name.as_str(), list[0].entries.len()), ("Morgen", 1));

        let gone = tutor::run_tool_for_test(&mut conn, "delete_playlist", serde_json::json!({ "playlist_id": id }));
        assert_eq!(gone["deleted"], "Morgen");
        assert!(playlists::list(&mut conn).unwrap().is_empty());
        assert_eq!(songs::list_songs(&mut conn, false).unwrap().len(), 3, "the songs stay");
    }

    #[test]
    fn tutor_conversations_keep_their_own_history() {
        // A message from before conversations existed is adopted, not lost.
        // Only an older database can hold one (version 7 is made with
        // conversations), so it is made with the version-5 schema.
        let path = std::env::temp_dir()
            .join("learn-danish-tests")
            .join(format!("conversations-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        {
            let mut old = Conn::establish(&path.to_string_lossy()).unwrap();
            old.batch_execute(include_str!("../test-fixtures/schema-v5.sql")).unwrap();
            old.batch_execute(
                "INSERT INTO tutor_messages (role, content, created_at) VALUES ('user', 'old question', 5);
                 PRAGMA user_version = 5;",
            )
            .unwrap();
        }
        let mut conn = scratch_db_keep("conversations");
        let listed = tutor::list_conversations(&mut conn).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Earlier conversation");
        assert_eq!(tutor::load_history(&mut conn, listed[0].id).unwrap().len(), 1);

        let id = tutor::create_conversation(&mut conn, "How do numbers work?").unwrap();
        assert_eq!(tutor::load_history(&mut conn, id).unwrap().len(), 0, "a new one starts empty");
        assert_eq!(tutor::list_conversations(&mut conn).unwrap().len(), 2);
    }

    #[test]
    fn a_song_gets_a_listening_lesson_that_goes_with_it() {
        let mut conn = scratch_db("listening");
        conn.batch_execute(&fixture(
            r#"INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
               VALUES ($s1, 'Kaffe', 'En kaffe, tak', 'A coffee, please', 'pop',
                       '[{"kind":"chorus","lines":[{"en":"A coffee, please","da":"En kaffe, tak"}]}]',
                       '', 'V6', 'ready', 0);"#,
        ))
        .unwrap();
        let made = tutor::run_tool_for_test(&mut conn, "create_listening_lesson", serde_json::json!({ "song_id": song(1), "complexity": "high" }));
        let id: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).expect("made");
        let lesson = lessons::load(&mut conn, id).unwrap();
        assert_eq!((lesson.lesson_type.as_str(), lesson.song_id), ("listening", Some(song(1))));
        assert!(lessons::detail(&mut conn, id).unwrap().objectives.len() >= 6, "high asks about grammar too");
        // One per song: asking again returns the same lesson.
        assert_eq!(lessons::create_listening(&mut conn, song(1), None, None, None, "tutor").unwrap().id, id);
        // A song without a course files it in "Other lessons"; it goes when the song goes.
        assert_eq!(lesson.course_title.as_deref(), Some("Other lessons"));

        // A song written from a course's words files it in that course.
        let food = db::require_course_id(&mut conn, 3).unwrap();
        conn.batch_execute(&format!(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at, course_id)
             VALUES ('{s2}', 'Obiad', 'Smacznego', 'Enjoy', 'pop', '[]', '', 'V6', 'ready', 0, '{food}');",
            s2 = song(2),
        ))
        .unwrap();
        let made = tutor::run_tool_for_test(&mut conn, "create_listening_lesson", serde_json::json!({ "song_id": song(2) }));
        let filed = lessons::load(&mut conn, serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap()).unwrap();
        assert_eq!(filed.course_ids, vec![food]);
        // Or in the course asked for.
        conn.batch_execute(&fixture(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
             VALUES ($s3, 'Dom', 'Dom', 'Home', 'pop', '[]', '', 'V6', 'ready', 0);",
        ))
        .unwrap();
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let made = tutor::run_tool_for_test(
            &mut conn,
            "create_listening_lesson",
            serde_json::json!({ "song_id": song(3), "course_id": basics }),
        );
        assert_eq!(lessons::load(&mut conn, serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap()).unwrap().course_ids, vec![basics]);

        // The tutor reads a song's lyrics and files a course-less one itself.
        conn.batch_execute(&fixture(
            r#"INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
               VALUES ($s4, 'Tog', 'Hvor er stationen?', 'Where is the station?', 'pop',
                       '[{"kind":"chorus","lines":[{"en":"Where is the station?","da":"Hvor er stationen?"}]}]',
                       '', 'V6', 'ready', 0);"#,
        ))
        .unwrap();
        let lesson = lessons::create_listening(&mut conn, song(4), None, None, None, "you").unwrap();
        assert_eq!(lesson.course_title.as_deref(), Some("Other lessons"));
        let got = tutor::run_tool_for_test(&mut conn, "get_song", serde_json::json!({ "song_id": song(4) }));
        assert_eq!(got["lyrics"][0]["lines"][0]["da"], "Hvor er stationen?");
        assert_eq!(got["filed"]["listening_lesson_in"][0]["title"], "Other lessons");
        assert!(got["filed"]["written_from_course"].is_null());
        let travel = db::require_course_id(&mut conn, 5).unwrap();
        let listening = lesson.course_id.unwrap();
        // A lesson is never moved: the one in Other lessons goes, and a new
        // one is made in the course.
        tutor::run_tool_for_test(
            &mut conn,
            "remove_lesson_from_course",
            serde_json::json!({ "lesson_id": lesson.id, "course_id": listening }),
        );
        tutor::run_tool_for_test(&mut conn, "create_listening_lesson", serde_json::json!({ "song_id": song(4), "course_id": travel }));
        let got = tutor::run_tool_for_test(&mut conn, "get_song", serde_json::json!({ "song_id": song(4) }));
        assert_eq!(got["filed"]["written_from_course"]["course_id"], serde_json::json!(travel), "the song now knows its course");
        songs::delete_song(&mut conn, song(1)).unwrap();
        assert!(lessons::load(&mut conn, id).is_err());
    }

    #[test]
    fn the_tutor_lists_and_deletes_songs() {
        let mut conn = scratch_db("tutor-songs");
        conn.batch_execute(&fixture(
            "INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at)
             VALUES ($s1, 'Kaffe', 'En kaffe, tak', 'A coffee, please', 'pop', '[]', '', 'V6', 'ready', 0),
                    ($s2, 'Farver', 'Himlen er blå', 'The sky is blue', 'pop', '[]', '', 'V6', 'failed', 0);
INSERT INTO files (id, kind, name, mime, size, sha256, bytes, created_at) VALUES ($f100, 'song_audio', 'f100.mp3', 'audio/mpeg', 1, '', x'00', 0);
             INSERT INTO song_tracks (id, song_id, ord, remote_id, audio_file_id, play_count)
                 VALUES ($t10, $s1, 0, 'a', $f100, 3);",
        ))
        .unwrap();
        let listed = tutor::run_tool_for_test(&mut conn, "list_songs", serde_json::json!({}));
        assert_eq!(listed.as_array().unwrap().len(), 2);
        let kawa = listed.as_array().unwrap().iter().find(|s| s["title"] == "Kaffe").unwrap();
        assert_eq!(kawa["plays"], 3);

        let deleted = tutor::run_tool_for_test(&mut conn, "delete_songs", serde_json::json!({ "song_ids": [song(2), song(99)] }));
        assert_eq!(deleted["_action"]["kind"], "songsDeleted");
        assert_eq!(deleted["deleted"], serde_json::json!(["Farver"]));
        assert_eq!(songs::list_songs(&mut conn, false).unwrap().len(), 1);
    }

    #[test]
    fn the_tutor_reports_progress_and_finds_vocabulary() {
        let mut conn = scratch_db("tutor-read");
        let progress = tutor::run_tool_for_test(&mut conn, "get_progress", serde_json::json!({}));
        assert_eq!(progress["words_met"], 0);
        let found = tutor::run_tool_for_test(&mut conn, "search_vocabulary", serde_json::json!({ "query": "godmorgen" }));
        assert!(!found["words"].as_array().unwrap().is_empty());
        let courses = tutor::run_tool_for_test(&mut conn, "list_courses", serde_json::json!({}));
        assert_eq!(courses.as_array().unwrap().len(), 11);
        let unknown = tutor::run_tool_for_test(&mut conn, "fly_to_the_moon", serde_json::json!({}));
        assert!(unknown["error"].is_string());
    }

    #[test]
    fn a_version_5_database_renames_blocks_to_lessons_and_topics_to_courses() {
        let path = std::env::temp_dir()
            .join("learn-danish-tests")
            .join(format!("v5-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // The demo course as a fresh install makes it: the old database below
        // holds the same course with integer ids, as an older install would.
        let fresh_path = path.with_file_name(format!("v5-fresh-demo-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&fresh_path);
        let mut fresh = db::open(&fresh_path).unwrap();
        let demo_course: CourseId =
            t::courses::table.filter(t::courses::slug.eq(crate::demo_course::SLUG)).select(t::courses::id).first(&mut fresh).unwrap();
        let demo_lessons: Vec<(LessonId, String, String)> = t::course_lessons::table
            .inner_join(t::lessons::table)
            .filter(t::course_lessons::course_id.eq(demo_course))
            .order(t::course_lessons::ord)
            .select((t::lessons::id, t::lessons::lesson_type, t::lessons::title))
            .load(&mut fresh)
            .unwrap();
        assert!(!demo_lessons.is_empty());
        let demo_ids = |conn: &mut Conn| -> (Vec<LessonId>, Vec<WordId>, Vec<SentenceId>, Vec<LineId>) {
            let course: CourseId =
                t::courses::table.filter(t::courses::slug.eq(crate::demo_course::SLUG)).select(t::courses::id).first(conn).unwrap();
            let lessons: Vec<LessonId> = t::course_lessons::table
                .filter(t::course_lessons::course_id.eq(course))
                .order(t::course_lessons::ord)
                .select(t::course_lessons::lesson_id)
                .load(conn)
                .unwrap();
            let mut words: Vec<WordId> =
                t::words::table.filter(t::words::lesson_id.eq_any(&lessons)).select(t::words::id).load(conn).unwrap();
            let mut sentences: Vec<SentenceId> =
                t::sentences::table.filter(t::sentences::lesson_id.eq_any(&lessons)).select(t::sentences::id).load(conn).unwrap();
            let mut lines: Vec<LineId> = t::conversation_lines::table
                .filter(t::conversation_lines::lesson_id.eq_any(&lessons))
                .select(t::conversation_lines::id)
                .load(conn)
                .unwrap();
            words.sort();
            sentences.sort();
            lines.sort();
            (lessons, words, sentences, lines)
        };
        let fresh_demo = demo_ids(&mut fresh);
        // The old rows of the demo, as SQL: blocks 200…, words 1000…,
        // sentences 2000…, lines 3000….
        let mut demo_sql = String::from(
            "INSERT INTO topics (id, slug, ord, kind, title_da, title_en, summary_en, created_at)
                 VALUES (3, 'demo-first-steps', 3, 'course', 'Pierwsze kroki', 'First steps', '', 0);",
        );
        let (mut word_no, mut sentence_no, mut line_no) = (1000, 2000, 3000);
        let mut first_demo_word = None;
        let q = |text: &str| text.replace('\'', "''");
        for (place, (lesson, kind, title)) in demo_lessons.iter().enumerate() {
            let block = 200 + place;
            demo_sql.push_str(&format!(
                "INSERT INTO blocks (id, kind, format, ord, title, complexity, period_days, origin, created_at)
                     VALUES ({block}, 'drill', '{kind}', {place}, '{}', 'low', 3, 'starter', 0);
                 INSERT INTO course_blocks (topic_id, block_id, ord) VALUES (3, {block}, {place});",
                q(title)
            ));
            let words: Vec<(i64, String, String, String)> = t::words::table
                .filter(t::words::lesson_id.eq(lesson))
                .order((t::words::ord, t::words::id))
                .select((t::words::ord, t::words::da, t::words::en, t::words::pos))
                .load(&mut fresh)
                .unwrap();
            for (ord, da, en, pos) in words {
                first_demo_word.get_or_insert((word_no, da.clone()));
                demo_sql.push_str(&format!(
                    "INSERT INTO words (id, block_id, ord, da, en, pos) VALUES ({word_no}, {block}, {ord}, '{}', '{}', '{}');",
                    q(&da), q(&en), q(&pos)
                ));
                word_no += 1;
            }
            let sentences: Vec<(i64, String, String)> = t::sentences::table
                .filter(t::sentences::lesson_id.eq(lesson))
                .order((t::sentences::ord, t::sentences::id))
                .select((t::sentences::ord, t::sentences::da, t::sentences::en))
                .load(&mut fresh)
                .unwrap();
            for (ord, da, en) in sentences {
                demo_sql.push_str(&format!(
                    "INSERT INTO sentences (id, block_id, ord, da, en) VALUES ({sentence_no}, {block}, {ord}, '{}', '{}');",
                    q(&da), q(&en)
                ));
                sentence_no += 1;
            }
            let lines: Vec<(i64, String, String, String, String)> = t::conversation_lines::table
                .filter(t::conversation_lines::lesson_id.eq(lesson))
                .order((t::conversation_lines::ord, t::conversation_lines::id))
                .select((
                    t::conversation_lines::ord,
                    t::conversation_lines::speaker,
                    t::conversation_lines::gender,
                    t::conversation_lines::da,
                    t::conversation_lines::en,
                ))
                .load(&mut fresh)
                .unwrap();
            for (ord, speaker, gender, da, en) in lines {
                demo_sql.push_str(&format!(
                    "INSERT INTO conversation_lines (id, block_id, ord, speaker, gender, da, en)
                         VALUES ({line_no}, {block}, {ord}, '{}', '{gender}', '{}', '{}');",
                    q(&speaker), q(&da), q(&en)
                ));
                line_no += 1;
            }
        }
        drop(fresh);
        let (old_demo_word, demo_word_da) = first_demo_word.expect("the demo has words");

        {
            // The schema before the rename, so raw SQL.
            let mut old = Conn::establish(&path.to_string_lossy()).unwrap();
            old.batch_execute("PRAGMA foreign_keys = ON;").unwrap();
            old.batch_execute(include_str!("../test-fixtures/schema-v5.sql")).unwrap();
            old.batch_execute(
                "INSERT INTO topics (id, slug, ord, kind, title_da, title_en, summary_en, created_at)
                     VALUES (1, 'lessons', 1, 'course', 'Lektioner', 'Lessons', '', 0),
                            (2, 'bakery', 2, 'course', 'Bageri', 'Bakery', '', 0);
                 INSERT INTO blocks (id, kind, format, ord, title, complexity, lesson, period_days, origin, created_at, passed)
                     VALUES (10, 'study', 'study', 1, 'At have', 'low', '**Jeg har** — I have', 7, 'tutor', 0, 1),
                            (11, 'drill', 'words', 2, 'Words 1', 'low', '', 3, 'tutor', 0, 0);
                 INSERT INTO course_blocks (topic_id, block_id, ord) VALUES (1, 10, 1), (2, 11, 1), (2, 10, 2);
                 INSERT INTO words (id, block_id, ord, da, en, pos) VALUES (5, 11, 1, 'brød', 'bread', 'n-common');
                 INSERT INTO checkins (id, block_id, started_at, result) VALUES (3, 10, 0, 'pass');
                 INSERT INTO wiki_sources (block_id, lang, title, url, pageid, revid, summary, content, fetched_at, checked_at)
                     VALUES (10, 'en', 'Danish grammar', 'https://en.wikipedia.org/wiki/Danish_grammar', 1, 1, 'x', 'x', 0, 0);
                 INSERT INTO reviews (kind, item_id, due_day, interval, ease, reps, lapses, last_day)
                     VALUES ('word', 5, 100, 3, 2.5, 3, 0, 97);
                 INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at, topic_id)
                     VALUES (1, 'Brød', 'Brød', 'Bread', 'pop', '[]', '', 'V5', 'ready', 0, 2);
                 INSERT INTO songs (id, title, chorus_da, chorus_en, style, sections, lyrics, suno_model, status, created_at, origin)
                     VALUES (2, 'Brød again', 'Brød', 'Bread', 'pop', '[]', '', 'V5', 'ready', 1, '{\"basedOn\":1,\"brief\":\"the same\"}');
                 INSERT INTO tutor_conversations (id, title, created_at, updated_at) VALUES (4, 'Bread', 0, 0);
                 INSERT INTO tutor_messages (id, role, content, actions, created_at, conversation_id)
                     VALUES (6, 'assistant', 'Made it.',
                             '[{\"kind\":\"lessonCreated\",\"lessonId\":11,\"courseId\":2},{\"kind\":\"songMade\",\"songId\":1,\"title\":\"Brød\"}]',
                             0, 4);
                 INSERT INTO settings (key, value) VALUES ('daily_ids', '[11, 10]'), ('daily_day', '100'),
                     ('demo_first_steps_seeded_v1', '1');
                 PRAGMA user_version = 5;",
            )
            .unwrap();
            old.batch_execute(&demo_sql).unwrap();
            old.batch_execute(&format!(
                "INSERT INTO reviews (kind, item_id, due_day, interval, ease, reps, lapses, last_day)
                     VALUES ('word', {old_demo_word}, 100, 3, 2.5, 5, 0, 97);"
            ))
            .unwrap();
        }

        let mut conn = db::open(&path).expect("upgrade");
        // The database as it was is kept, in case the upgrade went wrong.
        let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
        let backups: Vec<_> = std::fs::read_dir(path.parent().unwrap().join("backups"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(&stem))
            .collect();
        assert!(!backups.is_empty(), "backed up before the upgrade");
        // Ids are tagged now (version 7): rows are found by what they hold.
        let titled = |conn: &mut Conn, title: &str| -> LessonId {
            t::lessons::table.filter(t::lessons::title.eq(title)).select(t::lessons::id).first(conn).unwrap()
        };
        let at_have = titled(&mut conn, "At have");
        let words_1 = titled(&mut conn, "Words 1");
        let bakery: CourseId = t::courses::table.filter(t::courses::slug.eq("bakery")).select(t::courses::id).first(&mut conn).unwrap();
        assert_eq!(at_have.to_string().split('_').next(), Some("ls"), "a lesson's id is tagged");
        let study = lessons::detail(&mut conn, at_have).unwrap();
        assert_eq!(study.content, "**Jeg har** — I have", "a study lesson keeps its text");
        assert!(study.lesson.passed);
        // The old built-in folder (course 1, Grammar & culture) is merged
        // into Other lessons. The lesson was in two courses: it stays in one
        // of them (version 18), with its history.
        let other: CourseId = t::courses::table.filter(t::courses::slug.eq(courses::OTHER)).select(t::courses::id).first(&mut conn).unwrap();
        assert_eq!(study.lesson.course_ids.len(), 1, "a lesson is in one course");
        assert!([bakery, other].contains(&study.lesson.course_ids[0]), "{:?}", study.lesson.course_ids);
        let bread: i64 = t::course_words::table
            .filter(t::course_words::course_id.eq(bakery))
            .filter(t::course_words::da.eq("brød"))
            .count()
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(bread, 1, "the views are made again under the new names");
        let checkin: LessonId = t::checkins::table.select(t::checkins::lesson_id).first(&mut conn).unwrap();
        assert_eq!(checkin, at_have);
        let source: Option<LessonId> = t::wiki_sources::table.select(t::wiki_sources::lesson_id).first(&mut conn).unwrap();
        assert_eq!(source, Some(at_have));
        let title: String = t::courses::table.find(other).select(t::courses::title_en).first(&mut conn).unwrap();
        assert_eq!(title, "Other lessons");
        assert_eq!(
            t::courses::table.filter(t::courses::slug.eq("lessons")).count().get_result::<i64>(&mut conn).unwrap(),
            0,
            "the old folder is gone"
        );
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);

        // Version 7: links survive under the new ids — review history, a
        // song's course, the ids inside stored JSON, and the day's list.
        let brod: WordId = t::words::table.filter(t::words::da.eq("brød")).select(t::words::id).first(&mut conn).unwrap();
        assert_eq!(srs::load(&mut conn, "word", &brod.to_string()).unwrap().unwrap().reps, 3, "review history follows the word");
        let first: SongId = t::songs::table.filter(t::songs::title.eq("Brød")).select(t::songs::id).first(&mut conn).unwrap();
        let song_course: Option<CourseId> =
            t::songs::table.find(first).select(t::songs::course_id).first(&mut conn).unwrap();
        assert_eq!(song_course, Some(bakery));
        let origin: String = t::songs::table.filter(t::songs::title.eq("Brød again")).select(t::songs::origin).first(&mut conn).unwrap();
        let origin: serde_json::Value = serde_json::from_str(&origin).unwrap();
        assert_eq!(origin["basedOn"], serde_json::json!(first), "a variant points at its song's new id");
        assert_eq!(origin["brief"], "the same");
        let conversation: ConversationId =
            t::tutor_conversations::table.filter(t::tutor_conversations::title.eq("Bread")).select(t::tutor_conversations::id).first(&mut conn).unwrap();
        let history = tutor::load_history(&mut conn, conversation).unwrap();
        assert_eq!(history.len(), 1, "a message stays in its conversation");
        let actions: Option<String> = t::tutor_messages::table.select(t::tutor_messages::actions).first(&mut conn).unwrap();
        let actions: serde_json::Value = serde_json::from_str(&actions.unwrap()).unwrap();
        assert_eq!(actions[0]["lessonId"], serde_json::json!(words_1), "{actions}");
        assert_eq!(actions[0]["courseId"], serde_json::json!(bakery), "{actions}");
        assert_eq!(actions[1]["songId"], serde_json::json!(first), "{actions}");
        assert_eq!(actions[1]["title"], "Brød", "other fields stay");
        let daily: Vec<LessonId> = serde_json::from_str(&db::get_setting(&mut conn, "daily_ids").unwrap().unwrap()).unwrap();
        assert_eq!(daily, vec![words_1, at_have], "the day's list keeps its lessons, in order");

        // The demo course gets the ids a fresh install gives it, and its
        // review history follows.
        assert_eq!(demo_ids(&mut conn), fresh_demo, "the demo's ids are derived, as on a fresh install");
        let demo_course_now: CourseId =
            t::courses::table.filter(t::courses::slug.eq(crate::demo_course::SLUG)).select(t::courses::id).first(&mut conn).unwrap();
        assert_eq!(demo_course_now, demo_course);
        let demo_word: WordId = t::words::table
            .filter(t::words::da.eq(&demo_word_da))
            .filter(t::words::lesson_id.eq_any(&fresh_demo.0))
            .select(t::words::id)
            .first(&mut conn)
            .unwrap();
        assert_eq!(srs::load(&mut conn, "word", &demo_word.to_string()).unwrap().unwrap().reps, 5);
        assert_eq!(db::foreign_key_violations(&mut conn).unwrap(), 0);

        // References follow the renamed tables: deleting a lesson still
        // takes its words and course listings with it.
        diesel::delete(t::lessons::table.find(words_1)).execute(&mut conn).unwrap();
        assert_eq!(t::words::table.find(brod).count().get_result::<i64>(&mut conn).unwrap(), 0);
        assert_eq!(
            t::course_lessons::table.filter(t::course_lessons::course_id.eq(bakery)).count().get_result::<i64>(&mut conn).unwrap(),
            1
        );
        #[derive(QueryableByName)]
        struct Name {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let old_names = diesel::sql_query(
            "SELECT name FROM sqlite_master WHERE name IN ('blocks', 'topics', 'course_blocks')
                 OR sql LIKE '%block_id%' OR sql LIKE '%topic_id%' OR name GLOB '*__old'",
        )
        .load::<Name>(&mut conn)
        .unwrap();
        assert!(old_names.is_empty(), "left with old names: {:?}", old_names.iter().map(|n| &n.name).collect::<Vec<_>>());

        // Opening again changes nothing.
        drop(conn);
        let mut conn = db::open(&path).expect("reopen");
        assert_eq!(lessons::detail(&mut conn, at_have).unwrap().content, "**Jeg har** — I have");
        assert_eq!(demo_ids(&mut conn), fresh_demo);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_danish_era_database_is_upgraded_in_place() {
        let path = std::env::temp_dir()
            .join("learn-danish-tests")
            .join(format!("legacy-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        {
            // An old schema, so raw SQL: these tables are not in crate::schema.
            let mut old = Conn::establish(&path.to_string_lossy()).unwrap();
            old.batch_execute(
                "CREATE TABLE topics (id INTEGER PRIMARY KEY, slug TEXT NOT NULL UNIQUE, ord INTEGER NOT NULL,
                     title_da TEXT NOT NULL, title_en TEXT NOT NULL, title_legacy TEXT NOT NULL,
                     summary_en TEXT NOT NULL, summary_da TEXT NOT NULL);
                 CREATE TABLE words (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL REFERENCES topics(id),
                     ord INTEGER NOT NULL, da TEXT NOT NULL UNIQUE, en TEXT NOT NULL, legacy_translation TEXT NOT NULL, pos TEXT NOT NULL);
                 CREATE TABLE sentences (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL REFERENCES topics(id),
                     ord INTEGER NOT NULL, da TEXT NOT NULL UNIQUE, en TEXT NOT NULL, legacy_translation TEXT NOT NULL);
                 CREATE TABLE conversation_lines (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL REFERENCES topics(id),
                     ord INTEGER NOT NULL, speaker TEXT NOT NULL, da TEXT NOT NULL, en TEXT NOT NULL, legacy_translation TEXT NOT NULL);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE reviews (kind TEXT NOT NULL, item_id INTEGER NOT NULL, due_day INTEGER NOT NULL,
                     interval INTEGER NOT NULL, ease REAL NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL,
                     last_day INTEGER NOT NULL, PRIMARY KEY (kind, item_id));
                 CREATE TABLE chat_messages (id INTEGER PRIMARY KEY, topic_id INTEGER NOT NULL, role TEXT NOT NULL,
                     content TEXT NOT NULL, created_at INTEGER NOT NULL);
                 INSERT INTO topics VALUES (1, 'basics', 1, 'Det første møde', 'Meeting', 'Møde', 'Hi', 'Hej');
                 INSERT INTO words VALUES (7, 1, 0, 'hej', 'hi, bye', 'hej', 'interj');
                 INSERT INTO reviews VALUES ('word', 7, 100, 3, 2.5, 2, 0, 97);
                 INSERT INTO settings VALUES ('source_language', 'da'), ('content_version', '2');",
            )
            .unwrap();
        }

        let mut conn = db::open(&path).expect("upgrade");
        let basics: CourseId = t::courses::table.filter(t::courses::slug.eq("basics")).select(t::courses::id).first(&mut conn).unwrap();
        let (id, en): (WordId, String) = t::course_words::table
            .filter(t::course_words::course_id.eq(basics))
            .filter(t::course_words::da.eq("hej"))
            .select((t::course_words::id, t::course_words::en))
            .first(&mut conn)
            .unwrap();
        assert_eq!(en, "hi, bye");
        assert_eq!(srs::load(&mut conn, "word", &id.to_string()).unwrap().unwrap().reps, 2, "review history follows the word to its new id");
        #[derive(QueryableByName)]
        struct Column {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let columns = diesel::sql_query("SELECT name FROM pragma_table_info('words')")
            .load::<Column>(&mut conn)
            .unwrap();
        assert!(!columns.iter().any(|c| c.name == "legacy_translation"));
        assert_eq!(db::get_setting(&mut conn, "source_language").unwrap(), None);
        // Only the small demo is added to this older database.
        let courses: i64 = t::courses::table.count().get_result(&mut conn).unwrap();
        assert_eq!(courses, 2);
        let basics_words: i64 =
            t::course_words::table.filter(t::course_words::course_id.eq(basics)).filter(t::course_words::da.eq("hej")).count().get_result(&mut conn).unwrap();
        assert_eq!(basics_words, 1);

        // Opening again is a no-op.
        drop(conn);
        db::open(&path).expect("second open");
    }

    #[test]
    fn say_players_read_as_plain_text() {
        assert_eq!(ai::unsay("Say [[say: opvarmning]] slowly"), "Say opvarmning slowly");
        assert_eq!(ai::unsay("[[say:m: Godmorgen]]!"), "Godmorgen!");
        assert_eq!(ai::unsay("broken [[say: x"), "broken [[say: x");
        assert!(ai::with_say_player("p".into()).contains("[[say:"));
    }


    /// One answer, put in the learning log on a chosen day.
    #[allow(clippy::too_many_arguments)]
    fn logged(conn: &mut Conn, day: i64, item_kind: &str, item_id: &str, lesson: LessonId, exercise: &str, correct: bool, given: &str, expected: &str, mistake: &str, detail: &str, contrast: &str) {
        diesel::insert_into(t::attempts::table)
            .values((
                t::attempts::id.eq(AttemptId::new()),
                t::attempts::at.eq(day * 86_400_000 + 10 * 3_600_000),
                t::attempts::day.eq(day),
                t::attempts::device.eq(crate::sync::device_id(conn).unwrap()),
                t::attempts::item_kind.eq(item_kind),
                t::attempts::item_id.eq(item_id),
                t::attempts::lesson_id.eq(Some(lesson)),
                t::attempts::exercise.eq(exercise),
                t::attempts::correct.eq(i64::from(correct)),
                t::attempts::given.eq(given),
                t::attempts::expected.eq(expected),
                t::attempts::mistake.eq(mistake),
                t::attempts::detail.eq(detail),
                t::attempts::contrast.eq(contrast),
            ))
            .execute(conn)
            .unwrap();
    }

    #[test]
    fn grammar_tags_come_with_new_content_and_are_checked() {
        let mut conn = scratch_db("grammar-tags");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let made = tutor::run_tool_for_test(&mut conn, "create_drill_lesson", serde_json::json!({
            "title": "Hvor", "format": "sentences", "complexity": "low", "course_id": basics,
            "sentences": [{ "da": "Jeg bor i huset.", "en": "I live in the house.", "grammar": ["i-place", "definite-singular", "i-place"] }]
        }));
        let lesson: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let sentence = session::lesson_sentences(&mut conn, lesson).unwrap().remove(0).0;
        let links = crate::grammar::all_links(&mut conn).unwrap();
        assert_eq!(links[&sentence.id.to_string()].len(), 2, "duplicates dropped");
        // A made-up tag fails the call, by name, and nothing is made.
        let refused = tutor::run_tool_for_test(&mut conn, "create_drill_lesson", serde_json::json!({
            "title": "Forkert", "format": "words", "complexity": "low", "course_id": basics,
            "words": [{ "da": "en kat", "en": "cat", "pos": "n-common", "grammar": ["made-up-tag"] }]
        }));
        assert!(refused["error"].as_str().unwrap().contains("made-up-tag"), "{refused}");
        // Existing words: tagged when asked, through the tutor.
        let before = crate::grammar::untagged_count(&mut conn).unwrap();
        let listed = tutor::run_tool_for_test(&mut conn, "list_untagged", serde_json::json!({ "course_id": basics, "limit": 3 }));
        assert_eq!(listed["items"].as_array().unwrap().len(), 3);
        assert_eq!(listed["total"], before);
        let id = listed["items"][0]["id"].as_str().unwrap().to_string();
        let tagged = tutor::run_tool_for_test(&mut conn, "tag_grammar", serde_json::json!({ "items": [{ "id": id, "grammar": ["common-gender"] }] }));
        assert_eq!(tagged["still_untagged"], before - 1, "{tagged}");
        assert!(tagged["_action"]["label"].as_str().unwrap().starts_with("Tagged the grammar"));
        let wrong = tutor::run_tool_for_test(&mut conn, "tag_grammar", serde_json::json!({ "items": [{ "id": lesson.to_string(), "grammar": ["common-gender"] }] }));
        assert!(wrong["error"].is_string(), "a lesson id is not a word or sentence: {wrong}");
        // The course as the tutor reads it carries ids and tags.
        let course = tutor::run_tool_for_test(&mut conn, "get_course", serde_json::json!({ "course_id": basics }));
        assert!(course["sentences"].as_array().unwrap().iter().any(|s| s["grammar"] == serde_json::json!(["i-place", "definite-singular"]) || s["grammar"] == serde_json::json!(["definite-singular", "i-place"])));
        // Deleting the lesson takes its tags (cascade).
        lessons::delete(&mut conn, lesson).unwrap();
        assert!(!crate::grammar::all_links(&mut conn).unwrap().contains_key(&sentence.id.to_string()));
    }

    #[test]
    fn analytics_on_a_fixed_history() {
        let mut conn = scratch_db("analytics");
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let made = tutor::run_tool_for_test(&mut conn, "create_drill_lesson", serde_json::json!({
            "title": "Fixed", "format": "words", "complexity": "low", "course_id": basics,
            "words": [{ "da": "kat", "en": "cat", "pos": "n-common", "grammar": ["common-gender"] }, { "da": "hund", "en": "dog", "pos": "n-common" }],
            "sentences": [{ "da": "Jeg bor i huset.", "en": "I live in the house.", "grammar": ["i-place", "definite-singular"] }]
        }));
        let lesson: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let words = session::lesson_words(&mut conn, lesson).unwrap();
        let id = |da: &str| words.iter().find(|w| w.0.da == da).unwrap().0.id.to_string();
        let (kat, hund) = (id("kat"), id("hund"));
        let sentence = session::lesson_sentences(&mut conn, lesson).unwrap().remove(0).0.id.to_string();
        let today = db::today(&mut conn);
        let set = lesson.to_string();
        logged(&mut conn, today - 20, "word", &kat, lesson, "type", false, "kta", "kat", "typo", "kat→kta", "");
        logged(&mut conn, today - 19, "word", &kat, lesson, "type", false, "katten", "kat", "ending", "kat→katten", "");
        logged(&mut conn, today - 2, "word", &kat, lesson, "type", true, "kat", "kat", "", "", "");
        logged(&mut conn, today - 1, "word", &kat, lesson, "recognise", true, "cat", "cat", "", "", "");
        logged(&mut conn, today - 3, "word", &hund, lesson, "type", false, "kat", "hund", "unknown_word", "hund→kat", "");
        logged(&mut conn, today - 1, "word", &hund, lesson, "type", false, "bil", "hund", "unknown_word", "hund→bil", "");
        logged(&mut conn, today - 15, "sentence", &sentence, lesson, "wordOrder", false, "Jeg bor i husene.", "Jeg bor i huset.", "ending", "huset→husene", "");
        logged(&mut conn, today - 1, "sentence", &sentence, lesson, "wordOrder", true, "Jeg bor i huset.", "Jeg bor i huset.", "", "", "");
        logged(&mut conn, today - 5, "set", &set, lesson, "soundPair", false, "tre", "træ", "sound_contrast", "æ→e", "æ or e");
        logged(&mut conn, today - 4, "set", &set, lesson, "soundPair", true, "træ", "træ", "", "æ", "æ or e");
        use crate::analytics::{self as a, Filter};
        let all = Filter::default();

        let o = a::overview(&mut conn, &all).unwrap();
        assert_eq!((o.attempts, o.correct, o.items, o.days_active), (10, 4, 4, 8));
        assert!((o.accuracy - 0.4).abs() < 1e-9);
        let classes: Vec<(&str, i64)> = o.mistakes.iter().map(|c| (c.class.as_str(), c.count)).collect();
        assert_eq!(classes, [("ending", 2), ("unknown_word", 2), ("sound_contrast", 1), ("typo", 1)]);
        assert_eq!(o.daily.iter().map(|d| d.attempts).sum::<i64>(), 10);
        assert_eq!(o.words_met, 2);

        let week = Filter::period(&mut conn, "week", None).unwrap();
        let w = a::overview(&mut conn, &week).unwrap();
        assert_eq!((w.attempts, w.correct, w.accuracy_before, w.trend.as_str()), (7, 4, None, "new"), "nothing the week before");
        let fortnight = a::overview(&mut conn, &Filter { from_day: Some(today - 13), to_day: None, course_id: None }).unwrap();
        assert_eq!((fortnight.attempts, fortnight.correct), (7, 4));
        assert_eq!(fortnight.accuracy_before, Some(0.0), "the two weeks before: three misses");
        assert_eq!(fortnight.trend, "improving");

        let hard = a::hard_items(&mut conn, "word", &all, 10).unwrap();
        assert_eq!(hard.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), ["hund", "kat"], "the latest misses weigh most");
        assert_eq!((hard[0].attempts, hard[0].wrong), (2, 2));
        assert_eq!(hard[0].last[0].given, "bil", "newest first");
        let structures = a::hard_items(&mut conn, "structure", &all, 10).unwrap();
        assert!(structures.iter().any(|s| s.key == "i-place" && s.attempts == 2 && s.wrong == 1));

        let words = a::met_words(&mut conn).unwrap();
        let strength = |da: &str| words.iter().find(|w| w.da == da).unwrap().strength;
        assert_eq!(strength("hund"), a::Strength::Tricky);

        let history = a::item_history(&mut conn, &kat).unwrap();
        assert_eq!((history.attempts, history.wrong, history.history[0].given.as_str()), (4, 2, "cat"));
        assert_eq!(history.grammar[0].key, "common-gender");

        let st = a::structure(&mut conn, "i-place", &all).unwrap();
        assert_eq!((st.row.items, st.row.attempts, st.row.correct), (1, 2, 1));
        assert_eq!(st.mistakes[0].given, "Jeg bor i husene.");

        let sounds = a::sounds(&mut conn, &all).unwrap();
        assert_eq!((sounds[0].label.as_str(), sounds[0].attempts, sounds[0].hits), ("æ or e", 2, 1));
        let ae = sounds[0].sides.iter().find(|s| s.said == "æ").unwrap();
        assert_eq!((ae.attempts, ae.missed, ae.heard_as.as_str()), (2, 1, "e"));

        let endings = a::mistakes_of(&mut conn, "ending", &all, 10).unwrap();
        assert_eq!(endings.count, 2);
        assert!((endings.share - 2.0 / 6.0).abs() < 1e-9);
        assert!(a::mistakes_of(&mut conn, "grumpy", &all, 10).is_err());

        let earlier = Filter { from_day: Some(today - 27), to_day: Some(today - 14), course_id: None };
        let later = Filter { from_day: Some(today - 13), to_day: Some(today), course_id: None };
        let c = a::compare(&mut conn, &earlier, &later).unwrap();
        assert_eq!((c.a.attempts, c.b.attempts), (3, 7));
        assert!(c.improved.iter().any(|x| x.title == "kat" && x.accuracy_a == 0.0 && x.accuracy_b == 1.0), "{:?}", c.improved);

        let act = a::activity(&mut conn, &all).unwrap();
        assert_eq!(act.by_hour.iter().sum::<i64>(), 10);
        assert_eq!(act.by_hour[10], 10, "all at ten in the morning (UTC offset 0)");
        assert_eq!(act.by_weekday.iter().sum::<i64>(), 10);

        // The course filter: another course's answers are left out.
        let other = db::require_course_id(&mut conn, 2).unwrap();
        assert_eq!(a::overview(&mut conn, &Filter { course_id: Some(other), ..all }).unwrap().attempts, 0);

        // The tutor's tools, on the same history.
        let run = |conn: &mut Conn, name: &str, args: serde_json::Value| tutor::run_tool_for_test(conn, name, args);
        let ov = run(&mut conn, "get_learning_overview", serde_json::json!({ "period": "all" }));
        assert_eq!((ov["answers"].as_i64(), ov["accuracy_pct"].as_i64()), (Some(10), Some(40)), "{ov}");
        let hard = run(&mut conn, "find_hard_items", serde_json::json!({ "kind": "word", "limit": 1 }));
        assert_eq!(hard["items"].as_array().unwrap().len(), 1);
        assert_eq!(hard["items"][0]["title"], "hund");
        assert_eq!(hard["items"][0]["latest_misses"][0]["given"], "bil");
        let item = run(&mut conn, "get_item_history", serde_json::json!({ "item_id": kat }));
        assert_eq!(item["answers"], 4);
        let st = run(&mut conn, "get_structure_stats", serde_json::json!({ "tag": "i-place", "period": "all" }));
        assert_eq!((st["answers"].as_i64(), st["accuracy_pct"].as_i64()), (Some(2), Some(50)));
        let m = run(&mut conn, "get_mistakes", serde_json::json!({ "class": "ending", "period": "all" }));
        assert_eq!((m["count"].as_i64(), m["examples"].as_array().unwrap().len()), (Some(2), 2));
        let date = |day: i64| { let (y, mo, d) = crate::commands::civil_date(day); format!("{y}-{mo:02}-{d:02}") };
        let cmp = run(&mut conn, "compare_periods", serde_json::json!({ "a_from": date(today - 27), "a_to": date(today - 14), "b_from": date(today - 13), "b_to": date(today) }));
        assert_eq!((cmp["a"]["answers"].as_i64(), cmp["b"]["answers"].as_i64()), (Some(3), Some(7)), "{cmp}");
        let defaulted = run(&mut conn, "compare_periods", serde_json::json!({}));
        assert_eq!(defaulted["b"]["answers"], 7, "the last 14 days by default");
        let act = run(&mut conn, "get_activity", serde_json::json!({ "period": "all" }));
        assert_eq!(act["answers"], 10);
        let bad = run(&mut conn, "find_hard_items", serde_json::json!({ "kind": "verb" }));
        assert!(bad["error"].is_string());
        let bad = run(&mut conn, "get_learning_overview", serde_json::json!({ "period": "decade" }));
        assert!(bad["error"].as_str().unwrap().contains("decade"));
    }

    #[test]
    fn games_and_check_ins_write_to_the_log() {
        let mut conn = scratch_db("log-games");
        let sentence: SentenceId = t::sentences::table.select(t::sentences::id).first(&mut conn).unwrap();
        let heard = crate::games::answer(&mut conn, crate::games::GameAnswer {
            game: "say".into(), item_id: Some(sentence.to_string()), expected: "Jeg bor i en by.".into(), given: "jeg bor i en bi".into(), ..Default::default()
        }).unwrap();
        assert!(!heard.correct);
        let pair = crate::sounds::built_in().remove(0);
        crate::games::answer(&mut conn, crate::games::GameAnswer {
            game: "sounds".into(), item_id: pair.id.map(|i| i.to_string()), expected: pair.b.clone(), given: pair.a.clone(),
            options: vec![pair.a.clone(), pair.b.clone()], ..Default::default()
        }).unwrap();
        assert!(crate::games::answer(&mut conn, crate::games::GameAnswer { game: "flip".into(), ..Default::default() }).is_err());
        let rows: Vec<(String, String, String, String)> = t::attempts::table.order(t::attempts::id)
            .select((t::attempts::exercise, t::attempts::item_kind, t::attempts::mistake, t::attempts::contrast)).load(&mut conn).unwrap();
        assert_eq!(rows[0], ("game:say".into(), "sentence".into(), "sound_contrast".into(), "y or i".into()));
        assert_eq!((rows[1].0.as_str(), rows[1].1.as_str(), rows[1].2.as_str(), rows[1].3.as_str()), ("game:sounds", "sound", "sound_contrast", "æ or e"));

        // A check-in's verdict: one answer for it, one per slip the examiner noted.
        let lesson = starter_lesson(&mut conn);
        let checkin = CheckinId::new();
        diesel::insert_into(t::checkins::table)
            .values((t::checkins::id.eq(checkin), t::checkins::lesson_id.eq(lesson.id), t::checkins::started_at.eq(0)))
            .execute(&mut conn).unwrap();
        let verdict: crate::examiner::Assessment = serde_json::from_value(serde_json::json!({
            "result": "partial", "summary": "Close.",
            "mistakes": [{ "said": "Jeg har et bil", "correct": "Jeg har en bil", "grammar": ["common-gender", "made-up"], "note": "bil is an en-word" }]
        })).unwrap();
        crate::examiner::record(&mut conn, checkin, verdict).unwrap();
        let logged: Vec<(String, i64, String, String)> = t::attempts::table.filter(t::attempts::run.eq(checkin.to_string())).order(t::attempts::id)
            .select((t::attempts::exercise, t::attempts::correct, t::attempts::mistake, t::attempts::details)).load(&mut conn).unwrap();
        assert_eq!(logged.len(), 2);
        assert_eq!((logged[0].0.as_str(), logged[0].1), ("checkin", 0));
        assert_eq!((logged[1].0.as_str(), logged[1].2.as_str()), ("checkinMistake", "ending"));
        assert_eq!(crate::learnlog::read_details(&logged[1].3).grammar, ["common-gender"], "an unknown tag dropped");
        // The structure counts the slip.
        let st = crate::analytics::structure(&mut conn, "common-gender", &crate::analytics::Filter::default()).unwrap();
        assert_eq!((st.row.attempts, st.mistakes.len()), (1, 1));
    }

    #[test]
    fn logging_a_long_session_stays_fast() {
        let mut conn = scratch_db("log-speed");
        let ids: Vec<WordId> = t::words::table.select(t::words::id).limit(50).load(&mut conn).unwrap();
        let sentences: Vec<(SentenceId, String, LessonId)> = t::sentences::table.select((t::sentences::id, t::sentences::da, t::sentences::lesson_id)).limit(20).load(&mut conn).unwrap();
        let context = crate::learnlog::PracticeContext { source: "daily".into(), course_id: None };
        let started = std::time::Instant::now();
        for i in 0..400 {
            let id = ids[i % ids.len()];
            // Every fourth a dictation: its words found and linked with the grade.
            let answer = if i % 4 == 0 {
                let (sentence, da, lesson) = &sentences[i % sentences.len()];
                practice::SubmitAnswer {
                    kind: "dictation".into(), item_kind: "sentence".into(), item_id: sentence.to_string(), expected: da.clone(),
                    given: da.to_lowercase(), lesson_id: Some(*lesson), run: "long".into(), context: Some(context.clone()), ..Default::default()
                }
            } else {
                practice::SubmitAnswer {
                    kind: "recognise".into(), item_kind: "word".into(), item_id: id.to_string(), expected: "kaffen".into(),
                    given: if i % 3 == 0 { "kaffe".into() } else { "kaffen".into() }, run: "long".into(), options: vec!["kaffen".into(), "kaffe".into(), "teen".into(), "vandet".into()],
                    card: Some(crate::learnlog::CardShown { key: format!("k{i}"), position: Some(i64::try_from(i).unwrap() + 1), of_cards: Some(400), ..Default::default() }),
                    context: Some(context.clone()),
                    signals: Some(crate::learnlog::Signals { ms_answer: Some(3000), ..Default::default() }), ..Default::default()
                }
            };
            practice::submit_answer_in(&mut conn, answer).unwrap();
        }
        let answers = started.elapsed();
        assert!(t::attempt_items::table.count().get_result::<i64>(&mut conn).unwrap() > 400, "the words of the sentences are linked");
        let events: Vec<crate::learnlog::EventIn> = (0..200)
            .map(|i| crate::learnlog::EventIn { kind: "audio_played".into(), item_id: ids[i % ids.len()].to_string(), item_kind: "word".into(), ..Default::default() })
            .collect();
        let started = std::time::Instant::now();
        assert_eq!(crate::learnlog::log_events(&mut conn, events).unwrap(), 200);
        let batch = started.elapsed();
        assert_eq!(t::attempts::table.count().get_result::<i64>(&mut conn).unwrap(), 400);
        // Generous bounds for a debug build on a slow machine: each answer
        // (grade, schedule, XP and its log row) in one short transaction,
        // and a batch of events in one.
        assert!(answers < std::time::Duration::from_secs(20), "400 answers took {answers:?}");
        assert!(batch < std::time::Duration::from_secs(2), "200 events took {batch:?}");
        // The pages read the long history quickly too.
        let started = std::time::Instant::now();
        crate::analytics::overview(&mut conn, &crate::analytics::Filter::default()).unwrap();
        crate::analytics::met_words(&mut conn).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "{:?}", started.elapsed());
        // The raw log: a page, deep or filtered, and a word in context.
        let started = std::time::Instant::now();
        let first = crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter::default(), None, 50).unwrap();
        crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter::default(), first.next.as_ref(), 50).unwrap();
        crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter { correct: Some(false), source: Some("daily".into()), ..Default::default() }, None, 50).unwrap();
        crate::contexts::item_contexts(&mut conn, &ids[0].to_string(), &crate::analytics::Filter::default()).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "{:?}", started.elapsed());
    }


    /// A lesson with kaffe, et glas and two annotated sentences, for the
    /// context tests: (lesson, kaffe, glas, "Jeg hælder kaffen i glasset.", "Det er et stort glas.").
    fn context_lesson(conn: &mut Conn) -> (LessonId, WordId, WordId, SentenceId, SentenceId) {
        let basics = db::require_course_id(conn, 1).unwrap();
        let made = tutor::run_tool_for_test(conn, "create_drill_lesson", serde_json::json!({
            "title": "Kaffe", "format": "words", "complexity": "low", "course_id": basics, "topics": ["food.drinks"],
            "words": [
                { "da": "kaffe", "en": "coffee", "pos": "n-common", "topics": ["food.drinks"] },
                { "da": "et glas", "en": "glass", "pos": "n-neuter", "topics": ["food.drinks"] },
                { "da": "te", "en": "tea", "pos": "n-common" }
            ],
            "sentences": [
                { "da": "Jeg hælder kaffen i glasset.", "en": "I pour the coffee into the glass.", "topics": ["food.drinks"],
                  "endings": [{ "form": "kaffen", "base": "kaffe", "wrong": ["kaffes", "kaffens"] }],
                  "tokens": [
                      { "form": "Jeg", "lemma": "jeg", "pos": "pron", "case": "subject", "person": "1", "number": "sg", "reason": "personal-pronouns" },
                      { "form": "hælder", "lemma": "hælde", "pos": "verb", "tense": "present", "voice": "active", "reason": "v2" },
                      { "form": "kaffen", "lemma": "kaffe", "pos": "noun", "number": "sg", "gender": "common", "definiteness": "definite-suffix", "reason": "definite-singular" },
                      { "form": "i", "lemma": "i", "pos": "prep" },
                      { "form": "glasset", "lemma": "glas", "pos": "noun", "number": "sg", "gender": "neuter", "definiteness": "definite-suffix", "reason": "definite-singular" }
                  ] },
                { "da": "Det er et stort glas.", "en": "It is a big glass.",
                  "tokens": [
                      { "form": "Det", "lemma": "det", "pos": "pron", "case": "subject", "gender": "neuter" },
                      { "form": "er", "lemma": "være", "pos": "verb", "tense": "present" },
                      { "form": "et", "lemma": "en", "pos": "art", "gender": "neuter", "definiteness": "indefinite", "reason": "neuter" },
                      { "form": "stort", "lemma": "stor", "pos": "adj", "gender": "neuter", "definiteness": "indefinite", "inflection": "t", "reason": "adjective-t" },
                      { "form": "glas", "lemma": "et glas", "pos": "noun", "number": "sg", "gender": "neuter", "definiteness": "indefinite", "reason": "neuter" }
                  ] }
            ]
        }));
        assert!(made["error"].is_null(), "{made}");
        let lesson: LessonId = serde_json::from_value(made["_action"]["lessonId"].clone()).unwrap();
        let words = session::lesson_words(conn, lesson).unwrap();
        let word = |da: &str| words.iter().find(|w| w.0.da == da).unwrap().0.id;
        let sentences = session::lesson_sentences(conn, lesson).unwrap();
        let sentence = |da: &str| sentences.iter().find(|s| s.0.da == da).unwrap().0.id;
        (lesson, word("kaffe"), word("et glas"), sentence("Jeg hælder kaffen i glasset."), sentence("Det er et stort glas."))
    }

    #[allow(clippy::too_many_arguments)]
    fn answer(conn: &mut Conn, lesson: LessonId, kind: &str, item_kind: &str, item: &str, expected: &str, given: &str, context: Option<crate::learnlog::PracticeContext>) -> AttemptId {
        practice::submit_answer_in(conn, practice::SubmitAnswer {
            kind: kind.into(), item_kind: item_kind.into(), item_id: item.into(), expected: expected.into(), given: given.into(),
            lesson_id: Some(lesson), run: "ctx".into(), context, ..Default::default()
        }).unwrap();
        t::attempts::table.order(t::attempts::id.desc()).select(t::attempts::id).first(conn).unwrap()
    }

    #[test]
    fn where_it_was_practised_is_recorded_from_every_entry_point() {
        use crate::learnlog::PracticeContext;
        let mut conn = scratch_db("log-context");
        let (lesson, kaffe, ..) = context_lesson(&mut conn);
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        // Opened from a course's page: the course the screen says.
        let second = db::require_course_id(&mut conn, 2).unwrap();
        let from_second = PracticeContext { source: "course".into(), course_id: Some(second) };
        let plan = practice::session_with(&mut conn, lesson, Some(&from_second)).unwrap();
        assert_eq!(plan.context, from_second, "the plan carries its context back to the screen");
        let row = |conn: &mut Conn, id: AttemptId| -> (Option<CourseId>, String) {
            t::attempts::table.find(id).select((t::attempts::course_id, t::attempts::source)).first(conn).unwrap()
        };
        let a = answer(&mut conn, lesson, "type", "word", &kaffe.to_string(), "kaffe", "kaffe", Some(plan.context.clone()));
        assert_eq!(row(&mut conn, a), (Some(second), "course".into()));
        // The daily five: no course given, so the lesson's own.
        let a = answer(&mut conn, lesson, "type", "word", &kaffe.to_string(), "kaffe", "kaffe", Some(PracticeContext { source: "daily".into(), course_id: None }));
        assert_eq!(row(&mut conn, a), (Some(basics), "daily".into()));
        // Other entry points.
        for source in ["other", "tutor", "music", "link", "search"] {
            let a = answer(&mut conn, lesson, "recognise", "word", &kaffe.to_string(), "coffee", "coffee", Some(PracticeContext { source: source.into(), course_id: None }));
            assert_eq!(row(&mut conn, a).1, source);
        }
        // An unknown source is a screen bug: refused by name.
        let bad = practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "type".into(), item_kind: "word".into(), item_id: kaffe.to_string(), expected: "kaffe".into(), given: "kaffe".into(),
            lesson_id: Some(lesson), context: Some(PracticeContext { source: "fridge".into(), course_id: None }), ..Default::default()
        });
        assert!(bad.unwrap_err().to_string().contains("fridge"));
        // A quick game: source game, no course.
        let sentence: SentenceId = t::sentences::table.select(t::sentences::id).first(&mut conn).unwrap();
        crate::games::answer(&mut conn, crate::games::GameAnswer { game: "dictation".into(), item_id: Some(sentence.to_string()), expected: "God morgen.".into(), given: "god morgen".into(), ..Default::default() }).unwrap();
        let game: (Option<CourseId>, String, String) = t::attempts::table.order(t::attempts::id.desc()).select((t::attempts::course_id, t::attempts::source, t::attempts::direction)).first(&mut conn).unwrap();
        assert_eq!(game, (None, "game".into(), "audio→da typed".into()));
        // A drill's end, with its context.
        practice::lesson_finish_with(&mut conn, lesson, practice::Score { correct: 3, almost: 0, total: 4 }, Some(&from_second)).unwrap();
        let finished: (Option<CourseId>, String) = t::learning_events::table.filter(t::learning_events::kind.eq("lesson_finished"))
            .select((t::learning_events::course_id, t::learning_events::source)).first(&mut conn).unwrap();
        assert_eq!(finished, (Some(second), "course".into()));
        // Events from the screens carry it too; an unknown source fails the batch.
        crate::learnlog::log_events(&mut conn, vec![crate::learnlog::EventIn { kind: "lesson_opened".into(), lesson_id: Some(lesson), course_id: Some(second), source: "course".into(), ..Default::default() }]).unwrap();
        assert!(crate::learnlog::log_events(&mut conn, vec![crate::learnlog::EventIn { kind: "lesson_opened".into(), source: "fridge".into(), ..Default::default() }]).is_err());
        // A check-in: started from the daily five, its verdict and slips logged so, with the check-in and its verdict.
        let study = starter_lesson(&mut conn);
        let checkin = crate::examiner::start(&mut conn, study.id, Some(&PracticeContext { source: "daily".into(), course_id: None })).unwrap();
        let verdict: crate::examiner::Assessment = serde_json::from_value(serde_json::json!({
            "result": "pass", "summary": "Good.", "mistakes": [{ "said": "et stor glas", "correct": "et stort glas", "grammar": ["adjective-t"] }]
        })).unwrap();
        crate::examiner::record(&mut conn, checkin, verdict).unwrap();
        type Logged = (Option<CourseId>, String, Option<CheckinId>, String, String);
        let logged: Vec<Logged> = t::attempts::table.filter(t::attempts::run.eq(checkin.to_string())).order(t::attempts::id)
            .select((t::attempts::course_id, t::attempts::source, t::attempts::checkin_id, t::attempts::details, t::attempts::prompt)).load(&mut conn).unwrap();
        assert_eq!(logged.len(), 2);
        for (course, source, id, details, prompt) in &logged {
            assert_eq!((course.is_some(), source.as_str(), *id), (true, "daily", Some(checkin)));
            let v = crate::learnlog::read_details(details).verdict.unwrap();
            assert_eq!(v.result, "pass");
            assert!(v.xp > 0 && prompt.starts_with("Check-in"));
        }
        // The slip's words are linked: glas, as written correctly.
        let slip: AttemptId = t::attempts::table.filter(t::attempts::exercise.eq("checkinMistake")).select(t::attempts::id).first(&mut conn).unwrap();
        let glas: Vec<(String, String)> = t::attempt_items::table.filter(t::attempt_items::attempt_id.eq(slip)).select((t::attempt_items::form, t::attempt_items::role)).load(&mut conn).unwrap();
        assert!(glas.contains(&("glas".into(), "in_sentence".into())), "{glas:?}");
    }

    #[test]
    fn a_logged_answer_rebuilds_its_card() {
        let mut conn = scratch_db("log-card");
        let (lesson, kaffe, glas, ..) = context_lesson(&mut conn);
        let options = vec!["tea".to_string(), "coffee".into(), "glass".into(), "water".into()];
        practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "recognise".into(), item_kind: "word".into(), item_id: kaffe.to_string(), expected: "coffee".into(), given: "glass".into(),
            lesson_id: Some(lesson), run: "r1".into(), prompt: "kaffe".into(), da: "kaffe".into(), options: options.clone(),
            card: Some(crate::learnlog::CardShown { key: format!("w{kaffe}-recognise"), position: Some(3), of_cards: Some(12), hint: "n-common".into(), ..Default::default() }),
            signals: Some(crate::learnlog::Signals { ms_first: Some(800), ms_answer: Some(2400), plays: 2, tries: 1, changed: true, first_pick: Some("tea".into()), ..Default::default() }),
            ..Default::default()
        }).unwrap();
        let page = crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter { kind: Some("answers".into()), ..Default::default() }, None, 10).unwrap();
        let row = &page.rows[0];
        let a = row.answer.as_ref().unwrap();
        assert_eq!((a.prompt.as_str(), a.direction.as_str(), a.card_key.as_str(), a.position), ("kaffe", "da→en", format!("w{kaffe}-recognise").as_str(), Some(3)));
        assert_eq!(a.options, options, "every option, in the order shown");
        assert_eq!((a.given.as_str(), a.expected.as_str(), a.correct), ("glass", "coffee", false));
        let shown = a.shown.as_ref().unwrap();
        assert_eq!((shown.of_cards, shown.hint.as_str()), (Some(12), "n-common"));
        assert_eq!((a.ms_first, a.ms_answer, a.plays, a.tries, a.first_pick.as_deref()), (Some(800), Some(2400), 2, 1, Some("tea")));
        assert_eq!((row.item.as_str(), row.lesson.as_str(), row.course_id.is_some()), ("kaffe", "Kaffe", true));
        // The words beside it: glas was picked by mistake, te was not.
        let target = a.links.iter().find(|l| l.role == "target").unwrap();
        assert_eq!((target.item_id.clone(), target.correct), (kaffe.to_string(), Some(false)));
        let picked = a.links.iter().find(|l| l.item_id == glas.to_string()).unwrap();
        assert_eq!((picked.role.as_str(), picked.correct), ("distractor", Some(false)));
        assert!(a.links.iter().any(|l| l.da == "te" && l.role == "distractor" && l.correct == Some(true)));
    }

    #[test]
    fn answers_link_every_known_word_in_them() {
        let mut conn = scratch_db("log-links");
        let (lesson, kaffe, glas, coffee, big) = context_lesson(&mut conn);
        // A dictation: glasset misspelt, kaffen right.
        answer(&mut conn, lesson, "dictation", "sentence", &coffee.to_string(), "Jeg hælder kaffen i glasset.", "Jeg hælder kaffen i glaset.", None);
        let links = |conn: &mut Conn| -> Vec<(String, String, String, i64, Option<i64>)> {
            let last: AttemptId = t::attempts::table.order(t::attempts::id.desc()).select(t::attempts::id).first(conn).unwrap();
            t::attempt_items::table.filter(t::attempt_items::attempt_id.eq(last)).order(t::attempt_items::position)
                .select((t::attempt_items::item_id, t::attempt_items::role, t::attempt_items::form, t::attempt_items::position, t::attempt_items::correct)).load(conn).unwrap()
        };
        let got = links(&mut conn);
        assert!(got.contains(&(coffee.to_string(), "target".into(), "Jeg hælder kaffen i glasset.".into(), 0, Some(0))), "{got:?}");
        assert!(got.contains(&(kaffe.to_string(), "in_sentence".into(), "kaffen".into(), 2, Some(1))), "an inflected form, by its annotation: {got:?}");
        assert!(got.contains(&(glas.to_string(), "in_sentence".into(), "glasset".into(), 4, Some(0))), "its own word-by-word result: {got:?}");
        // Without an annotation: the endings, then the glossary's forms.
        diesel::delete(t::sentence_tokens::table).execute(&mut conn).unwrap();
        answer(&mut conn, lesson, "wordOrder", "sentence", &coffee.to_string(), "Jeg hælder kaffen i glasset.", "Jeg hælder kaffen i glasset.", None);
        let got = links(&mut conn);
        assert!(got.contains(&(kaffe.to_string(), "in_sentence".into(), "kaffen".into(), 2, Some(1))), "kaffen by the sentence's endings: {got:?}");
        assert!(got.contains(&(glas.to_string(), "in_sentence".into(), "glasset".into(), 4, Some(1))), "glasset by the glossary (et glas, a doubled s): {got:?}");
        answer(&mut conn, lesson, "wordOrder", "sentence", &big.to_string(), "Det er et stort glas.", "Glas det er et stort.", None);
        let got = links(&mut conn);
        assert!(got.contains(&(glas.to_string(), "in_sentence".into(), "glas".into(), 4, Some(0))), "tiles graded word by word: glas out of place: {got:?}");
        // A word's history includes every appearance.
        let history = crate::contexts::item_contexts(&mut conn, &glas.to_string(), &crate::analytics::Filter::default()).unwrap();
        assert_eq!(history.appearances, 3);
    }

    #[test]
    fn token_annotations_are_checked_by_name() {
        let mut conn = scratch_db("log-tokens");
        let (_, _, glas, coffee, _) = context_lesson(&mut conn);
        let tokens = crate::grammar::tokens_of(&mut conn, coffee).unwrap();
        assert_eq!(tokens.len(), 5);
        assert_eq!((tokens[4].definiteness.as_str(), tokens[4].reason.as_str(), tokens[4].word_id), ("definite-suffix", "definite-singular", Some(glas)), "linked to the word by its lemma, without its et");
        assert_eq!(tokens[0].case, "subject");
        let tok = |v: serde_json::Value| -> Vec<crate::grammar::TokenIn> { serde_json::from_value(v).unwrap() };
        let check = |conn: &mut Conn, v: serde_json::Value| crate::grammar::validate_tokens(conn, "Drik kaffen!", &tok(v)).map(|_| ()).map_err(|e| e.to_string());
        let drik = serde_json::json!({ "form": "Drik", "lemma": "drikke", "pos": "verb", "tense": "imperative", "reason": "imperative" });
        let ok = serde_json::json!([drik, { "form": "kaffen", "lemma": "kaffe", "pos": "noun", "definiteness": "definite-suffix", "reason": "definite-singular" }]);
        assert!(check(&mut conn, ok).is_ok());
        let err = check(&mut conn, serde_json::json!([drik])).unwrap_err();
        assert!(err.contains("2 words") && err.contains("kaffen"), "{err}");
        let err = check(&mut conn, serde_json::json!([drik, { "form": "kaffe", "lemma": "kaffe", "pos": "noun" }])).unwrap_err();
        assert!(err.contains("Token 2") && err.contains("kaffen"), "{err}");
        let err = check(&mut conn, serde_json::json!([drik, { "form": "kaffen", "lemma": "kaffe", "pos": "noun", "case": "accusative" }])).unwrap_err();
        assert!(err.contains("Unknown case \"accusative\"") && err.contains("object"), "Danish nouns have no cases: {err}");
        let err = check(&mut conn, serde_json::json!([{ "form": "Drik", "lemma": "drikke", "pos": "verb", "tense": "aorist" }, { "form": "kaffen", "lemma": "kaffe", "pos": "noun" }])).unwrap_err();
        assert!(err.contains("aorist"), "{err}");
        let err = check(&mut conn, serde_json::json!([drik, { "form": "kaffen", "lemma": "kaffe", "pos": "noun", "definiteness": "definite-suffix", "reason": "front-article" }])).unwrap_err();
        assert!(err.contains("calls for definiteness \"definite-front\""), "{err}");
        let err = check(&mut conn, serde_json::json!([{ "form": "Drik", "lemma": "drikke", "pos": "verb", "tense": "present", "reason": "imperative" }, { "form": "kaffen", "lemma": "kaffe", "pos": "noun" }])).unwrap_err();
        assert!(err.contains("calls for tense \"imperative\""), "{err}");
        let err = check(&mut conn, serde_json::json!([drik, { "form": "kaffen", "lemma": "kaffe", "pos": "noun", "reason": "because" }])).unwrap_err();
        assert!(err.contains("Unknown reason \"because\""), "{err}");
        // A tool call with a bad token makes nothing.
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let refused = tutor::run_tool_for_test(&mut conn, "create_drill_lesson", serde_json::json!({
            "title": "Forkert", "format": "sentences", "complexity": "low", "course_id": basics,
            "sentences": [{ "da": "Jeg har en kat.", "en": "I have a cat.", "tokens": [{ "form": "Jeg", "lemma": "jeg", "pos": "pron" }, { "form": "har", "lemma": "have", "pos": "verb" }, { "form": "en", "lemma": "en", "pos": "art" }, { "form": "kat", "lemma": "en kat", "pos": "noun", "gender": "masc" }] }]
        }));
        assert!(refused["error"].as_str().unwrap().contains("masc"), "{refused}");
        // Existing sentences: annotated only when asked, through the tutor.
        let before = crate::grammar::unannotated_count(&mut conn).unwrap();
        assert!(before > 0);
        let listed = tutor::run_tool_for_test(&mut conn, "list_untagged", serde_json::json!({ "missing": "tokens", "limit": 1 }));
        assert_eq!(listed["total"], before);
        let id = listed["items"][0]["id"].as_str().unwrap().to_string();
        let da = listed["items"][0]["da"].as_str().unwrap().to_string();
        let tokens: Vec<serde_json::Value> = crate::grammar::words_of(&da).iter().map(|(w, l)| serde_json::json!({ "form": w, "lemma": l, "pos": "noun" })).collect();
        let done = tutor::run_tool_for_test(&mut conn, "annotate_sentences", serde_json::json!({ "items": [{ "id": id, "tokens": tokens }] }));
        assert_eq!(done["still_unannotated"], before - 1, "{done}");
        assert!(done["_action"]["label"].as_str().unwrap().starts_with("Annotated 1 sentence"));
    }

    #[test]
    fn topics_are_checked_and_tagged_when_asked() {
        let mut conn = scratch_db("log-topics");
        let (lesson, kaffe, ..) = context_lesson(&mut conn);
        let links = crate::topics::all_links(&mut conn).unwrap();
        assert_eq!(links[&lesson.to_string()], vec![crate::topics::id_of("food.drinks")], "a lesson takes the topics its tool was given");
        assert_eq!(links[&kaffe.to_string()], vec![crate::topics::id_of("food.drinks")]);
        let basics = db::require_course_id(&mut conn, 1).unwrap();
        let refused = tutor::run_tool_for_test(&mut conn, "create_drill_lesson", serde_json::json!({
            "title": "Øl", "format": "words", "complexity": "low", "course_id": basics, "topics": ["food.beer"],
            "words": [{ "da": "en øl", "en": "beer", "pos": "n-common" }]
        }));
        assert!(refused["error"].as_str().unwrap().contains("food.beer"), "{refused}");
        assert_eq!(t::lessons::table.filter(t::lessons::title.eq("Øl")).count().get_result::<i64>(&mut conn).unwrap(), 0, "nothing made");
        let refused = tutor::run_tool_for_test(&mut conn, "create_drill_lesson", serde_json::json!({
            "title": "Øl", "format": "words", "complexity": "low", "course_id": basics,
            "words": [{ "da": "en øl", "en": "beer", "pos": "n-common", "topics": ["drinks"] }]
        }));
        assert!(refused["error"].as_str().unwrap().contains("drinks"), "{refused}");
        // Existing content: tagged when asked.
        let before = crate::topics::untagged_count(&mut conn).unwrap();
        let listed = tutor::run_tool_for_test(&mut conn, "list_untagged", serde_json::json!({ "missing": "topics", "limit": 2 }));
        assert_eq!(listed["total"], before);
        let ids: Vec<String> = listed["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect();
        let done = tutor::run_tool_for_test(&mut conn, "tag_topics", serde_json::json!({ "items": ids.iter().map(|id| serde_json::json!({ "id": id, "topics": ["time"] })).collect::<Vec<_>>() }));
        assert_eq!(done["still_untagged"], before - 2, "{done}");
        assert!(done["_action"]["label"].as_str().unwrap().starts_with("Tagged the topics"));
        let bad = tutor::run_tool_for_test(&mut conn, "tag_topics", serde_json::json!({ "items": [{ "id": ids[0], "topics": ["time.never"] }] }));
        assert!(bad["error"].as_str().unwrap().contains("time.never"));
    }

    #[test]
    fn context_stats_on_a_fixed_history() {
        use crate::analytics::Filter;
        use crate::contexts::{stats, Subject};
        let mut conn = scratch_db("log-context-stats");
        let (lesson, _, glas, coffee, big) = context_lesson(&mut conn);
        let second = db::require_course_id(&mut conn, 2).unwrap();
        let from = |source: &str, course: Option<CourseId>| Some(crate::learnlog::PracticeContext { source: source.into(), course_id: course });
        // glas on its own: right twice (from its course, then the second).
        answer(&mut conn, lesson, "type", "word", &glas.to_string(), "et glas", "et glas", from("course", None));
        answer(&mut conn, lesson, "type", "word", &glas.to_string(), "et glas", "et glas", from("course", Some(second)));
        // With et: right; with the definite ending: wrong.
        answer(&mut conn, lesson, "wordOrder", "sentence", &big.to_string(), "Det er et stort glas.", "Det er et stort glas.", from("daily", None));
        answer(&mut conn, lesson, "dictation", "sentence", &coffee.to_string(), "Jeg hælder kaffen i glasset.", "Jeg hælder kaffen i glaset.", from("daily", None));
        let all = Filter::default();
        let item = Subject { item_id: Some(glas.to_string()), ..Default::default() };
        let groups = |conn: &mut Conn, by: &str| -> Vec<(String, i64, i64)> {
            stats(conn, &item, by, &all).unwrap().groups.into_iter().map(|g| (g.key, g.attempts, g.correct)).collect()
        };
        let mut by_reason = groups(&mut conn, "reason");
        by_reason.sort();
        assert_eq!(by_reason, [("alone".into(), 2, 2), ("definite-singular".into(), 1, 0), ("neuter".into(), 1, 1)], "glas: right with et, wrong with its ending");
        let mut by_definiteness = groups(&mut conn, "definiteness");
        by_definiteness.sort();
        assert_eq!(by_definiteness, [("alone".into(), 2, 2), ("definite-suffix".into(), 1, 0), ("indefinite".into(), 1, 1)]);
        let mut by_role = groups(&mut conn, "role");
        by_role.sort();
        assert_eq!(by_role, [("in_sentence".into(), 2, 1), ("target".into(), 2, 2)]);
        let by_course = groups(&mut conn, "course");
        assert!(by_course.contains(&(second.to_string(), 1, 1)), "{by_course:?}");
        let mut by_form = groups(&mut conn, "form");
        by_form.sort();
        assert_eq!(by_form, [("et glas".into(), 2, 2), ("glas".into(), 1, 1), ("glasset".into(), 1, 0)], "on its own card it is shown with et");
        let by_source = groups(&mut conn, "source");
        assert!(by_source.contains(&("daily".into(), 2, 1)) && by_source.contains(&("course".into(), 2, 2)), "{by_source:?}");
        let by_topic = groups(&mut conn, "topic");
        assert!(by_topic.contains(&("food.drinks".into(), 4, 3)), "{by_topic:?}");
        assert!(stats(&mut conn, &item, "case", &all).unwrap_err().to_string().contains("case"), "no grouping by case in Danish");
        // A topic's own numbers, and a structure's words.
        let topic = stats(&mut conn, &Subject { topic: Some("food".into()), ..Default::default() }, "exercise", &all).unwrap();
        assert_eq!(topic.attempts, 4, "a domain takes its topics' answers in");
        let detail = crate::contexts::topic_detail(&mut conn, "food.drinks", &all).unwrap();
        let glass = detail.words.iter().find(|w| w.word_id == glas).unwrap();
        assert_eq!((glass.attempts, glass.correct), (4, 3));
        let page = crate::contexts::topics_page(&mut conn, &all).unwrap();
        assert!(page.domains.iter().any(|d| d.domain.topic.key == "food" && d.topics.iter().any(|t| t.topic.key == "food.drinks" && t.attempts == 4)));
        let words = crate::contexts::structure_words(&mut conn, "definite-singular", &all).unwrap();
        let glass = words.iter().find(|w| w.word_id == glas).unwrap();
        assert_eq!((glass.attempts, glass.correct, glass.forms.clone()), (1, 0, vec!["glasset".to_string()]));
        let words = crate::contexts::structure_words(&mut conn, "adjective-t", &all).unwrap();
        assert!(words.iter().all(|w| w.word_id != glas), "stort is the adjective-t there, not glas: {words:?}");
        // Drilling down: the answers behind a group.
        let ids = crate::contexts::attempt_ids(&mut conn, &item, "reason", "definite-singular", &all).unwrap();
        assert_eq!(ids.len(), 1);
        let drilled = crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter { item_id: Some(glas.to_string()), context: Some(crate::rawlog::ContextRef { group_by: "reason".into(), key: "definite-singular".into() }), ..Default::default() }, None, 50).unwrap();
        assert_eq!(drilled.rows.len(), 1);
        assert_eq!(drilled.rows[0].answer.as_ref().unwrap().given, "Jeg hælder kaffen i glaset.");
        // The word page carries every breakdown; the tutor's tool the same numbers.
        let detail = crate::analytics::met_word(&mut conn, glas).unwrap();
        assert_eq!(detail.contexts.appearances, 4);
        let tool = tutor::run_tool_for_test(&mut conn, "get_context_stats", serde_json::json!({ "item_id": glas.to_string(), "group_by": "reason", "period": "all" }));
        let definite = tool["groups"].as_array().unwrap().iter().find(|g| g["key"] == "definite-singular").unwrap();
        assert_eq!((definite["answers"].as_i64(), definite["accuracy_pct"].as_i64()), (Some(1), Some(0)), "{tool}");
    }

    #[test]
    fn the_raw_log_filters_and_pages() {
        use crate::rawlog::{page, RawFilter};
        let mut conn = scratch_db("log-raw");
        let (lesson, kaffe, glas, coffee, _) = context_lesson(&mut conn);
        let daily = Some(crate::learnlog::PracticeContext { source: "daily".into(), course_id: None });
        for i in 0..23 {
            let given = if i % 3 == 0 { "kaffen" } else { "kaffe" };
            answer(&mut conn, lesson, "type", "word", &kaffe.to_string(), "kaffe", given, if i % 2 == 0 { daily.clone() } else { None });
        }
        answer(&mut conn, lesson, "dictation", "sentence", &coffee.to_string(), "Jeg hælder kaffen i glasset.", "Jeg hælder kaffen i glaset.", None);
        crate::learnlog::log_events(&mut conn, (0..7).map(|_| crate::learnlog::EventIn { kind: "audio_played".into(), item_kind: "word".into(), item_id: kaffe.to_string(), lesson_id: Some(lesson), ..Default::default() }).collect()).unwrap();
        // Pages of 5 cover everything once, newest first, however many share a moment.
        let mut seen: Vec<String> = Vec::new();
        let mut cursor = None;
        loop {
            let p = page(&mut conn, &RawFilter::default(), cursor.as_ref(), 5).unwrap();
            assert!(p.rows.len() <= 5);
            assert!(p.rows.windows(2).all(|w| w[0].at >= w[1].at));
            seen.extend(p.rows.iter().map(|r| r.id.clone()));
            match p.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        let total = t::attempts::table.count().get_result::<i64>(&mut conn).unwrap() + t::learning_events::table.count().get_result::<i64>(&mut conn).unwrap();
        assert_eq!(i64::try_from(seen.len()).unwrap(), total);
        assert_eq!(seen.iter().collect::<std::collections::HashSet<_>>().len(), seen.len(), "no row twice");
        let count = |conn: &mut Conn, f: RawFilter| crate::rawlog::all_rows(conn, &f).unwrap().len();
        assert_eq!(count(&mut conn, RawFilter { correct: Some(false), ..Default::default() }), 9, "8 kaffen, 1 dictation");
        assert_eq!(count(&mut conn, RawFilter { exercise: Some("dictation".into()), ..Default::default() }), 1);
        assert_eq!(count(&mut conn, RawFilter { mistake: Some("ending".into()), ..Default::default() }), 8);
        assert_eq!(count(&mut conn, RawFilter { source: Some("daily".into()), kind: Some("answers".into()), ..Default::default() }), 12);
        assert_eq!(count(&mut conn, RawFilter { kind: Some("events".into()), ..Default::default() }), t::learning_events::table.count().get_result::<i64>(&mut conn).unwrap().try_into().unwrap_or(0usize));
        // An item: its own answers, and the dictation it appeared in.
        assert_eq!(count(&mut conn, RawFilter { item_id: Some(glas.to_string()), kind: Some("answers".into()), ..Default::default() }), 1);
        assert_eq!(count(&mut conn, RawFilter { item_id: Some(kaffe.to_string()), role: Some("in_sentence".into()), ..Default::default() }), 1);
        assert_eq!(count(&mut conn, RawFilter { search: Some("glasset".into()), kind: Some("answers".into()), ..Default::default() }), 1);
        assert_eq!(count(&mut conn, RawFilter { topic: Some("food".into()), kind: Some("answers".into()), ..Default::default() }), 24);
        assert_eq!(count(&mut conn, RawFilter { grammar: Some("definite-singular".into()), ..Default::default() }), 1, "by the sentence's annotation");
        let other = db::require_course_id(&mut conn, 3).unwrap();
        assert_eq!(count(&mut conn, RawFilter { course_id: Some(other), ..Default::default() }), 0);
        assert_eq!(count(&mut conn, RawFilter { lesson_id: Some(lesson), kind: Some("answers".into()), ..Default::default() }), 24);
        let today = db::today(&mut conn);
        assert_eq!(count(&mut conn, RawFilter { to_day: Some(today - 1), ..Default::default() }), 0);
        assert!(page(&mut conn, &RawFilter { source: Some("fridge".into()), ..Default::default() }, None, 5).is_err());
        // The tutor's view pages the same way.
        let first = tutor::run_tool_for_test(&mut conn, "get_raw_log", serde_json::json!({ "limit": 3, "kind": "answers", "correct": false }));
        assert_eq!(first["rows"].as_array().unwrap().len(), 3);
        let next = tutor::run_tool_for_test(&mut conn, "get_raw_log", serde_json::json!({ "limit": 10, "kind": "answers", "correct": false, "next": first["next"] }));
        assert_eq!(next["rows"].as_array().unwrap().len(), 6, "{next}");
        let dictation = tutor::run_tool_for_test(&mut conn, "get_raw_log", serde_json::json!({ "exercise": "dictation" }));
        let words: Vec<&str> = dictation["rows"][0]["words"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
        assert!(words.contains(&"et glas=glasset@in_sentence:wrong (definite-singular)"), "{words:?}");
    }

    #[test]
    fn the_raw_log_exports_as_csv_and_json() {
        let mut conn = scratch_db("log-export");
        let (lesson, kaffe, _, coffee, _) = context_lesson(&mut conn);
        answer(&mut conn, lesson, "type", "word", &kaffe.to_string(), "kaffe", "kaffen", None);
        practice::submit_answer_in(&mut conn, practice::SubmitAnswer {
            kind: "recognise".into(), item_kind: "word".into(), item_id: kaffe.to_string(), expected: "coffee".into(), given: "coffee".into(),
            lesson_id: Some(lesson), prompt: "kaffe, \"the drink\"".into(), options: vec!["tea".into(), "coffee".into()], ..Default::default()
        }).unwrap();
        answer(&mut conn, lesson, "dictation", "sentence", &coffee.to_string(), "Jeg hælder kaffen i glasset.", "Jeg hælder kaffen i glaset.", None);
        crate::learnlog::log_events(&mut conn, vec![crate::learnlog::EventIn { kind: "talk_opened".into(), item_kind: "word".into(), item_id: kaffe.to_string(), ..Default::default() }]).unwrap();
        let rows = crate::rawlog::all_rows(&mut conn, &crate::rawlog::RawFilter::default()).unwrap();
        let csv = crate::rawlog::export_text(&rows, "csv", 60).unwrap();
        // A small CSV reader: fields, quotes doubled inside quoted ones.
        let parse = |text: &str| -> Vec<Vec<String>> {
            let (mut out, mut row, mut field, mut quoted, mut chars) = (vec![], vec![], String::new(), false, text.chars().peekable());
            while let Some(c) = chars.next() {
                match (c, quoted) {
                    ('"', true) if chars.peek() == Some(&'"') => { field.push('"'); chars.next(); }
                    ('"', _) => quoted = !quoted,
                    (',', false) => row.push(std::mem::take(&mut field)),
                    ('\n', false) => { row.push(std::mem::take(&mut field)); out.push(std::mem::take(&mut row)); }
                    _ => field.push(c),
                }
            }
            out
        };
        let table = parse(&csv);
        assert_eq!(table[0], crate::rawlog::CSV_COLUMNS);
        assert_eq!(table.len(), rows.len() + 1);
        assert!(table.iter().all(|r| r.len() == crate::rawlog::CSV_COLUMNS.len()), "every row has every column");
        let col = |name: &str| crate::rawlog::CSV_COLUMNS.iter().position(|c| *c == name).unwrap();
        let choice = table.iter().find(|r| r[col("exercise")] == "recognise").unwrap();
        assert_eq!((choice[col("prompt")].as_str(), choice[col("options")].as_str(), choice[col("direction")].as_str()), ("kaffe, \"the drink\"", "tea | coffee", "da→en"));
        let dictation = table.iter().find(|r| r[col("exercise")] == "dictation").unwrap();
        assert!(dictation[col("linked_words")].contains("et glas=glasset@in_sentence#4:wrong:definite-singular"), "{}", dictation[col("linked_words")]);
        assert_eq!(table.iter().filter(|r| r[col("kind")] == "event").count(), rows.iter().filter(|r| r.kind == "event").count());
        let json = crate::rawlog::export_text(&rows, "json", 0).unwrap();
        let back: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), rows.len());
        assert!(back.iter().any(|r| r["answer"]["links"].as_array().is_some_and(|l| !l.is_empty())));
        assert!(crate::rawlog::export_text(&rows, "xlsx", 0).is_err());
    }

    /// Not a test: a realistic learning history (a few hundred answers over
    /// eleven weeks, with every kind of mistake, classified as the app
    /// classifies them), and what the insights pages read from it, written
    /// as JSON for the preview harness. `PREVIEW_OUT=dir cargo test preview_history -- --ignored`.
    #[test]
    #[ignore]
    fn preview_history() {
        use rand::{Rng, SeedableRng};
        use crate::analytics::{self as a, Filter};
        let out = std::path::PathBuf::from(std::env::var("PREVIEW_OUT").expect("PREVIEW_OUT"));
        std::fs::create_dir_all(&out).unwrap();
        let write = |name: &str, value: &dyn erased_preview::Shape| std::fs::write(out.join(format!("{name}.json")), value.json()).unwrap();
        mod erased_preview {
            pub trait Shape { fn json(&self) -> String; }
            impl<T: serde::Serialize> Shape for T { fn json(&self) -> String { serde_json::to_string(self).unwrap() } }
        }
        let mut conn = scratch_db("preview-history");
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let today = db::today(&mut conn);
        let device = crate::sync::device_id(&mut conn).unwrap();
        let courses: Vec<CourseId> = (1..=4).map(|o| db::require_course_id(&mut conn, o).unwrap()).collect();
        let mut words: Vec<(WordId, String, String, String, LessonId)> = Vec::new();
        for c in &courses {
            let mut some: Vec<(WordId, String, String, String, LessonId)> = t::course_words::table.filter(t::course_words::course_id.eq(*c))
                .select((t::course_words::id, t::course_words::da, t::course_words::en, t::course_words::pos, t::course_words::lesson_id))
                .limit(18).load(&mut conn).unwrap();
            words.append(&mut some);
        }
        let sentences: Vec<(SentenceId, String, String, LessonId)> = t::course_sentences::table.filter(t::course_sentences::course_id.eq_any(&courses))
            .select((t::course_sentences::id, t::course_sentences::da, t::course_sentences::en, t::course_sentences::lesson_id))
            .limit(30).load(&mut conn).unwrap();
        // Grammar tags, as the tutor would give them.
        let tag = |keys: &[&str]| crate::grammar::validate(&keys.iter().map(|k| k.to_string()).collect::<Vec<_>>()).unwrap();
        for (i, (id, da, _, pos, _)) in words.iter().enumerate() {
            let keys: Vec<&str> = match pos.as_str() {
                "n-neuter" => if i % 3 == 0 { vec!["neuter", "plural-e"] } else { vec!["neuter"] },
                p if p.starts_with('n') => if i % 3 == 0 { vec!["common-gender", "plural-er"] } else { vec!["common-gender"] },
                "v" => if da.contains(" sig") { vec!["reflexive-verbs"] } else if i % 2 == 0 { vec!["infinitive", "present"] } else { vec!["past-te"] },
                "adj" => vec!["adjective-t", "adjective-e"],
                "num" => vec!["numbers"],
                "prep" => vec!["i-place"],
                _ => vec!["politeness"],
            };
            crate::grammar::set_word(&mut conn, *id, &tag(&keys)).unwrap();
        }
        for (id, da, _, _) in &sentences {
            let low = format!(" {} ", da.to_lowercase());
            let mut keys = vec![];
            if low.contains(" i ") { keys.push("i-place"); }
            if low.contains(" på ") { keys.push("paa-place"); }
            if low.contains(" til ") { keys.push("til"); }
            if low.contains(" ikke ") { keys.push("ikke-main"); }
            if low.contains("?") { keys.push("yes-no-questions"); }
            if low.contains("en ") || low.contains("et ") { keys.push("definite-singular"); }
            if low.contains(" sig ") { keys.push("reflexive-verbs"); }
            if keys.is_empty() { keys.push("present"); }
            crate::grammar::set_sentence(&mut conn, *id, &tag(&keys)).unwrap();
        }
        // Topics, as the tutor would give them: a course's lessons share a
        // domain; words and sentences take their lesson's, or one by what
        // they say.
        let titles: Vec<String> = courses.iter().map(|c| t::courses::table.find(*c).select(t::courses::title_en).first::<String>(&mut conn).unwrap().to_lowercase()).collect();
        let course_topics: Vec<[&str; 2]> = titles.iter().map(|title| {
            if title.contains("famil") { ["family.relatives", "family.celebrations"] }
            else if title.contains("cloth") || title.contains("shop") { ["shopping.clothes", "shopping.prices"] }
            else if title.contains("meet") || title.contains("hello") { ["communication.greetings", "people.introductions"] }
            else if title.contains("caf") || title.contains("food") || title.contains("restaur") { ["food.drinks", "food.restaurant"] }
            else if title.contains("travel") || title.contains("train") || title.contains("city") { ["travel.transport", "city.places"] }
            else { ["time.days-dates", "time.routine"] }
        }).collect();
        let topic = |keys: &[&str]| crate::topics::validate(&keys.iter().map(|k| k.to_string()).collect::<Vec<_>>()).unwrap();
        for (c, course) in courses.iter().enumerate() {
            for lesson in crate::lessons::for_course(&mut conn, *course).unwrap() {
                crate::topics::set_lesson(&mut conn, lesson.id, &topic(&course_topics[c])).unwrap();
            }
        }
        let topic_by_text = |da: &str, fallback: &'static str| -> &'static str {
            let low = da.to_lowercase();
            for (needle, key) in [("kaffe", "food.drinks"), ("te ", "food.drinks"), ("vand", "food.drinks"), ("øl", "food.drinks"), ("brød", "food.meals"), ("middag", "food.meals"), ("hus", "home.rooms"), ("arbejd", "work.office"), ("mor", "family.relatives"), ("far", "family.relatives"), ("tog", "travel.transport"), ("bus", "travel.transport"), ("butik", "shopping.shops"), ("kat", "nature.animals"), ("hund", "nature.animals"), ("vejr", "weather.conditions"), ("regn", "weather.conditions")] {
                if low.contains(needle) { return key; }
            }
            fallback
        };
        let course_of_lesson = |conn: &mut Conn, lesson: LessonId| -> usize {
            let c: CourseId = t::course_lessons::table.filter(t::course_lessons::lesson_id.eq(lesson)).select(t::course_lessons::course_id).first(conn).unwrap();
            courses.iter().position(|x| *x == c).unwrap_or(0)
        };
        for (id, da, _, _, lesson) in &words {
            let c = course_of_lesson(&mut conn, *lesson);
            crate::topics::set_word(&mut conn, *id, &topic(&[topic_by_text(da, course_topics[c][0])])).unwrap();
        }
        // The learner's nouns, bare: a form is definite when it is one of
        // them with -en / -et (doubled consonant or dropped e allowed).
        let nouns: std::collections::HashSet<String> = t::words::table.filter(t::words::pos.like("n%")).select(t::words::da).load::<String>(&mut conn).unwrap()
            .iter().map(|d| crate::singalong::bare(d).to_lowercase()).collect();
        let definite_of = |w: &str| -> Option<&'static str> {
            nouns.iter().find_map(|n| {
                let stem = n.strip_suffix('e').unwrap_or(n);
                let last = n.chars().last().map(String::from).unwrap_or_default();
                for ending in ["en", "et", "n", "t"] {
                    if w == format!("{n}{ending}") || w == format!("{stem}{ending}") || w == format!("{n}{last}{ending}") { return Some("definite-singular"); }
                }
                for ending in ["erne", "ene", "ne"] {
                    if w == format!("{n}{ending}") || w == format!("{stem}{ending}") || w == format!("{n}{last}{ending}") { return Some("definite-plural"); }
                }
                None
            })
        };
        for (id, da, _, lesson) in &sentences {
            let c = course_of_lesson(&mut conn, *lesson);
            crate::topics::set_sentence(&mut conn, *id, &topic(&[topic_by_text(da, course_topics[c][1])])).unwrap();
            // Word by word, as the tutor would annotate it (roughly, for the preview).
            let ws = crate::grammar::words_of(da);
            let tokens: Vec<crate::grammar::TokenIn> = ws.iter().enumerate().map(|(i, (form, low))| {
                let preps = ["i", "på", "til", "fra", "med", "ved", "under", "over", "foran", "bag", "uden", "om", "for", "efter"];
                let objects = ["mig", "dig", "ham", "hende", "os", "jer", "dem", "sig"];
                let subjects = ["jeg", "du", "han", "hun", "vi", "man"];
                let possessive = ["min", "mit", "mine", "din", "dit", "dine", "vores", "jeres", "deres", "hans", "hendes", "sin", "sit", "sine"];
                let prev = if i > 0 { ws[i - 1].1.as_str() } else { "" };
                let last = i + 1 == ws.len();
                let w = low.as_str();
                let chars = w.chars().count();
                // (part of speech, case, definiteness, adjective ending, tense, reason)
                let (pos, case, definite, inflection, tense, reason) = match w {
                    _ if objects.contains(&w) => ("pron", "object", "", "", "", if preps.contains(&prev) { "after-preposition" } else { "object-pronouns" }),
                    _ if subjects.contains(&w) => ("pron", "subject", "", "", "", "personal-pronouns"),
                    "en" | "et" if !last => ("art", "", "indefinite", "", "", if w == "et" { "neuter" } else { "common-gender" }),
                    "den" | "det" | "de" if !last && ws[i + 1].1.ends_with('e') && i + 2 < ws.len() => ("art", "", "definite-front", "", "", "front-article"),
                    "ikke" => ("adv", "", "", "", "", "ikke-main"),
                    _ if preps.contains(&w) => ("prep", "", "", "", "", ""),
                    "er" | "har" | "kan" | "vil" | "skal" => ("verb", "", "", "", "present", if i == 0 { "yes-no-questions" } else if i == 1 { "v2" } else { "inversion" }),
                    "var" | "havde" => ("verb", "", "", "", "past", if i == 1 { "v2" } else { "" }),
                    _ if matches!(prev, "den" | "det" | "de") && w.ends_with('e') && !last => ("adj", "", "definite-front", "e", "", "adjective-e"),
                    _ if i > 1 && matches!(ws[i - 2].1.as_str(), "den" | "det" | "de") && prev.ends_with('e') => ("noun", "", "definite-front", "", "", "front-article"),
                    _ if nouns.contains(w) && matches!(prev, "en" | "et") => ("noun", "", "indefinite", "", "", if prev == "et" { "neuter" } else { "common-gender" }),
                    _ if nouns.contains(w) && i > 1 && matches!(ws[i - 2].1.as_str(), "en" | "et") => ("noun", "", "indefinite", "", "", if ws[i - 2].1 == "et" { "neuter" } else { "common-gender" }),
                    _ if nouns.contains(w) && possessive.contains(&prev) => ("noun", "", "", "", "", "possessives"),
                    _ if nouns.contains(w) => ("noun", "", "indefinite", "", "", ""),
                    _ if prev == "et" && !last && w.ends_with('t') && nouns.contains(&ws[i + 1].1) => ("adj", "", "indefinite", "t", "", "adjective-t"),
                    _ if prev == "en" && !last && nouns.contains(&ws[i + 1].1) => ("adj", "", "indefinite", "base", "", "agreement"),
                    _ if chars > 3 && definite_of(w).is_some() => ("noun", "", "definite-suffix", "", "", definite_of(w).unwrap_or_default()),
                    _ if prev == "et" => ("noun", "", "indefinite", "", "", "neuter"),
                    _ if prev == "en" => ("noun", "", "indefinite", "", "", "common-gender"),
                    _ if i == 1 && w.ends_with('r') => ("verb", "", "", "", "present", "v2"),
                    "min" | "mit" | "mine" | "din" | "dit" | "dine" | "vores" | "jeres" | "deres" | "hans" | "hendes" => ("pron", "", "", "", "", "possessives"),
                    "sin" | "sit" | "sine" => ("pron", "", "", "", "", "sin-sit-sine"),
                    _ => ("noun", "", "", "", "", ""),
                };
                crate::grammar::TokenIn {
                    form: form.clone(), lemma: low.clone(), pos: pos.into(), case: case.into(), definiteness: definite.into(),
                    inflection: inflection.into(), tense: tense.into(), reason: reason.into(), ..Default::default()
                }
            }).collect();
            let checked = crate::grammar::validate_tokens(&mut conn, da, &tokens).unwrap();
            crate::grammar::set_tokens(&mut conn, *id, &checked).unwrap();
        }
        // How hard each thing is, and how much better it gets over the weeks.
        let hardness: Vec<f64> = words.iter().map(|_| rng.random_range(0.04..0.55)).collect();
        let fold = |w: &str| w.replace('æ', "ae").replace('ø', "oe").replace('å', "aa");
        let ending = |w: &str, r: u32| format!("{}{}", w.strip_suffix('e').unwrap_or(w), ["en", "et", "er", "ene", "t"][usize::try_from(r % 5).unwrap()]);
        let typo = |w: &str| { let mut c: Vec<char> = w.chars().collect(); if c.len() > 3 { c.swap(1, 2); } c.into_iter().collect::<String>() };
        let sound = |w: &str| { for (a, b) in [("æ", "e"), ("ø", "y"), ("y", "i"), ("å", "o"), ("nd", "n"), ("u", "o"), ("r", "h")] { if w.contains(a) { return Some(w.replacen(a, b, 1)); } } None };
        let position = std::cell::Cell::new(0i64);
        let put = |conn: &mut Conn, day: i64, ms: i64, kind: &str, id: &str, lesson: LessonId, exercise: &str, run: &str, given: &str, expected: &str, correct: bool, diff: &[crate::dictation::DiffWord], options: &[String], prompt: &str, da: &str| {
            let mistake = crate::mistakes::classify(&crate::mistakes::Graded { exercise, expected, given, correct, diff, options, prompt, da, endings: &[] });
            let signals = crate::learnlog::Signals { ms_first: Some(ms / 3), ms_answer: Some(ms), plays: i64::from(ms > 6000), reveals: i64::from(ms > 8000), tries: i64::from(ms % 7 == 0), ..Default::default() };
            // Where it was practised: the course page most days, the daily
            // five or Other lessons on others.
            let source = ["course", "daily", "course", "other", "daily"][usize::try_from(day.rem_euclid(5)).unwrap()];
            let context = crate::learnlog::resolve(conn, Some(&crate::learnlog::PracticeContext { source: source.into(), course_id: None }), Some(lesson), "").unwrap();
            position.set(position.get() + 1);
            let card = crate::learnlog::CardShown { key: format!("{}-{exercise}", &id[..id.len().min(12)]), position: Some(position.get()), of_cards: Some(14), hint: String::new(), tokens: vec![], image: None, meaning: String::new() };
            let shown = if prompt.is_empty() { expected } else { prompt };
            let at_id = crate::learnlog::write_attempt(conn, &crate::learnlog::NewAttempt {
                item_kind: kind, item_id: id, lesson_id: Some(lesson), exercise, run, correct, given, expected, mistake: &mistake, signals: &signals, options, grammar: vec![], note: String::new(),
                context: &context, prompt: shown, card: Some(&card), da, checkin_id: None, verdict: None,
            }).unwrap();
            let hour = [8, 8, 12, 13, 19, 20, 21, 21, 22][usize::try_from(day.unsigned_abs() % 9).unwrap()];
            diesel::update(t::attempts::table.find(at_id)).set((t::attempts::day.eq(day), t::attempts::at.eq(day * 86_400_000 + hour * 3_600_000 + ms))).execute(conn).unwrap();
        };
        let pairs = crate::sounds::built_in();
        for back in (0..77).rev() {
            let day = today - back;
            if rng.random_bool(0.28) { continue; }
            let progress = 1.0 - crate::convert::real(back) / 110.0;
            let lesson = words[rng.random_range(0..words.len())].4;
            let run = format!("{lesson}-{day}");
            let cards = rng.random_range(8..16);
            position.set(0);
            let mut answered = 0;
            let mut right = 0;
            for _ in 0..cards {
                let n = rng.random_range(0..words.len());
                let (id, da, en, _, lesson) = &words[n];
                let wrong = rng.random_bool((hardness[n] * (1.3 - progress)).clamp(0.02, 0.9));
                let ms = rng.random_range(1800..9000);
                let roll: u32 = rng.random_range(0..10);
                let (exercise, expected, given) = if roll < 3 {
                    let other = &words[rng.random_range(0..words.len())].2;
                    ("recognise", en.clone(), if wrong { other.clone() } else { en.clone() })
                } else {
                    let kind = if roll < 7 { "type" } else { "produce" };
                    let given = if !wrong {
                        if kind == "type" && rng.random_bool(0.25) { fold(da) } else { da.clone() }
                    } else {
                        match rng.random_range(0..6) {
                            0 | 1 => ending(da, rng.random()),
                            2 => typo(da),
                            3 => sound(da).unwrap_or_else(|| typo(da)),
                            4 if kind == "type" => format!("{}x", fold(da)),
                            _ => words[rng.random_range(0..words.len())].1.clone(),
                        }
                    };
                    (kind, da.clone(), given)
                };
                let correct = if exercise == "recognise" { given == expected } else { crate::practice::normalize(&given) == crate::practice::normalize(&expected) };
                // A choice shows four options, the answer among three others.
                let options: Vec<String> = if exercise == "type" { vec![] } else {
                    let mut o: Vec<String> = (0..3).map(|k| { let w = &words[(n + 1 + k * 7) % words.len()]; if exercise == "recognise" { w.2.clone() } else { w.1.clone() } }).collect();
                    if !o.contains(&given) && given != expected { o[0] = given.clone(); }
                    o.insert(usize::try_from(day.rem_euclid(4)).unwrap(), expected.clone());
                    o
                };
                let prompt = if exercise == "recognise" { da.clone() } else { en.clone() };
                put(&mut conn, day, ms, "word", &id.to_string(), *lesson, exercise, &run, &given, &expected, correct, &[], &options, &prompt, "");
                answered += 1;
                right += i64::from(correct);
            }
            for _ in 0..rng.random_range(1..4) {
                let (id, da, _, lesson) = &sentences[rng.random_range(0..sentences.len())];
                let ws: Vec<&str> = da.split_whitespace().collect();
                let wrong = rng.random_bool((0.45 * (1.25 - progress)).clamp(0.05, 0.8));
                let given = if !wrong { da.clone() } else if ws.len() > 2 && rng.random_bool(0.5) {
                    let mut v = ws.clone(); v.swap(0, 1); v.join(" ")
                } else {
                    let mut v: Vec<String> = ws.iter().map(|w| w.to_string()).collect();
                    let k = v.len() / 2; v[k] = ending(&v[k], rng.random()); v.join(" ")
                };
                if rng.random_bool(0.5) {
                    let checked = crate::dictation::check(da, &given);
                    put(&mut conn, day, 9000, "sentence", &id.to_string(), *lesson, "dictation", &run, &given, da, checked.correct, &checked.words, &[], "", "");
                    right += i64::from(checked.correct);
                } else {
                    let correct = crate::tiles::fold(da) == crate::tiles::fold(&given);
                    put(&mut conn, day, 7000, "sentence", &id.to_string(), *lesson, "wordOrder", &run, &given, da, correct, &[], &[], "", "");
                    right += i64::from(correct);
                }
                answered += 1;
            }
            for _ in 0..rng.random_range(0..3) {
                let pair = &pairs[rng.random_range(0..pairs.len())];
                let hard = pair.label.contains('æ') || pair.label.contains("stød");
                let said = if rng.random_bool(0.5) { &pair.a } else { &pair.b };
                let other = if said == &pair.a { &pair.b } else { &pair.a };
                let wrong = rng.random_bool(if hard { 0.45 } else { 0.15 });
                let given = if wrong { other } else { said };
                put(&mut conn, day, 2500, "set", &lesson.to_string(), lesson, "soundPair", &run, given, said, !wrong, &[], &[pair.a.clone(), pair.b.clone()], "", "");
                answered += 1;
            }
            if back % 5 == 0 {
                let (right_p, wrong_p) = [("under bordet", "på bordet"), ("bag huset", "foran huset"), ("i skabet", "på skabet")][usize::try_from(back % 3).unwrap()];
                let wrong = rng.random_bool(0.4);
                put(&mut conn, day, 5000, "set", &lesson.to_string(), lesson, "sceneWhere", &run, if wrong { wrong_p } else { right_p }, right_p, !wrong, &[], &[right_p.into(), wrong_p.into()], "", "");
                let (id, da, _, lesson) = &sentences[usize::try_from(back).unwrap() % sentences.len()];
                let heard = if rng.random_bool(0.5) { sound(da).unwrap_or_else(|| da.clone()) } else { da.clone() };
                let spoken = crate::speaking::check(da, &heard);
                put(&mut conn, day, 6500, "sentence", &id.to_string(), *lesson, "sayIt", &run, &heard, da, spoken.correct, &spoken.words, &[], "", "");
            }
            let ev = |conn: &mut Conn, kind: &str, details: crate::learnlog::EventDetails| {
                crate::learnlog::write_event(conn, crate::learnlog::EventIn { kind: kind.into(), lesson_id: Some(lesson), run: run.clone(), at: None, details, ..Default::default() }).unwrap();
            };
            ev(&mut conn, "lesson_started", crate::learnlog::EventDetails { of_cards: Some(cards), ..Default::default() });
            if back % 6 == 2 {
                ev(&mut conn, "lesson_left", crate::learnlog::EventDetails { at_card: Some(cards / 2), of_cards: Some(cards), ..Default::default() });
            } else {
                ev(&mut conn, "lesson_finished", crate::learnlog::EventDetails { answered: Some(answered), correct: Some(right), ..Default::default() });
            }
            // Events are written "now": moved to the day they belong to.
            conn.batch_execute(&format!("UPDATE learning_events SET day = {day}, at = {} WHERE run = '{run}'", day * 86_400_000 + 20 * 3_600_000)).unwrap();
            diesel::insert_into(t::day_log::table)
                .values((t::day_log::day.eq(day), t::day_log::device.eq(device), t::day_log::xp.eq(answered * 6), t::day_log::answered.eq(answered), t::day_log::correct.eq(right), t::day_log::minutes.eq(answered / 2 + 3)))
                .on_conflict((t::day_log::day, t::day_log::device)).do_nothing().execute(&mut conn).unwrap();
        }
        let all = Filter::default();
        let month = Filter { from_day: Some(today - 29), to_day: None, course_id: None };
        let n: i64 = t::attempts::table.count().get_result(&mut conn).unwrap();
        println!("{n} attempts");
        write("analytics_overview", &a::overview(&mut conn, &month).unwrap());
        write("analytics_overview_all", &a::overview(&mut conn, &all).unwrap());
        for kind in a::HARD_KINDS {
            write(&format!("analytics_hard_{kind}"), &a::hard_items(&mut conn, kind, &month, 40).unwrap());
        }
        let met = a::met_words(&mut conn).unwrap();
        write("met_words_list", &met);
        write("met_word_detail", &a::met_word(&mut conn, met[0].id).unwrap());
        let structures = a::structures(&mut conn, &month).unwrap();
        write("met_structures", &structures);
        write("met_structure", &a::structure(&mut conn, &structures.tags[0].tag.key, &month).unwrap());
        write("met_sounds", &a::sounds(&mut conn, &month).unwrap());
        let overview = a::overview(&mut conn, &month).unwrap();
        write("analytics_mistakes", &a::mistakes_of(&mut conn, &overview.mistakes[0].class, &month, 40).unwrap());
        write("analytics_compare", &a::compare(&mut conn, &Filter { from_day: Some(today - 13), to_day: Some(today - 7), course_id: None }, &Filter { from_day: Some(today - 6), to_day: Some(today), course_id: None }).unwrap());
        write("analytics_activity", &a::activity(&mut conn, &month).unwrap());
        write("analytics_item", &a::item_history(&mut conn, &met[0].id.to_string()).unwrap());
        // Schema 15: the raw log, a word in its contexts, topics.
        let raw = crate::rawlog::page(&mut conn, &crate::rawlog::RawFilter::default(), None, 50).unwrap();
        write("learning_raw_log", &raw);
        // The word met in the most grammatical contexts, alone and in sentences.
        let candidates: Vec<String> = t::attempt_items::table.filter(t::attempt_items::item_kind.eq("word")).filter(t::attempt_items::role.ne("target"))
            .select(t::attempt_items::item_id).distinct().load(&mut conn).unwrap();
        let busiest = candidates.iter().max_by_key(|id| {
            let c = crate::contexts::item_contexts(&mut conn, id, &all).unwrap();
            (c.by_reason.len().min(4), c.by_role.len(), c.appearances)
        }).unwrap();
        write("met_word_context", &a::met_word(&mut conn, WordId::parse(busiest).unwrap()).unwrap());
        let topics = crate::contexts::topics_page(&mut conn, &month).unwrap();
        write("met_topics", &topics);
        write("met_topic", &crate::contexts::topic_detail(&mut conn, &topics.domains[0].domain.topic.key, &month).unwrap());
        write("met_structure_words", &a::structure(&mut conn, "definite-singular", &all).unwrap());
        write("today", &today);
    }

}

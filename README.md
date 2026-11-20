# Dansk

A little Danish, every day. A native Danish-learning app for English speakers,
adapted from Learn Polish with its own identity and app storage.
Built with **Tauri 2, Rust, SQLite and React** for Android and macOS.

The rules that settle design questions are in
[PRINCIPLES.md](PRINCIPLES.md); how data is kept, in [DATA.md](DATA.md).

## The spec

What the app is meant to do is written down in [`.spec`](.spec): one JSON
file per part of the product, each with user stories and requirements that
can be checked against the app (`MAST-7`, `SYNC-3`). The format is the one
described in
[spec-llms.txt](https://raw.githubusercontent.com/madsbuch/better-github/refs/heads/main/spec-llms.txt);
read it before changing the spec. The spec comes first: a change to what the
app does starts as a new requirement, a new version of one with the reason,
or a deprecation with the reason — never a deletion. It does not record
whether something is built.
`bun run build` checks the spec (`bun run check:spec` alone).

Polski ([learn-polish](https://github.com/madsbuch/learn-polish)) has the same spec, item for
item and id for id; a requirement added or changed here is ported there.

## The app

- **Exercises:** drills (word cards, pictures of everyday objects, typing,
  memory, sentence building, gap fills, dialogues) and conversations (study,
  writing and listening). A **check-in** is a lesson coming back on its
  schedule. Each lesson has one type from a single flat list; see
  [ADDING_A_LESSON_TYPE.md](ADDING_A_LESSON_TYPE.md). Each day the app picks the five
  most important lessons (slipping, then due, then up to two new, then — if a
  place is still free — at most one mastered lesson whose words are due, as a
  light review) as a way in; doing them, or a quick game on a day with none,
  is the day done (a failed check-in counts too: it is the work, not the XP).
  Mastery is for good: a mastered lesson is never due again and never loses
  it, and a mastered course is ready to close — *Close as mastered* is your
  own celebration, and final. To revisit a theme, the tutor makes a new
  course, a level up. A lesson's minutes come from one table per type
  (`lesson_types::minutes`). Extra practice (a lesson already passed today)
  earns half XP, still counts toward mastery, leaves the schedule alone, and
  a fail there costs no mastery.
  Mastery points: the first pass 30, another the same day 10, one a day to
  two weeks later 16 to 35, an ace (every answer fully right) a quarter more;
  a partial (60–84% right) half what a repeat pass would at that gap, without
  passing the lesson; a fail −20. One sitting reaches 90 at most: the last
  stretch takes a pass on a second day. Every change is written in the
  lesson's **mastery ledger** (`mastery_log`, `masterylog.rs`), shown on its
  page as "Mastery points" and to the tutor as `get_mastery_log`. A typed
  answer one letter off a word of four letters or more, or a dictation or
  spoken sentence of three words or more with one word off, is **almost**:
  still a mistake, but half XP, its review comes back sooner rather than
  starting over, and it counts as half right in the drill's score (85%
  passes; a drill of three to six scored cards may have one miss). A drill
  with nothing scored is not graded. A word counts once a day, so only
  answers on different days make it stick.
  XP, streaks and achievements carry over from the original app.
- **Tutor:** ask questions or have the tutor create courses, lessons, drills,
  writing situations and songs on a subject you choose. It replies in the language
  you write in (English when that is unclear) and can switch to Danish
  conversation when requested.
- **Music:** generate learning songs with English/Danish verses and Danish
  choruses; play, organise, download and cast them, with lyrics and listening
  exercises.
- **Settings:** AI models and keys, voices, audio cache, themes, reminders,
  costs, memories and database export.
- **Courses kept offline:** a course's page has *Keep this course offline*.
  Switched on, the app downloads everything in it that is not a conversation —
  the audio of every word, sentence and line, at normal speed and slowly; the
  example sentences a wrong answer shows for each word; and every picture —
  and tops it up
  as the course grows (`src-tauri/src/offline.rs`). Conversation lessons talk
  to a model and need a connection whatever is downloaded; switching it off
  deletes nothing.

The app starts with one small **First steps** demo course with a lesson of
every type — a dialogue, words, typing, memory cards, pictures, a scene,
sentences, gaps, a story, a study lesson, a listening lesson and a writing
lesson — on saying hello and introducing yourself. The drills work without a
key; the lessons that need pictures, a recording or an AI model say so until
one is set, and then make them. The demo is editable and installed only once;
deleting it is permanent.

A lesson is in exactly one course (*Other lessons* counts as one) and is never
moved: for the same practice elsewhere, the tutor makes a new lesson there.
Taking a lesson out of its course deletes it. Before version 18 a lesson could
be listed in several courses; upgrading keeps it in the course it was first
added to.

Upgrading from the first release replaces the unchanged default collection
with this single demo. Edited, extended or shared courses are preserved,
along with user-created courses.

Danish speech uses the device's `da-DK` voice when available, or the configured
OpenRouter speech provider. Listening clips are cached for reuse. Typing accepts
`ae`, `oe` and `aa` for `æ`, `ø` and `å`.

AI features require your own OpenRouter key in Settings; song audio also needs
a kie.ai key. Drills and courses kept offline work without either key; what
needs one says so.

## Visual identity

Dannebrog red, warm linen surfaces and harbor blue replace the original purple
palette. The new mark is a Danish **ø** inside a speech bubble, with a subtle
Nordic cross behind it. Light and dark themes share this palette. The SVG is
used in the app and generates the desktop, Android and iOS icons.

- Design tokens and components: `src/design/`
- Logo: `public/icon.svg`
- Native icon generation: `node scripts/make-icon.mjs`
- Component conventions: [DESIGN.md](DESIGN.md)

## Run locally

Requires Bun and a Rust toolchain, plus the platform prerequisites for Tauri.

```sh
bun install
bun run app:dev         # native desktop app
bun run app:dev:tablet  # tablet-sized window; resize to test the phone layout
bun run android:dev     # configured Android device or emulator
```

`bun run dev` serves the frontend on port **1430**. The app needs the native
Tauri backend for its database and learning features; a browser alone is not
a working app. The distinct port allows the Polish app to run alongside it.

```sh
bun run build                         # design rules, TypeScript and production frontend
cd src-tauri && cargo test --locked    # backend, content and integration tests
```

Build a native package with `bun run app:build`, or initialise the Android
project with `bun run android:init` before `bun run android:build`.

## App identity and data

The product is **Dansk**, package `learn-danish`, native identifier
`com.madsbuch.learndanish`. It uses its own application data directory, browser
storage keys, notification identity and Android media plugin namespace, so it
can be installed alongside Learn Polish. No progress, keys or signing material
are copied from the Polish installation.

All progress, courses, settings, cached speech and generated songs live locally
in SQLite and the app's file storage. Settings → Your data exports a consistent
database copy; API keys are excluded unless explicitly included.

## Source layout

```text
src/design/            shared components, tokens and themes
src/screens/           exercises, tutor, music, settings and detail screens
src/exercises/         one module per exercise kind, and their registry
src/lib/api.ts         typed Tauri command interface
src/lib/types.ts       shared frontend data shapes (Danish content uses da)
src-tauri/src/         database, learning engine, agents, speech and music
src-tauri/src/lesson_types/  every lesson type, one module each, and their registry
src-tauri/plugins/     Android playback, sharing and background work
```

The webview uses Rust commands for database access and network requests.
The tutor's tool definitions and instructions live in `src-tauri/src/tutor.rs`;
all AI agents share the transport in `agent.rs` and `ai.rs`. Wikipedia source
readers can offer the Danish article when one exists.

Beyond the small demo, course content is created in the app and stored in its
database. Development tests use isolated historical fixtures; those old
courses are not included in the app.

## Releases

The included GitHub workflow tests the app and compiles Android and macOS on
pull requests. On `main`, it also publishes `Dansk-android-universal.apk` and
`Dansk-macos-universal.dmg` as a rolling `latest` release after all jobs pass.
Manual runs on other branches keep the APK and DMG as artifacts without publishing.
It is not published by local builds. Android initialization applies the same
launcher icons and keyboard handling locally and in CI.

Release signing requires the four `ANDROID_KEYSTORE_*` / `ANDROID_KEY_*`
repository secrets; a missing secret stops the release. Pull requests build
unsigned test APKs and do not need signing secrets.

This repository has a dedicated Dansk signing key. Its local recovery backup
is in the ignored `.signing/` directory, with owner-only file permissions.
Keep both the keystore and its password details safe: future upgrades must use
the same key. GitHub stores the CI copy as encrypted repository secrets.
For a separate installation, `./scripts/make-android-keystore.sh` generates a
new key and describes the four secrets to configure.

The macOS DMG holds one universal app (Apple Silicon and Intel). It is ad-hoc
signed and not notarized, so the first launch is blocked by Gatekeeper: run
`xattr -dr com.apple.quarantine /Applications/Dansk.app`, or choose *Open Anyway*
in System Settings → Privacy & Security after the first attempt.

# Dansk

A little Danish, every day. A native Danish-learning app for English speakers,
adapted from Learn Polish with its own identity and app storage.
Built with **Tauri 2, Rust, SQLite and React** for Android and macOS.

## The app

- **Exercises:** daily check-ins, word cards, typing, memory, sentence building,
  gap fills, dialogues, writing and listening. Spaced repetition, XP, streaks,
  achievements and adjustable daily goals carry over from the original app.
- **Tutor:** ask questions or have the tutor create courses, lessons, drills,
  writing situations and songs on a subject you choose. It teaches Danish in
  English and can switch to Danish conversation when requested.
- **Music:** generate learning songs with English/Danish verses and Danish
  choruses; play, organise, download and cast them, with lyrics and listening
  exercises.
- **Settings:** AI models and keys, voices, audio cache, themes, reminders,
  costs, memories and database export.

The app starts with an empty course library. Ask the tutor to build a course
around what you want to learn, then edit and reorganise it as you go.
No default curriculum is installed or restored automatically.

Upgrading from the first release removes unchanged default courses. Edited,
extended or shared courses are preserved, along with user-created courses.

Danish speech uses the device's `da-DK` voice when available, or the configured
OpenRouter speech provider. Listening clips are cached for reuse. Typing accepts
`ae`, `oe` and `aa` for `æ`, `ø` and `å`.

AI features require your own OpenRouter key in Settings; song audio also needs
a kie.ai key. Once courses have been created, their offline exercises work
without either key.

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
src/lib/api.ts         typed Tauri command interface
src/lib/types.ts       shared frontend data shapes (Danish content uses da)
src-tauri/src/         database, learning engine, agents, speech and music
src-tauri/plugins/     Android playback, sharing and background work
```

The webview uses Rust commands for database access and network requests.
The tutor's tool definitions and instructions live in `src-tauri/src/tutor.rs`;
all AI agents share the transport in `agent.rs` and `ai.rs`. Wikipedia source
readers can offer the Danish article when one exists.

Course content is created in the app and stored in its database. Development
tests use isolated fixtures; those courses are not included in the app.

## Android releases

The included GitHub workflow tests the app and compiles Android on pull requests.
On `main`, it also publishes
`Dansk-android-universal.apk` as a rolling `latest` release after both jobs pass.
Manual runs on other branches keep the APK as an artifact without publishing.
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

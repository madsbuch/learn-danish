import { useEffect } from "react";

import { Stack, Text } from "../design";
import { speakQuietly } from "../lib/speech";
import type { DiffWord } from "../lib/schemas";
import { SlowButton, SpeakButton } from "./ui";

/*
 * Pieces for the ear (dictation and sound pairs), shared by the lesson cards
 * (src/exercises/listen.tsx) and the quick games: the buttons that say a
 * sentence again, normally or slowly, and a dictation compared word by word.
 */

/**
 * Says `text` once as it appears (a new sentence, a new round), then waits
 * for the buttons. Failures stay silent: the buttons show them.
 */
export function useSayOnce(text: string, gender = "", key: string = text) {
  useEffect(() => {
    // A moment for the card to land first, so the sound goes with it.
    const timer = window.setTimeout(() => speakQuietly(text, gender), 250);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
}

/** Hear it again: normally, or slowly (its own clearer version). */
export function ListenButtons({ text, gender }: { text: string; gender?: string }) {
  return (
    <div className="listen-buttons">
      <Stack gap={1} align="center">
        <SpeakButton text={text} gender={gender} label="Play it again" variant="tinted" />
        <Text variant="caption" tone="faint">
          Again
        </Text>
      </Stack>
      <Stack gap={1} align="center">
        <SlowButton text={text} gender={gender} variant="tinted" />
        <Text variant="caption" tone="faint">
          Slowly
        </Text>
      </Stack>
    </div>
  );
}

/** Danish letters as a keyboard without them spells them. */
const SPELLED: Record<string, string> = { æ: "ae", ø: "oe", å: "aa" };

/**
 * The letters of `right` that `typed` had otherwise ("maelk" for "mælk"):
 * the word with those letters marked. An æ, ø or å typed as two letters
 * lines up with both.
 */
function MarkedLetters({ right, typed }: { right: string; typed: string }) {
  const b = typed.toLowerCase();
  let j = 0;
  return (
    <>
      {[...right].map((letter, i) => {
        const lower = letter.toLowerCase();
        const spelled = SPELLED[lower];
        if (b.startsWith(lower, j)) {
          j += lower.length;
          return letter;
        }
        j += spelled && b.startsWith(spelled, j) ? spelled.length : 1;
        return (
          <mark key={i} className="diff-letter">
            {letter}
          </mark>
        );
      })}
    </>
  );
}

const STATUS_LABEL: Record<DiffWord["status"], string> = {
  right: "right",
  accents: "right, mind the Danish letters",
  wrong: "wrong",
  missing: "missing",
  extra: "not in the sentence",
};

/**
 * A dictation, word by word: right words in green, a word with its Danish
 * letters left off in green with the letters marked, a wrong word struck
 * through with the right one after it, a word left out shown dashed, and a
 * word that wasn't said struck through. The words land one after another,
 * once.
 */
export function DictationDiff({ words, label = "What you wrote, word by word" }: { words: DiffWord[]; label?: string }) {
  const accents = words.filter((w) => w.status === "accents").length;
  return (
    <Stack gap={2}>
      <p className="dictation-diff" aria-label={label}>
        {words.map((w, i) => (
          <span
            key={i}
            className="diff-word"
            data-status={w.status}
            title={STATUS_LABEL[w.status]}
            style={{ "--i": Math.min(i, 12) }}
          >
            {w.status === "right" ? (
              w.expected
            ) : w.status === "accents" ? (
              <MarkedLetters right={w.expected} typed={w.given} />
            ) : w.status === "wrong" ? (
              <>
                <s className="diff-typed">{w.given}</s> <span className="diff-fix">{w.expected}</span>
              </>
            ) : w.status === "missing" ? (
              <span className="diff-fix">{w.expected}</span>
            ) : (
              <s className="diff-typed">{w.given}</s>
            )}
          </span>
        ))}
      </p>
      {accents > 0 ? (
        <Text variant="caption" tone="dim">
          Danish letters marked: accepted without them, but mind them.
        </Text>
      ) : null}
    </Stack>
  );
}

// Dictation uses the web view's speech recognition, which is Apple's in the
// macOS app. Where the web view has none, such as on Linux, the note field
// shows no dictation button.

import { useCallback, useEffect, useRef, useState } from "react";
import { useLatest } from "./useLatest";

type Result = { 0: { transcript: string }; isFinal: boolean };
type Recognition = {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  onresult: ((event: { results: ArrayLike<Result> }) => void) | null;
  onerror: ((event: { error: string }) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
};
type RecognitionClass = new () => Recognition;

const Recognizer: RecognitionClass | undefined =
  typeof window === "undefined"
    ? undefined
    : ((window as unknown as Record<string, RecognitionClass | undefined>).SpeechRecognition ??
      (window as unknown as Record<string, RecognitionClass | undefined>).webkitSpeechRecognition);

/** Whether this window supports speech recognition. */
export const canDictate = !!Recognizer;

/** The message to show when recognition stops with an error, or null when there is none. */
export function dictationError(error: string): string | null {
  switch (error) {
    case "aborted":
    case "no-speech":
      return null;
    case "not-allowed":
    case "service-not-allowed":
      return "Pinrail may not use the microphone or speech recognition. Allow both in System Settings › Privacy & Security.";
    case "audio-capture":
      return "No microphone was found.";
    case "network":
      return "Speech recognition needs a network connection.";
    default:
      return `Dictation stopped: ${error}.`;
  }
}

/** The events a dictation sends its listener: the text recognized so far, then the end. */
export type Heard = { text: string } | { done: true; error?: string };

/**
 * Starts speech recognition and sends `listener` the text recognized so
 * far as it grows, then an end event, with a message if it failed. Returns
 * a stop function; the final text and the end event still follow it.
 */
export function listen(listener: (heard: Heard) => void): () => void {
  if (!Recognizer) {
    listener({ done: true, error: "This window cannot turn speech into text." });
    return () => {};
  }
  const recognition = new Recognizer();
  recognition.continuous = true;
  recognition.interimResults = true;
  recognition.lang = navigator.language;
  let failed: string | null = null;
  recognition.onresult = (event) => {
    let said = "";
    for (let i = 0; i < event.results.length; i++) said += event.results[i][0].transcript;
    listener({ text: said });
  };
  recognition.onerror = (event) => {
    failed = dictationError(event.error) ?? failed;
  };
  recognition.onend = () => listener(failed ? { done: true, error: failed } : { done: true });
  recognition.start();
  return () => recognition.stop();
}

/**
 * Appends recognized speech to a field's text. While recognition runs,
 * `onText` receives the field's text followed by the text recognized so
 * far, and `onError` receives a message when recognition fails.
 */
export function useDictation(onText: (text: string) => void, onError: (message: string) => void) {
  const [listening, setListening] = useState(false);
  const stopping = useRef<(() => void) | null>(null);
  const handlers = useLatest({ onText, onError });

  const stop = useCallback(() => stopping.current?.(), []);
  const start = useCallback(
    (before: string) => {
      if (!Recognizer || stopping.current) return;
      const lead = before && !/\s$/.test(before) ? `${before} ` : before;
      stopping.current = listen((heard) => {
        if ("text" in heard) handlers.current.onText(lead + heard.text.trimStart());
        else {
          stopping.current = null;
          setListening(false);
          if (heard.error) handlers.current.onError(heard.error);
        }
      });
      setListening(true);
    },
    [handlers],
  );
  // leaving the review stops the dictation
  useEffect(() => () => stopping.current?.(), []);
  return { listening, start, stop };
}

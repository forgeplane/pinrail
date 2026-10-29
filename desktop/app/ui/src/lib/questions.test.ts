import { describe, expect, test } from "vitest";
import type { Question } from "../api/types";
import { answersOf, decisionOf, pick, unanswered } from "./questions";

const where: Question = { id: "q1", question: "Where?", answer: "choice", choices: ["staging", "production"] };
const checks: Question = { id: "q2", question: "Which checks?", answer: "choices", choices: ["lint", "tests", "e2e"] };
const keep: Question = { id: "q3", question: "Keep it?", answer: "yes-no" };
const name: Question = { id: "q4", question: "Name?", answer: "text" };
const all = [where, checks, keep, name];

describe("decisionOf", () => {
  test("gives every question an answer, and those left alone as unanswered", () => {
    const drafts = {
      q1: { choice: "staging", note: "  then production  " },
      q2: { choices: ["lint", "e2e"] },
      q3: { yes: false },
      q4: { text: "  v2 " },
    };
    expect(decisionOf(all, drafts)).toEqual({
      answers: [
        { id: "q1", choice: "staging", note: "then production" },
        { id: "q2", choices: ["lint", "e2e"] },
        { id: "q3", yes: false },
        { id: "q4", text: "v2" },
      ],
    });
    expect(decisionOf(all, {})).toEqual({
      answers: all.map((q) => ({ id: q.id, unanswered: true })),
    });
  });

  test("takes the person's own words in place of a choice, while they are writing them", () => {
    const own = { q1: { choice: "staging", own: true, text: "neither" }, q3: { yes: true, own: true, text: "" } };
    expect(decisionOf([where, keep], own).answers).toEqual([
      { id: "q1", text: "neither" },
      // nothing written yet: the pick stands
      { id: "q3", yes: true },
    ]);
  });

  test("keeps a note on a question left unanswered", () => {
    expect(decisionOf([name], { q4: { text: " ", note: "ask Maria" } }).answers).toEqual([
      { id: "q4", unanswered: true, note: "ask Maria" },
    ]);
  });
});

describe("pick", () => {
  test("sets one choice, and toggles among several", () => {
    expect(pick(where, { choice: "staging" }, "production")).toEqual({ choice: "production" });
    const once = pick(checks, undefined, "tests");
    expect(once).toEqual({ choices: ["tests"] });
    // in the order the question offers them, whatever order they are picked in
    expect(pick(checks, pick(checks, once, "e2e"), "lint")).toEqual({ choices: ["lint", "tests", "e2e"] });
    expect(pick(checks, { choices: ["lint", "tests"] }, "lint")).toEqual({ choices: ["tests"] });
  });

  test("unpicks the choice picked, or the yes or no given", () => {
    expect(pick(where, { choice: "staging", note: "n" }, "staging")).toEqual({ note: "n" });
    expect(pick(keep, { yes: true }, "yes")).toEqual({});
    expect(pick(keep, { yes: false }, "yes")).toEqual({ yes: true });
  });

  test("answers yes or no", () => {
    expect(pick(keep, undefined, "yes")).toEqual({ yes: true });
    expect(pick(keep, { yes: true, note: "n" }, "no")).toEqual({ yes: false, note: "n" });
  });

  test("leaves the person's own words for the pick", () => {
    expect(pick(where, { own: true, text: "neither", note: "n" }, "staging")).toEqual({
      choice: "staging",
      own: false,
      text: "neither",
      note: "n",
    });
  });
});

test("unanswered counts the questions that would go back without an answer", () => {
  expect(unanswered(all, { q1: { choice: "staging" }, q2: { choices: [] }, q4: { text: " " } })).toBe(3);
});

test("answersOf reads a decision back by question", () => {
  const answers = answersOf({
    answers: [
      { id: "q1", choice: "staging" },
      { id: "q3", unanswered: true },
    ],
  });
  expect(answers.get("q1")).toEqual({ id: "q1", choice: "staging" });
  expect(answers.get("q3")?.unanswered).toBe(true);
  expect(answersOf(null).size).toBe(0);
});

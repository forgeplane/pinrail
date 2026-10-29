// The person's answers to questions an agent asked with `pinrail ask`: what
// they picked or wrote for each question while answering, and the decision
// that is sent, which gives every question an answer or marks it unanswered.

import type { Answer, Question } from "../api/types";

/** What the person has picked or written for one question so far. */
export type Draft = {
  choice?: string;
  choices?: string[];
  yes?: boolean;
  /** the answer to a text question, or the person's own words for another */
  text?: string;
  /** whether they are answering a question with choices in their own words */
  own?: boolean;
  note?: string;
};

/** The answer a draft gives, or none when the question would go back unanswered. */
function given(question: Question, draft: Draft | undefined): Omit<Answer, "id" | "note"> | null {
  if (!draft) return null;
  const words = draft.text?.trim();
  if ((question.answer === "text" || draft.own) && words) return { text: words };
  switch (question.answer) {
    case "choice":
      return draft.choice !== undefined ? { choice: draft.choice } : null;
    case "choices":
      return draft.choices?.length ? { choices: draft.choices } : null;
    case "yes-no":
      return draft.yes !== undefined ? { yes: draft.yes } : null;
    default:
      return null;
  }
}

/** The decision to send: an answer for every question, in their order. */
export function decisionOf(questions: Question[], drafts: Record<string, Draft>): { answers: Answer[] } {
  return {
    answers: questions.map((question) => {
      const draft = drafts[question.id];
      const answer: Answer = { id: question.id, ...(given(question, draft) ?? { unanswered: true }) };
      const note = draft?.note?.trim();
      if (note) answer.note = note;
      return answer;
    }),
  };
}

/** How many questions would go back without an answer. */
export function unanswered(questions: Question[], drafts: Record<string, Draft>): number {
  return questions.filter((q) => !given(q, drafts[q.id])).length;
}

/** The draft after the person picks `value`: a choice, a toggle among
 *  several, or yes or no. Picking leaves their own words. */
export function pick(question: Question, draft: Draft | undefined, value: string): Draft {
  const next: Draft = { ...draft };
  if (draft?.own) next.own = false;
  switch (question.answer) {
    case "choices": {
      const had = draft?.choices ?? [];
      const chosen = had.includes(value) ? had.filter((c) => c !== value) : [...had, value];
      next.choices = (question.choices ?? []).filter((c) => chosen.includes(c));
      break;
    }
    case "yes-no":
      next.yes = value === "yes";
      break;
    default:
      next.choice = value;
  }
  return next;
}

/** A decision's answers, by question. */
export function answersOf(data: unknown): Map<string, Answer> {
  const answers = (data as { answers?: Answer[] } | null)?.answers ?? [];
  return new Map(answers.map((a) => [a.id, a]));
}

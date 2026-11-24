// The questions of one `pinrail ask` at component level, against the fake
// core: picking with the pointer and the keys, moving between questions,
// answers in the person's own words, and what is sent.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Question, Review } from "../../api/types";
import { core } from "../../testing/core";
import { cleanup, render } from "../../testing/render";
import { QuestionSet } from "./QuestionSet";

vi.mock("../../api/client", async (real) => (await import("../../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
  sessionStorage.clear();
});

let next = 1;

/** A pending question set of `questions`, asked by an agent's session. */
function asked(questions: Question[], fields: Partial<Review> = {}): Review {
  return {
    id: `r_${next++}`,
    kind: "question",
    plugin: "question",
    title: "Questions",
    status: "pending",
    requested_by: "claude-code",
    session: "s-1",
    created_at: "2026-11-19T09:00:00Z",
    payload: { questions },
    ...fields,
  } as Review;
}

/** The set, active as the thread makes it, with a footer for its box. */
async function open(review: Review, active = true) {
  const footer = document.createElement("div");
  document.body.append(footer);
  const onDone = vi.fn();
  core.answer("decide", {});
  await render(<QuestionSet review={review} active={active} footer={footer} onActivate={() => {}} onDone={onDone} />);
  return { onDone, footer };
}

const deploy: Question = {
  id: "deploy",
  question: "Deploy to staging or production?",
  answer: "choice",
  choices: ["staging", "production"],
  recommended: "staging",
};
const keep: Question = { id: "keep", question: "Keep the old endpoint?", answer: "yes-no" };
const name: Question = { id: "name", question: "What should the endpoint be called?", answer: "text" };

const chips = (question: Question) => page.getByRole("radiogroup", { name: question.question });
const send = () => userEvent.keyboard("{ControlOrMeta>}{Enter}{/ControlOrMeta}");

describe("a question set", () => {
  test("shows each question with its chips, the recommended one marked", async () => {
    await open(asked([deploy, keep]));
    await expect.element(chips(deploy).getByRole("radio", { name: /staging/ })).toBeVisible();
    await expect
      .element(
        chips(deploy)
          .getByRole("radio", { name: /staging/ })
          .getByLabelText("recommended"),
      )
      .toBeInTheDocument();
    await expect.element(chips(keep).getByRole("radio", { name: /yes/i })).toBeVisible();
  });

  test("a number key picks in the current question and moves on to the next one unanswered", async () => {
    const review = asked([keep, deploy]);
    await open(review);
    await userEvent.keyboard("1");
    await expect.element(chips(keep).getByRole("radio", { name: /yes/i })).toHaveAttribute("aria-checked", "true");
    await userEvent.keyboard("2");
    await expect
      .element(chips(deploy).getByRole("radio", { name: /production/ }))
      .toHaveAttribute("aria-checked", "true");
    await send();
    await expect
      .poll(() => core.calls("decide"))
      .toEqual([
        [
          review.id,
          {
            answers: [
              { id: "keep", yes: true },
              { id: "deploy", choice: "production" },
            ],
          },
          "",
        ],
      ]);
  });

  test("a click picks, and a second click on the same chip unpicks", async () => {
    await open(asked([deploy]));
    const staging = chips(deploy).getByRole("radio", { name: /staging/ });
    await userEvent.click(staging);
    await expect.element(staging).toHaveAttribute("aria-checked", "true");
    await userEvent.click(staging);
    await expect.element(staging).toHaveAttribute("aria-checked", "false");
  });

  test("N and P move between the questions", async () => {
    await open(asked([deploy, keep]));
    const blocks = page.getByTestId("question");
    await expect.element(blocks.nth(0)).toHaveClass(/is-current/);
    await userEvent.keyboard("n");
    await expect.element(blocks.nth(1)).toHaveClass(/is-current/);
    await userEvent.keyboard("p");
    await expect.element(blocks.nth(0)).toHaveClass(/is-current/);
  });

  test("a question answered in words sends the words, and one left alone goes back unanswered", async () => {
    const review = asked([name, keep]);
    const { onDone } = await open(review);
    await userEvent.fill(page.getByTestId("question").nth(0).getByRole("textbox"), "  /v2/rates  ");
    await send();
    await expect
      .poll(() => core.calls("decide"))
      .toEqual([
        [
          review.id,
          {
            answers: [
              { id: "name", text: "/v2/rates" },
              { id: "keep", unanswered: true },
            ],
          },
          "",
        ],
      ]);
    await expect.poll(() => onDone.mock.calls.length).toBe(1);
  });

  test("the keys answer only the set that is active", async () => {
    await open(asked([keep]), false);
    await userEvent.keyboard("1");
    await expect.element(chips(keep).getByRole("radio", { name: /yes/i })).toHaveAttribute("aria-checked", "false");
    await send();
    expect(core.calls("decide")).toEqual([]);
  });

  test("the keys leave the set alone behind the terminal, and while typing in it", async () => {
    core.answer("decide", {});
    const footer = document.createElement("div");
    document.body.append(footer);
    const set = (hidden: boolean) => (
      <div className="thread-main" aria-hidden={hidden || undefined}>
        <QuestionSet review={asked([keep])} active footer={footer} onActivate={() => {}} onDone={() => {}} />
      </div>
    );
    // the Terminal view covers the summary
    await render(set(true));
    await send();
    await userEvent.keyboard("{ControlOrMeta>}d{/ControlOrMeta}");
    expect(core.calls("decide")).toEqual([]);
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    cleanup();

    // the Split view, with the focus in the terminal
    await render(
      <>
        {set(false)}
        <div className="terminal-pane">
          <textarea aria-label="Terminal input" />
        </div>
      </>,
    );
    await userEvent.click(page.getByRole("textbox", { name: "Terminal input" }));
    await send();
    expect(core.calls("decide")).toEqual([]);
  });

  test("a set that was answered shows no chips to pick", async () => {
    await open(
      asked([keep], {
        status: "decided",
        decision: { data: { answers: [{ id: "keep", yes: false }] } },
      } as Partial<Review>),
      false,
    );
    await expect.element(page.getByTestId("question")).toBeVisible();
    await expect.element(chips(keep)).not.toBeInTheDocument();
  });
});

describe("how unanswered questions ended", () => {
  test.each([
    [
      "withdrawn",
      {
        status: "withdrawn",
        withdrawn_by: "claude-code",
        withdrawn_reason: "Reworking: each reply becomes its own row.",
      },
      "Withdrawn",
      "Reworking: each reply becomes its own row.",
    ],
    ["declined", { status: "discarded", discarded_by: "Ada", discarded_reason: "Not now." }, "Declined", "Not now."],
  ] as const)("a %s set says so in a badge, with the reason", async (_, fields, badge, reason) => {
    await open(asked([deploy], fields as Partial<Review>), false);
    const ended = page.getByText(badge, { exact: true });
    await expect.element(ended).toHaveClass(/status-badge/);
    // the reason, in the normal text colour
    const said = page.getByText(reason, { exact: true });
    await expect.element(said).toBeVisible();
    expect(said.element().closest(".set-ended-reason")).not.toBeNull();
  });

  test("an expired set says so in a badge", async () => {
    await open(asked([deploy], { status: "expired", expires_at: "2026-11-19T10:00:00Z" }), false);
    await expect.element(page.getByText("Expired", { exact: true })).toHaveClass(/status-expired/);
  });
});

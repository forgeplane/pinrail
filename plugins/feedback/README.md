# feedback

Questions an agent wants answered before it goes on: grouped, conditional, and
answered in one pass. The agent writes the questions and may recommend an
answer; the person answers, qualifies any of them with a comment, and hands
the lot back. Nothing here acts on the answers — what they mean is between the
agent and the workflow that reads them.

## Asking

It comes with the app, and the setup installs it; `pinrail plugins install
feedback` installs it otherwise. The payload inside any fixture here is
a working request (a fixture file wraps one in a review):

```sh
node -e 'process.stdout.write(JSON.stringify(require("./fixtures/01-incident.json").payload))' > /tmp/feedback-payload.json
pinrail submit feedback --title "Choose a recovery plan for checkout failures" --data /tmp/feedback-payload.json --wait
```

The header uses the supplied review title. The plugin has no submit button; Pinrail owns the final hand-over. It does not execute the agent's proposed actions.

## Keys

`j` / `k` move to the next and previous question and focus its answer, so
the arrow keys or space answer it. They do nothing while you type in a text
box.

## Payload

See `schemas/payload.schema.json` for the complete contract and `fixtures/` for examples.

```json
{
  "description": "I need your preferences before sending the invitation.",
  "groups": [{
    "id": "invitation",
    "title": "Invitation details",
    "questions": [{
      "id": "include_guests",
      "type": "boolean",
      "prompt": "Allow guests to bring a friend?",
      "required": true
    }, {
      "id": "instructions",
      "type": "text",
      "prompt": "What should guests know?",
      "when": {"question_id": "include_guests", "operator": "equals", "value": true}
    }]
  }]
}
```

Every group and question needs a stable, unique ID. Questions are optional unless `required: true`.

| Type | Answer | Options |
| --- | --- | --- |
| `single_choice` | One option ID | `options: [{id, label, description?}]` |
| `multiple_choice` | Array of option IDs | `options`, `min_selections`, `max_selections` |
| `text` | Free-text string | `placeholder`, `min_length`, `max_length` |
| `boolean` | Explicit `true` or `false` | Yes/No radios; No satisfies required questions |
| `checkbox` | `true` or `false` | `checkbox_label`; required acknowledgments must be checked |

Choice, boolean, and acknowledgment questions have optional comments. Free-text questions use the answer itself for context, without a redundant comment field. Comments never satisfy a required answer.

`recommendation: {answer, reason?}` marks the agent's suggestion without preselecting anything; the person can take it with one click. Previous-round responses appear in a disclosure and do not prefill the new response.

### Conditions

Both groups and questions support `when`. References must point to earlier questions; group conditions can only reference questions before that group.

```json
{"all": [
  {"question_id": "notify", "operator": "equals", "value": true},
  {"question_id": "channels", "operator": "contains", "value": "email"}
]}
```

Use `all` / `any` to combine conditions. Operators are `equals`, `not_equals`, `contains` (multiple choice only), and `answered` (no value). Hidden, unanswered, or invalid parent answers never satisfy a condition, including `not_equals`. Text comparisons are exact, including whitespace. Cycles and forward references are rejected.

Changing an answer hides dependent questions immediately. Their drafts are retained if the branch becomes relevant again, but hidden answers and comments are excluded from the final response.

## Decision

```json
{
  "answers": [
    {"question_id": "include_guests", "answer": false, "comment": "Keep this first event small."}
  ],
  "unanswered": [],
  "excluded": ["instructions"]
}
```

- `answers` contains applicable answers and comments, in question order. Text is trimmed; its `comment` is always an empty string.
- `unanswered` lists visible optional questions without answers. An optional comment-only response has `answer: null` and also appears here.
- `excluded` lists questions hidden by conditions. Their values and comments are never included in `answers`.
- Required answers and configured limits are validated before submission. There is no implicit approval verdict: agents interpret the explicit responses.

The JSON Schema validates the response structure. Payload-specific rules, including valid choice IDs, dependencies, and required fields, are enforced by the view's shared core; agents consuming decisions should also validate them against their original request.

## The files

`schemas/` is the contract. `view/feedback-core.js` owns validation, conditions
and what a decision is made of, and is tested on its own in
`tests/core.test.cjs`; `view/feedback.js` is the SDK lifecycle and the
controls; `view/feedback.css` styles them with the SDK's tokens. Everything is
local: a view has no network.

`pnpm exec pinrail-sdk dev feedback` opens it in a browser without the app.

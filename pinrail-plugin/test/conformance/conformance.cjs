// What every host of a plugin view must do, whichever it is: the app, the
// SDK's dev shell, its test harness, or the core's preview page. Each check
// takes what the test view received and returns the problems it found, so
// a host's test can report them all at once.

const isObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

/** `appearance`, when a host sends one, comes before `init`; `init` carries
 *  every field of protocol 1. */
function handshakeProblems(received) {
  const problems = [];
  const types = received.map((m) => m.type);
  const init = received.find((m) => m.type === "init");
  if (!init) return ["no init"];
  const appearance = types.indexOf("appearance");
  if (appearance !== -1 && appearance > types.indexOf("init")) problems.push("appearance came after init");
  if (appearance !== -1 && !["dark", "light"].includes(received[appearance].theme))
    problems.push(`appearance.theme is ${JSON.stringify(received[appearance].theme)}`);
  const review = init.review;
  if (!isObject(review)) problems.push("init.review is not an object");
  else {
    for (const key of ["id", "title", "status"])
      if (typeof review[key] !== "string") problems.push(`init.review.${key} is not a string`);
    if (!("payload" in review)) problems.push("init.review has no payload");
    if (!Array.isArray(review.attachments)) problems.push("init.review.attachments is not a list");
  }
  if (!(init.previous === null || isObject(init.previous)))
    problems.push("init.previous is neither null nor an object");
  if (typeof init.readonly !== "boolean") problems.push("init.readonly is not a boolean");
  if (!("draft" in init)) problems.push("init has no draft");
  if (!isObject(init.settings)) problems.push("init.settings is not an object");
  if (typeof init.shell_origin !== "string") problems.push("init.shell_origin is not a string");
  if (!Array.isArray(init.capabilities) || !init.capabilities.includes("attachments"))
    problems.push("init.capabilities does not list attachments");
  return problems;
}

/** The answer to `attachment` for the fixture's note.txt, with request 7. */
function attachmentProblems(received) {
  const answer = received.find((m) => m.type === "attachment" && m.req === 7);
  if (!answer) return ["no answer to attachment request 7"];
  const problems = [];
  if (answer.ok !== true) problems.push(`attachment answer not ok: ${answer.error}`);
  if (answer.name !== "note.txt") problems.push(`attachment answer names ${answer.name}`);
  if (answer.bytes !== "hello from the review") problems.push(`attachment bytes are ${JSON.stringify(answer.bytes)}`);
  if (typeof answer.size !== "number") problems.push("attachment answer has no size");
  if (answer.media_type !== "text/plain") problems.push(`attachment media_type is ${answer.media_type}`);
  return problems;
}

/** The answer to an attachment the review does not carry, with request 8. */
function refusedAttachmentProblems(received) {
  const answer = received.find((m) => m.type === "attachment" && m.req === 8);
  if (!answer) return ["no answer to attachment request 8"];
  const problems = [];
  if (answer.ok !== false) problems.push("an attachment the review does not carry was handed over");
  if (typeof answer.error !== "string") problems.push("the refusal gives no error");
  return problems;
}

/** A submit that fails the decision schema is answered with violations. */
function violationsProblems(received) {
  const answer = received.find((m) => m.type === "violations");
  if (!answer) return ["no violations for a decision that fails the schema"];
  if (
    !Array.isArray(answer.errors) ||
    !answer.errors.every((e) => typeof e.path === "string" && typeof e.message === "string")
  )
    return ["violations.errors is not a list of { path, message }"];
  return [];
}

/** A second `ready` from the frame is another page in the view's place, and
 *  the host answers it nothing: no `init`, and no answer to the attachment
 *  request 9 the page sends next. `before` is how many messages the view had
 *  received when the second `ready` went out. */
function secondReadyProblems(received, before) {
  const after = received.slice(before);
  const problems = [];
  if (after.some((m) => m.type === "init")) problems.push("a second ready was answered with init");
  if (after.some((m) => m.type === "attachment" && m.req === 9))
    problems.push("a request after a second ready was answered");
  if (after.length) problems.push(`the host still sent ${after.map((m) => m.type).join(", ")}`);
  return problems;
}

/** Every `collect` carries a request number of its own. */
function collectProblems(received) {
  const reqs = received.filter((m) => m.type === "collect").map((m) => m.req);
  if (!reqs.length) return ["no collect"];
  const problems = [];
  if (!reqs.every((r) => Number.isInteger(r)))
    problems.push(`a collect has no request number: ${JSON.stringify(reqs)}`);
  if (new Set(reqs).size !== reqs.length) problems.push(`a request number was used twice: ${JSON.stringify(reqs)}`);
  return problems;
}

/** The number of the last request for the decision the view received. */
const lastRequest = (received) => received.filter((m) => m.type === "collect").at(-1)?.req;

module.exports = {
  collectProblems,
  lastRequest,
  handshakeProblems,
  attachmentProblems,
  refusedAttachmentProblems,
  violationsProblems,
  secondReadyProblems,
};

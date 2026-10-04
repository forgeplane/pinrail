// The types hold a view to what they claim: a view written as an author
// would compiles, and each mistake they exist to catch does not.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const ts = require("typescript");

const dir = path.join(__dirname, "types");
// the SDK's global comes from types.d.ts, which every file sees
const types = path.join(__dirname, "..", "types.d.ts");

function errors(file) {
  const program = ts.createProgram([path.join(dir, file), types], {
    strict: true,
    noEmit: true,
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    lib: ["lib.es2022.d.ts", "lib.dom.d.ts"],
    types: [],
    skipLibCheck: true,
  });
  return ts.getPreEmitDiagnostics(program).map((d) => ts.flattenDiagnosticMessageText(d.messageText, "\n"));
}

test("a view written against the types compiles", () => {
  assert.deepEqual(errors("good.ts"), []);
});

for (const [file, what] of [
  ["bad-decision.ts", "a decision of the wrong shape"],
  ["bad-draft.ts", "a draft used without a check"],
  ["bad-previous.ts", "the previous round read as this release's"],
  ["bad-review.ts", "a field the view does not receive"],
]) {
  test(`${what} does not compile`, () => {
    assert.notDeepEqual(errors(file), [], `${file} compiled`);
  });
}


## Tests

`tests/__NAME__.spec.ts` runs the view under the plugin SDK's harness,
with Playwright and without the app: it opens the sample, answers it, and
checks the decision the view hands over.

```sh
npm install                         # once
npx playwright install chromium     # once, the browser the tests run in
npm test                            # the tests in tests/
```

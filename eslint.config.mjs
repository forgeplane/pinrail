// ESLint for the JavaScript and TypeScript across the repository: the app's
// UI, the SDK, the plugins, the tests and the scripts. `npm run lint` at the
// root runs it over the files git tracks, and so do `mise run lint` and CI.
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

export default tseslint.config(
  // a disable comment that no longer silences anything is an error too
  { linterOptions: { reportUnusedDisableDirectives: "error" } },
  js.configs.recommended,
  tseslint.configs.recommended,
  {
    rules: {
      // a leading underscore marks an argument or binding kept on purpose
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_", ignoreRestSiblings: true },
      ],
    },
  },
  // code that runs in a browser: the app's UI, plugin views, the SDK's bridge
  {
    files: [
      "desktop/app/ui/**",
      "plugins/*/view/**",
      "plugins/*/src/**",
      "pinrail-plugin/src/**",
      "pinrail-plugin/host/**",
      "pinrail-plugin/shell/inspector.js",
      "pinrail-plugin/templates/*/src/**",
      "pinrail-plugin/templates/*/view/**",
      "docs/examples/*/*/src/**",
      "website/public/**",
    ],
    languageOptions: { globals: globals.browser },
  },
  // a plugin view's scripts share the page: the SDK's Pinrail global, and
  // a module check so the same file loads under Node for its tests
  {
    files: ["plugins/*/view/**", "plugins/*/src/**", "pinrail-plugin/templates/*/view/**", "pinrail-plugin/src/**"],
    languageOptions: { globals: { Pinrail: "readonly", module: "readonly" } },
    // a plain view's types come from a triple-slash reference to the SDK's
    rules: { "@typescript-eslint/triple-slash-reference": "off" },
  },
  // code that runs in Node: scripts, tests, the SDK's tools, build configs
  {
    files: ["**/*.cjs", "**/*.mjs", "**/tests/**", "**/test/**", "e2e/**", "**/*.config.*"],
    languageOptions: { globals: globals.node },
  },
  // tests, and the harness they use, also run functions inside the page
  {
    files: ["**/tests/**", "**/test/**", "e2e/**", "pinrail-plugin/harness/**"],
    languageOptions: { globals: globals.browser },
    rules: { "@typescript-eslint/no-explicit-any": "off" },
  },
  // the SDK's public types take a view's payload and settings as they come
  {
    files: ["**/*.d.ts", "**/*.d.cts"],
    rules: { "@typescript-eslint/no-explicit-any": "off" },
  },
  {
    files: ["**/*.cjs"],
    languageOptions: { sourceType: "commonjs" },
    rules: { "@typescript-eslint/no-require-imports": "off" },
  },
  // React: the rules of hooks, and complete dependency lists
  {
    files: ["**/*.tsx", "**/*.jsx", "desktop/app/ui/**/*.ts", "plugins/artifact/src/**/*.ts"],
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
);

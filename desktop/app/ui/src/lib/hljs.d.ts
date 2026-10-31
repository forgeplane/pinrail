// highlight.js's browser builds ship without types: the little the app uses.

declare module "@highlightjs/cdn-assets/es/core.min.js" {
  type Grammar = (hljs: unknown) => unknown;
  const hljs: {
    registerLanguage(name: string, grammar: Grammar): void;
    getLanguage(name: string): unknown;
    highlight(code: string, options: { language: string; ignoreIllegals?: boolean }): { value: string };
  };
  export default hljs;
}

declare module "@highlightjs/cdn-assets/es/languages/*" {
  const grammar: (hljs: unknown) => unknown;
  export default grammar;
}

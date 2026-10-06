/*
 * The Markdown renderer of the pinrail plugin SDK, protocol 1.
 *
 * Served by the app at /sdk/v1/markdown.js. A view that renders Markdown
 * loads it with a second script tag, beside the SDK's, and then calls
 * Pinrail.markdown(text) and Pinrail.markdownInline(text):
 *
 *   <script src="/sdk/v1/pinrail-plugin.js"></script>
 *   <script src="/sdk/v1/markdown.js"></script>
 *
 * The served file is this one with markdown-it in front of it, as
 * `markdownit` (lib/paths.cjs). It leaves no global of its own but the
 * renderer the SDK looks up.
 *
 * What the renderer promises is that the HTML is its own: raw HTML in the
 * source is escaped rather than passed through, which matters because a
 * view's frame runs inline scripts, so markup that reached the DOM would
 * run. Addresses are checked too: a link to anything but http, https or
 * mailto keeps its text and loses its address. Styling stays the view's:
 * the renderer writes plain elements and no classes.
 */
/* global markdownit -- the parser, put in front of this file when it is served */
(function (root, markdownit) {
  "use strict";

  const SAFE_HREF = /^(https?:|mailto:)/i;

  const parser = markdownit({
    // the default, and the reason no sanitiser is needed: raw HTML is escaped
    html: false,
    linkify: true,
    breaks: false,
    typographer: false,
  });
  const link = parser.renderer.rules.link_open || ((t, i, o, e, self) => self.renderToken(t, i, o));
  parser.renderer.rules.link_open = (tokens, i, options, env, self) => {
    if (!SAFE_HREF.test(tokens[i].attrGet("href") || "")) tokens[i].attrSet("href", "#");
    tokens[i].attrSet("rel", "noreferrer");
    tokens[i].attrSet("target", "_blank");
    return link(tokens, i, options, env, self);
  };

  root[Symbol.for("pinrail.markdown")] = {
    render: (src) => parser.render(src),
    renderInline: (src) => parser.renderInline(src),
  };
})(typeof window !== "undefined" ? window : globalThis, markdownit);

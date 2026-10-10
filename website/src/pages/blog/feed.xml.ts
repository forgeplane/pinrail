// The blog's Atom feed, built from the list of posts.
import type { APIRoute } from "astro";
import { allPosts, postUrl } from "../../components/blog/posts";

const escape = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

export const GET: APIRoute = async ({ site }) => {
  const posts = await allPosts();
  const at = (path: string) => new URL(path, site).href;
  const entries = posts.map(
    (post) => `  <entry>
    <title>${escape(post.data.title)}</title>
    <link href="${at(postUrl(post))}"/>
    <id>${at(postUrl(post))}</id>
    <published>${post.data.date}T00:00:00Z</published>
    <updated>${post.data.date}T00:00:00Z</updated>
    <summary>${escape(post.data.dek)} ${escape(post.data.description)}</summary>
  </entry>`,
  );
  const body = `<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Pinrail blog</title>
  <link href="${at("/blog/")}"/>
  <link rel="self" href="${at("/blog/feed.xml")}"/>
  <id>${at("/blog/")}</id>
  <updated>${posts[0].data.date}T00:00:00Z</updated>
  <author><name>Forgeplane</name></author>
${entries.join("\n")}
</feed>
`;
  return new Response(body, { headers: { "Content-Type": "application/atom+xml; charset=utf-8" } });
};

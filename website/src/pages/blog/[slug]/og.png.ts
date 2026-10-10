// A post's link-preview image, at /blog/<slug>/og.png.
import type { APIRoute } from "astro";
import { allPosts, type Post } from "../../../components/blog/posts";
import { card } from "../../../components/blog/card";

export async function getStaticPaths() {
  return (await allPosts()).map((post) => ({ params: { slug: post.id }, props: { post } }));
}

export const GET: APIRoute = async ({ props }) =>
  new Response(new Uint8Array(await card((props as { post: Post }).post)), {
    headers: { "Content-Type": "image/png" },
  });

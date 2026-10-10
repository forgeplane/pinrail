// The blog's posts, from the `blog` collection (src/content.config.ts),
// for the index, the feed and each post's page.
import { getCollection, type CollectionEntry } from "astro:content";

export type Post = CollectionEntry<"blog">;

/** Every post, newest first. */
export const allPosts = async () =>
  (await getCollection("blog")).sort((a, b) => b.data.date.localeCompare(a.data.date));

export const postUrl = (post: Post) => `/blog/${post.id}/`;

/** "10 October 2026" */
export const longDate = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });

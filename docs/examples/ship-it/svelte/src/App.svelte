<!-- Ship it? in Svelte. main.ts connects to the app and mounts this with what
     the app handed over (the review, whether it is read-only, the draft),
     and the `view` it fills in. The app's hand-over button asks for the
     decision, and `view` answers with it. The SDK is on the window from the
     script tag in index.html; the types come from the package. -->
<script lang="ts" module>
  import type { Violation } from "@forgeplane/pinrail-plugin/types";

  export type Payload = {
    service: string;
    version: string;
    environment: string;
    changes: { title: string; risky?: boolean }[];
    checks: { name: string; passed: boolean; detail?: string }[];
  };
  type Verdict = "ship" | "hold";
  export type Decision = { verdict: Verdict; note?: string };
  /** what is kept while the person decides: a verdict may not be chosen yet */
  type Draft = { verdict: Verdict | null; note: string };

  /** What the connection in main.ts asks of the view on screen. */
  export type View = {
    /** the decision, or nothing while there is no verdict to hand over */
    collect: () => Decision | undefined;
    violations: (errors: Violation[]) => void;
    submitted: () => void;
  };

  /** A draft kept by an earlier release may have another shape: use only
   *  what reads as this one's. */
  function draftOf(kept: unknown): Draft {
    const d = kept && typeof kept === "object" ? (kept as Partial<Draft>) : {};
    return {
      verdict: d.verdict === "ship" || d.verdict === "hold" ? d.verdict : null,
      note: typeof d.note === "string" ? d.note : "",
    };
  }
</script>

<script lang="ts">
  import { CircleCheck, CircleX, Hand, Rocket } from "@lucide/svelte";
  import { untrack } from "svelte";
  import type { Init, Plugin } from "@forgeplane/pinrail-plugin/types";

  let { plugin, init, view }: { plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision>; view: View } =
    $props();

  // the component is mounted afresh for each init, so it starts from it once
  let review = $state(untrack(() => init.review));
  let readonly = $state(untrack(() => init.readonly));
  let draft = $state<Draft>(untrack(() => draftOf(init.draft)));
  let error = $state("");
  // the object main.ts calls, kept for the life of the page: filled in once
  const handlers = untrack(() => view);

  handlers.collect = () => {
    const { verdict, note } = draft;
    if (!verdict) {
      error = "Choose ship or hold first.";
      return;
    }
    return note.trim() ? { verdict, note: note.trim() } : { verdict };
  };
  handlers.violations = (errors) => {
    error = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
  };
  handlers.submitted = () => {
    review = plugin.review!;
    readonly = true;
  };

  function choose(verdict: Verdict) {
    draft.verdict = verdict;
    error = "";
    plugin.draft(draft);
  }

  function writeNote(event: Event) {
    draft.note = (event.target as HTMLTextAreaElement).value;
    plugin.draft(draft);
  }

  // s and h decide. The app forwards them too when it has the focus, as a
  // keydown on the document itself, so the target is not always an element.
  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof Element && e.target.closest("textarea, input");
    if (readonly || e.metaKey || e.ctrlKey || e.altKey || typing) return;
    if (e.key === "s") choose("ship");
    if (e.key === "h") choose("hold");
  }

  // what the app's hand-over button says follows the choice
  $effect(() => {
    if (readonly) return;
    const label = draft.verdict === "ship" ? `Ship ${review.payload.version}` : draft.verdict === "hold" ? "Hold the deploy" : "Choose ship or hold";
    plugin.status({ label });
  });

  const decided = $derived(review.decision?.data);
</script>

<svelte:document onkeydown={onKey} />

{#if review}
  <main class="plugin-content ship">
    <p class="eyebrow">Deploy to {review.payload.environment}</p>
    <h1>{review.payload.service} <span class="meta">{review.payload.version}</span></h1>
    <section>
      <h2 class="eyebrow">Changes</h2>
      <ul aria-label="Changes">
        {#each review.payload.changes as c (c.title)}
          <li>{c.title}{#if c.risky}{" "}<span class="sev sev-major">risky</span>{/if}</li>
        {/each}
      </ul>
    </section>
    <section>
      <h2 class="eyebrow">Checks</h2>
      <ul aria-label="Checks">
        {#each review.payload.checks as c (c.name)}
          <li data-passed={String(c.passed)}>
            {#if c.passed}<CircleCheck />{:else}<CircleX />{/if} {c.name}{#if c.detail}{" "}<span class="detail">{c.detail}</span>{/if}
          </li>
        {/each}
      </ul>
    </section>
    {#if readonly}
      <p class="decided"><b>{decided?.verdict === "ship" ? "Shipped" : "Held"}</b>{#if decided?.note}: {decided.note}{/if}</p>
    {:else}
      <div class="choice" role="group" aria-label="Verdict">
        <button type="button" class="btn" aria-pressed={draft.verdict === "ship"} onclick={() => choose("ship")}>
          <Rocket /> Ship <kbd>s</kbd>
        </button>
        <button type="button" class="btn" aria-pressed={draft.verdict === "hold"} onclick={() => choose("hold")}>
          <Hand /> Hold <kbd>h</kbd>
        </button>
      </div>
      <textarea class="note" aria-label="Note to the agent" placeholder="A note for the agent (optional)" value={draft.note} oninput={writeNote}></textarea>
      <div class="errors" role="alert">{error}</div>
    {/if}
  </main>
{/if}

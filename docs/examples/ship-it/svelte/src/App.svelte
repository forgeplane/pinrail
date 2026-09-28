<!-- Ship it? in Svelte. The SDK is connected once, when the view mounts; what
     it hands over (the review, whether it is read-only, the draft) becomes
     state, and the markup renders from it. The SDK is on the window from the
     script tag in index.html; the types come from the package. -->
<script lang="ts">
  import { CircleCheck, CircleX, Hand, Rocket } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { Review, Plugin } from "@forgeplane/pinrail-plugin/types";

  type Payload = {
    service: string;
    version: string;
    environment: string;
    changes: { title: string; risky?: boolean }[];
    checks: { name: string; passed: boolean; detail?: string }[];
  };
  type Verdict = "ship" | "hold";
  type Decision = { verdict: Verdict; note?: string };
  /** what is kept while the person decides: a verdict may not be chosen yet */
  type Draft = { verdict: Verdict | null; note: string };

  const { Pinrail } = window;
  let review = $state<Review<Payload, Decision> | null>(null);
  let readonly = $state(false);
  let draft = $state<Draft>({ verdict: null, note: "" });
  let error = $state("");
  let plugin: Plugin<Payload, Decision>;

  onMount(() => {
    plugin = Pinrail.connect<Payload, Decision>({
      onInit(init) {
        review = init.review;
        readonly = init.readonly;
        if (init.draft) draft = init.draft as Draft;
      },
      onCollect() {
        const { verdict, note } = draft;
        if (!verdict) {
          error = "Choose ship or hold first.";
          return;
        }
        plugin.submit(note.trim() ? { verdict, note: note.trim() } : { verdict });
      },
      onViolations(errors) {
        error = errors.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
      },
      onSubmitted() {
        review = { ...plugin.review! };
        readonly = true;
      },
    });
  });

  // State is a proxy, which a message to the app cannot carry: the SDK is
  // always handed a snapshot.
  function choose(verdict: Verdict) {
    draft.verdict = verdict;
    error = "";
    plugin.draft($state.snapshot(draft), { flush: true });
  }

  function writeNote(event: Event) {
    draft.note = (event.target as HTMLTextAreaElement).value;
    plugin.draft($state.snapshot(draft));
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
    if (!review || readonly) return;
    const label = draft.verdict === "ship" ? `Ship ${review.payload.version}` : draft.verdict === "hold" ? "Hold the deploy" : "Choose ship or hold";
    plugin.status({ label });
  });

  const decided = $derived(review?.decision?.data);
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

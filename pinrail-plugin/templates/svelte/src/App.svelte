<!-- __TITLE__: one question, yes or no, in Svelte. The SDK is connected
     once, when the view mounts; what it hands over (the review, whether it
     is read-only, the draft) becomes state. The app's hand-over button (or
     ⌘/Ctrl+Enter) asks for the decision, and onCollect returns it. Replace
     the markup and onCollect with your own.

     The SDK is on the window from the script tag in index.html; the types
     come from the package, so `review.payload` is your payload. -->
<script lang="ts">
  import { Check, X } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { Review, Plugin } from "@forgeplane/pinrail-plugin/types";

  type Payload = { message: string };
  type Decision = { ok: boolean };
  /** what is kept between reloads: the decision so far, answer still open */
  type Draft = { ok: boolean | null };

  const { Pinrail } = window;
  let review = $state<Review<Payload, Decision> | null>(null);
  let readonly = $state(false);
  let draft = $state<Draft>({ ok: null });
  let errors = $state("");
  let plugin: Plugin<Payload, Decision>;

  onMount(() => {
    plugin = Pinrail.connect<Payload, Decision>({
      onInit(init) {
        review = init.review;
        readonly = init.readonly;
        const kept = init.draft as Draft | null;
        if (kept) draft = { ok: typeof kept.ok === "boolean" ? kept.ok : null };
      },
      // the decision, or nothing while there is no answer to hand over
      onCollect() {
        const { ok } = draft;
        if (ok === null) {
          errors = "Choose yes or no first.";
          return;
        }
        return { ok };
      },
      onViolations(list) {
        errors = list.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
      },
      onSubmitted() {
        review = { ...plugin.review! };
        readonly = true;
      },
    });
  });

  function pick(value: boolean) {
    draft.ok = draft.ok === value ? null : value;
    errors = "";
    plugin.draft(draft, { flush: true });
  }

  // what the app's hand-over button says follows the answer
  $effect(() => {
    if (!review || readonly) return;
    plugin.status({ label: draft.ok === null ? "Hand over" : `Hand over: ${draft.ok ? "yes" : "no"}` });
  });

  const decided = $derived(review?.decision?.data);
</script>

{#if !review}
  <p class="plugin-content dim">waiting for the shell…</p>
{:else}
  <main class="plugin-content">
    <div>{@html Pinrail.markdown(review.payload.message)}</div>
    {#if readonly && decided}
      <p class="dim">Decided: <b>{decided.ok ? "yes" : "no"}</b></p>
    {:else if readonly}
      <!-- withdrawn or expired: nobody answered -->
      <p class="dim">Closed without a decision ({review.status})</p>
    {:else}
      <div class="choice">
        <button type="button" class="btn" id="yes" aria-pressed={draft.ok === true} onclick={() => pick(true)}>
          <Check /> Yes
        </button>
        <button type="button" class="btn" id="no" aria-pressed={draft.ok === false} onclick={() => pick(false)}>
          <X /> No
        </button>
      </div>
      <div id="errors" class="errors">{errors}</div>
    {/if}
  </main>
{/if}

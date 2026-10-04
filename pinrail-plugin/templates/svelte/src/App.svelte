<!-- __TITLE__: one question, yes or no, in Svelte. main.ts connects to the
     app and mounts this with what the app handed over (the review, whether
     it is read-only, the draft). The app's hand-over button (or
     ⌘/Ctrl+Enter) asks for the decision, and `view` answers with it.
     Replace the markup and the decision with your own.

     The SDK is on the window from the script tag in index.html; the types
     come from the package, so `review.payload` is your payload. -->
<script lang="ts">
  import { Check, X } from "@lucide/svelte";
  import { untrack } from "svelte";
  import type { Init, Plugin } from "@forgeplane/pinrail-plugin/types";
  import { draftOf, view, type Decision, type Draft, type Payload } from "./view";

  let { plugin, init }: { plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision> } = $props();

  const { Pinrail } = window;
  // the component is mounted afresh for each init, so it starts from it once
  let review = $state(untrack(() => init.review));
  let readonly = $state(untrack(() => init.readonly));
  let draft = $state<Draft>(untrack(() => draftOf(init.draft)));
  let errors = $state("");

  view.collect = () => {
    const { ok } = draft;
    if (ok === null) {
      errors = "Choose yes or no first.";
      return;
    }
    return { ok };
  };
  view.violations = (list) => {
    errors = list.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
  };
  view.submitted = () => {
    review = plugin.review!;
    readonly = true;
  };

  function pick(value: boolean) {
    draft.ok = draft.ok === value ? null : value;
    errors = "";
    plugin.draft(draft);
  }

  // what the app's hand-over button says follows the answer
  $effect(() => {
    if (readonly) return;
    plugin.handOverLabel(draft.ok === null ? "Hand over" : `Hand over: ${draft.ok ? "yes" : "no"}`);
  });

  const decided = $derived(review.decision?.data);
</script>

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

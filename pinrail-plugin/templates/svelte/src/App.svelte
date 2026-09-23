<!-- __TITLE__: one question, yes or no, with an optional comment, in
     Svelte. The SDK is connected once, when the view mounts; what it hands
     over (the review, whether it is read-only, the draft) becomes state. The
     app's hand-over button (or ⌘/Ctrl+Enter) sends `collect` and this view
     submits. Replace the markup and handOver with your own.

     The SDK is on the window from the script tag in index.html; the types
     come from the package, so `gate.payload` is your payload. -->
<script lang="ts">
  import { onMount } from "svelte";
  import type { Gate, Plugin } from "@forgeplane/pinrail-plugin/types";

  type Payload = { message: string };
  type Decision = { ok: boolean; comment?: string };
  /** what is kept between reloads: the decision so far, answer still open */
  type Draft = { ok: boolean | null; comment: string };

  const { Pinrail } = window;
  let gate = $state<Gate<Payload, Decision> | null>(null);
  let readonly = $state(false);
  let draft = $state<Draft>({ ok: null, comment: "" });
  let errors = $state("");
  let plugin: Plugin<Payload, Decision>;

  onMount(() => {
    plugin = Pinrail.connect<Payload, Decision>({
      onInit(init) {
        gate = init.gate;
        readonly = init.readonly;
        const kept = init.draft as Draft | null;
        if (kept) draft = { ok: typeof kept.ok === "boolean" ? kept.ok : null, comment: kept.comment ?? "" };
      },
      onCollect() {
        const { ok, comment } = draft;
        if (ok === null) {
          errors = "Choose yes or no first.";
          return;
        }
        plugin.submit(comment.trim() ? { ok, comment: comment.trim() } : { ok });
      },
      onViolations(list) {
        errors = list.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
      },
      onSubmitted() {
        gate = { ...plugin.gate! };
        readonly = true;
      },
    });
  });

  // State is a proxy, which a message to the app cannot carry: the SDK is
  // always handed a snapshot.
  function pick(value: boolean) {
    draft.ok = draft.ok === value ? null : value;
    errors = "";
    plugin.draft($state.snapshot(draft), { flush: true });
  }

  function writeComment(event: Event) {
    draft.comment = (event.target as HTMLInputElement).value;
    plugin.draft($state.snapshot(draft));
  }

  // what the app's hand-over button says follows the answer
  $effect(() => {
    if (!gate || readonly) return;
    plugin.status({ label: draft.ok === null ? "Hand over" : `Hand over: ${draft.ok ? "yes" : "no"}` });
  });

  const decided = $derived(gate?.decision?.data);
</script>

{#if !gate}
  <p class="plugin-content dim">waiting for the shell…</p>
{:else}
  <main class="plugin-content">
    <div>{@html Pinrail.markdown(gate.payload.message)}</div>
    {#if readonly}
      <p class="dim">Decided: <b>{decided?.ok ? "yes" : "no"}</b>{#if decided?.comment} — {decided.comment}{/if}</p>
    {:else}
      <div class="choice">
        <button type="button" class="btn" id="yes" aria-pressed={draft.ok === true} onclick={() => pick(true)}>
          {@html Pinrail.icon("check")} Yes
        </button>
        <button type="button" class="btn" id="no" aria-pressed={draft.ok === false} onclick={() => pick(false)}>
          {@html Pinrail.icon("x")} No
        </button>
      </div>
      <input class="field" id="comment" placeholder="comment (optional)" aria-label="comment" value={draft.comment} oninput={writeComment} />
      <div id="errors" class="errors">{errors}</div>
    {/if}
  </main>
{/if}

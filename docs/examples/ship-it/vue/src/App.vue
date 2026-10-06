<!-- Ship it? in Vue. main.ts connects to the app and mounts this with what
     the app handed over (the review, whether it is read-only, the draft),
     and the `view` it fills in. The app's hand-over button asks for the
     decision, and `view` answers with it. The SDK is on the window from the
     script tag in index.html; the types come from the package. -->
<script lang="ts">

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
};

/** A draft kept by an earlier release may have another shape: use only what
 *  reads as this one's. */
function draftOf(kept: unknown): Draft {
  const d = kept && typeof kept === "object" ? (kept as Partial<Draft>) : {};
  return {
    verdict: d.verdict === "ship" || d.verdict === "hold" ? d.verdict : null,
    note: typeof d.note === "string" ? d.note : "",
  };
}
</script>

<script setup lang="ts">
import { CircleCheck, CircleX, Hand, Rocket } from "@lucide/vue";
import { onBeforeUnmount, onMounted, ref, watchEffect } from "vue";
import type { Init, Plugin } from "pinrail-sdk/types";

const props = defineProps<{ plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision>; view: View }>();
// the app closes the view once the decision is accepted: what it shows is
// the review as init handed it over
const { review, readonly } = props.init;
const draft = ref<Draft>(draftOf(props.init.draft));
const error = ref("");

props.view.collect = () => {
  const { verdict, note } = draft.value;
  if (!verdict) {
    error.value = "Choose ship or hold first.";
    return;
  }
  return note.trim() ? { verdict, note: note.trim() } : { verdict };
};

onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

function choose(verdict: Verdict) {
  draft.value = { ...draft.value, verdict };
  error.value = "";
  props.plugin.draft(draft.value);
}

function writeNote(event: Event) {
  draft.value = { ...draft.value, note: (event.target as HTMLTextAreaElement).value };
  props.plugin.draft(draft.value);
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
watchEffect(() => {
  if (readonly) return;
  const verdict = draft.value.verdict;
  const label = verdict === "ship" ? `Ship ${review.payload.version}` : verdict === "hold" ? "Hold the deploy" : "Choose ship or hold";
  props.plugin.handOverLabel(label);
});

const decided = review.decision?.data;
</script>

<template>
  <main class="pinrail-content ship">
    <p class="pinrail-eyebrow">Deploy to {{ review.payload.environment }}</p>
    <h1>{{ review.payload.service }} <span class="pinrail-chip">{{ review.payload.version }}</span></h1>
    <section>
      <h2 class="pinrail-eyebrow">Changes</h2>
      <ul aria-label="Changes">
        <li v-for="c in review.payload.changes" :key="c.title">
          {{ c.title }}<template v-if="c.risky">&#32;<span class="pinrail-tone pinrail-tone-warning">risky</span></template>
        </li>
      </ul>
    </section>
    <section>
      <h2 class="pinrail-eyebrow">Checks</h2>
      <ul aria-label="Checks">
        <li v-for="c in review.payload.checks" :key="c.name" :data-passed="String(c.passed)">
          <CircleCheck v-if="c.passed" /><CircleX v-else /> {{ c.name }}<template v-if="c.detail">&#32;<span class="detail">{{ c.detail }}</span></template>
        </li>
      </ul>
    </section>
    <p v-if="readonly" class="decided">
      <b>{{ decided?.verdict === "ship" ? "Shipped" : "Held" }}</b><template v-if="decided?.note">: {{ decided.note }}</template>
    </p>
    <template v-else>
      <div class="choice" role="group" aria-label="Verdict">
        <button type="button" class="pinrail-btn" :aria-pressed="draft.verdict === 'ship'" @click="choose('ship')">
          <Rocket /> Ship <kbd>s</kbd>
        </button>
        <button type="button" class="pinrail-btn" :aria-pressed="draft.verdict === 'hold'" @click="choose('hold')">
          <Hand /> Hold <kbd>h</kbd>
        </button>
      </div>
      <textarea
        class="pinrail-note"
        aria-label="Note to the agent"
        placeholder="A note for the agent (optional)"
        :value="draft.note"
        @input="writeNote"
      ></textarea>
      <div class="pinrail-errors" role="alert">{{ error }}</div>
    </template>
  </main>
</template>

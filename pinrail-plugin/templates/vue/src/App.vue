<!-- __TITLE__: one question, yes or no, with an optional comment, in Vue.
     The SDK is connected once, when the view mounts; what it hands over (the
     review, whether it is read-only, the draft) becomes reactive state. The
     app's hand-over button (or ⌘/Ctrl+Enter) sends `collect` and this view
     submits. Replace the template and handOver with your own.

     The SDK is on the window from the script tag in index.html; the types
     come from the package, so `review.payload` is your payload. -->
<script setup lang="ts">
import { Check, X } from "@lucide/vue";
import { onMounted, ref, watchEffect } from "vue";
import type { Review, Plugin } from "@forgeplane/pinrail-plugin/types";

type Payload = { message: string };
type Decision = { ok: boolean; comment?: string };
/** what is kept between reloads: the decision so far, answer still open */
type Draft = { ok: boolean | null; comment: string };

const { Pinrail } = window;
const review = ref<Review<Payload, Decision> | null>(null);
const readonly = ref(false);
const draft = ref<Draft>({ ok: null, comment: "" });
const errors = ref("");
let plugin: Plugin<Payload, Decision>;

onMounted(() => {
  plugin = Pinrail.connect<Payload, Decision>({
    onInit(init) {
      review.value = init.review;
      readonly.value = init.readonly;
      const kept = init.draft as Draft | null;
      if (kept) draft.value = { ok: typeof kept.ok === "boolean" ? kept.ok : null, comment: kept.comment ?? "" };
    },
    onCollect() {
      const { ok, comment } = draft.value;
      if (ok === null) {
        errors.value = "Choose yes or no first.";
        return;
      }
      plugin.submit(comment.trim() ? { ok, comment: comment.trim() } : { ok });
    },
    onViolations(list) {
      errors.value = list.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
    },
    onSubmitted() {
      review.value = { ...plugin.review! };
      readonly.value = true;
    },
  });
});

function pick(value: boolean) {
  draft.value = { ...draft.value, ok: draft.value.ok === value ? null : value };
  errors.value = "";
  plugin.draft(draft.value, { flush: true });
}

function writeComment(event: Event) {
  draft.value = { ...draft.value, comment: (event.target as HTMLInputElement).value };
  plugin.draft(draft.value);
}

// what the app's hand-over button says follows the answer
watchEffect(() => {
  if (!review.value || readonly.value) return;
  const ok = draft.value.ok;
  plugin.status({ label: ok === null ? "Hand over" : `Hand over: ${ok ? "yes" : "no"}` });
});

const markdown = (source: string) => Pinrail.markdown(source);
</script>

<template>
  <p v-if="!review" class="plugin-content dim">waiting for the shell…</p>
  <main v-else class="plugin-content">
    <div v-html="markdown(review.payload.message)"></div>
    <p v-if="readonly && review.decision" class="dim">
      Decided: <b>{{ review.decision.data?.ok ? "yes" : "no" }}</b><template v-if="review.decision.data?.comment"> — {{ review.decision.data.comment }}</template>
    </p>
    <!-- withdrawn or expired: nobody answered -->
    <p v-else-if="readonly" class="dim">Closed without a decision ({{ review.status }})</p>
    <template v-else>
      <div class="choice">
        <button type="button" class="btn" id="yes" :aria-pressed="draft.ok === true" @click="pick(true)">
          <Check /> Yes
        </button>
        <button type="button" class="btn" id="no" :aria-pressed="draft.ok === false" @click="pick(false)">
          <X /> No
        </button>
      </div>
      <input class="field" id="comment" placeholder="comment (optional)" aria-label="comment" :value="draft.comment" @input="writeComment" />
      <div id="errors" class="errors">{{ errors }}</div>
    </template>
  </main>
</template>

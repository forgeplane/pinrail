<!-- __TITLE__: one question, yes or no, in Vue.
     main.ts connects to the app and mounts this with what the app handed
     over (the review, whether it is read-only, the draft). The app's
     hand-over button (or ⌘/Ctrl+Enter) asks for the decision, and `view`
     answers with it. Replace the template and the decision with your own.

     The SDK is on the window from the script tag in index.html; the types
     come from the package, so `review.payload` is your payload. -->
<script setup lang="ts">
import { Check, X } from "@lucide/vue";
import { ref, shallowRef, watchEffect } from "vue";
import type { Init, Plugin } from "@forgeplane/pinrail-plugin/types";
import { draftOf, view, type Decision, type Draft, type Payload } from "./view";

const props = defineProps<{ plugin: Plugin<Payload, Decision>; init: Init<Payload, Decision> }>();
const { Pinrail } = window;
const review = shallowRef(props.init.review);
const readonly = ref(props.init.readonly);
const draft = ref<Draft>(draftOf(props.init.draft));
const errors = ref("");

view.collect = () => {
  const { ok } = draft.value;
  if (ok === null) {
    errors.value = "Choose yes or no first.";
    return;
  }
  return { ok };
};
view.violations = (list) => {
  errors.value = list.map((e) => `${e.path || "/"}: ${e.message}`).join("\n");
};
view.submitted = () => {
  review.value = props.plugin.review!;
  readonly.value = true;
};

function pick(value: boolean) {
  draft.value = { ok: draft.value.ok === value ? null : value };
  errors.value = "";
  props.plugin.draft(draft.value);
}

// what the app's hand-over button says follows the answer
watchEffect(() => {
  if (readonly.value) return;
  const ok = draft.value.ok;
  props.plugin.handOverLabel(ok === null ? "Hand over" : `Hand over: ${ok ? "yes" : "no"}`);
});

const markdown = (source: string) => Pinrail.markdown(source);
</script>

<template>
  <main class="plugin-content">
    <div v-html="markdown(review.payload.message)"></div>
    <p v-if="readonly && review.decision" class="dim">
      Decided: <b>{{ review.decision.data?.ok ? "yes" : "no" }}</b>
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
      <div id="errors" class="errors">{{ errors }}</div>
    </template>
  </main>
</template>

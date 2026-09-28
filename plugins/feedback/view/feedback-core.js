(function (root) {
  "use strict";
  const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
  const types = ["single_choice", "multiple_choice", "text", "boolean", "checkbox"];
  const questions = (p) => p.groups.flatMap((g) => g.questions);
  const blank = (v) => v === undefined || v === null || (typeof v === "string" && !v.trim());
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  function errorFor(q, value) {
    if (blank(value)) return q.required ? "Please answer this question." : null;
    if (q.type === "single_choice")
      return q.options.some((o) => o.id === value) ? null : "Choose one of the available options.";
    if (q.type === "multiple_choice") {
      if (
        !Array.isArray(value) ||
        value.some((v) => !q.options.some((o) => o.id === v)) ||
        new Set(value).size !== value.length
      )
        return "Choose only the available options.";
      const min = q.min_selections ?? (q.required ? 1 : 0),
        max = q.max_selections ?? q.options.length;
      if (value.length < min) return `Choose at least ${min} ${min === 1 ? "option" : "options"}.`;
      if (value.length > max) return `Choose at most ${max} ${max === 1 ? "option" : "options"}.`;
    } else if (q.type === "text") {
      if (typeof value !== "string") return "Enter a text response.";
      if (value.trim().length < (q.min_length ?? 0)) return `Use at least ${q.min_length} characters.`;
      if (value.length > (q.max_length ?? 10000)) return `Use at most ${q.max_length ?? 10000} characters.`;
    } else if (["boolean", "checkbox"].includes(q.type)) {
      if (typeof value !== "boolean") return "Choose yes or no.";
      if (q.type === "checkbox" && q.required && value !== true)
        return "Confirm this acknowledgment before handing over.";
    }
    return null;
  }
  const answered = (q, value) => !blank(value) && !errorFor(q, value);
  function validate(p) {
    if (!p || !Array.isArray(p.groups) || p.groups.length === 0)
      throw new Error("Provide at least one question group.");
    const groupIds = new Set(),
      known = new Map();
    const id = (value) => typeof value === "string" && /^[A-Za-z][A-Za-z0-9_-]{0,79}$/.test(value);
    function checkCondition(c, depth = 0) {
      if (!c || typeof c !== "object" || depth > 8) throw new Error("Invalid or excessively nested condition.");
      if (own(c, "all") || own(c, "any")) {
        const keys = Object.keys(c),
          entries = c.all || c.any;
        if (keys.length !== 1 || !Array.isArray(entries) || !entries.length || entries.length > 20)
          throw new Error("Conditions need a nonempty all or any list.");
        entries.forEach((child) => checkCondition(child, depth + 1));
        return;
      }
      const previous = known.get(c.question_id);
      if (!previous)
        throw new Error(
          `Condition references a missing or later question: ${c.question_id}. Conditions can only use earlier questions.`,
        );
      if (!["equals", "not_equals", "contains", "answered"].includes(c.operator))
        throw new Error("Unknown condition operator.");
      if (c.operator === "answered") {
        if (own(c, "value")) throw new Error("The answered operator does not take a value.");
        return;
      }
      if (!own(c, "value")) throw new Error("This condition needs a value.");
      if (c.operator === "contains") {
        if (previous.type !== "multiple_choice" || !previous.options.some((o) => o.id === c.value))
          throw new Error("contains must reference an option in an earlier multiple-choice question.");
      } else {
        if (previous.type === "multiple_choice") throw new Error("Use contains for multiple-choice conditions.");
        if (previous.type === "single_choice" && !previous.options.some((o) => o.id === c.value))
          throw new Error("Condition refers to an unknown choice.");
        if (["boolean", "checkbox"].includes(previous.type) && typeof c.value !== "boolean")
          throw new Error("Boolean conditions need true or false.");
        if (previous.type === "text" && typeof c.value !== "string") throw new Error("Text conditions need a string.");
      }
    }
    for (const g of p.groups) {
      if (
        !id(g.id) ||
        groupIds.has(g.id) ||
        typeof g.title !== "string" ||
        !Array.isArray(g.questions) ||
        !g.questions.length
      )
        throw new Error("Groups need unique IDs, titles, and questions.");
      groupIds.add(g.id);
      if (g.when) checkCondition(g.when);
      for (const q of g.questions) {
        if (!id(q.id) || known.has(q.id) || typeof q.prompt !== "string" || !q.prompt.trim() || !types.includes(q.type))
          throw new Error("Questions need unique IDs, nonempty prompts, and supported types.");
        if (q.required !== undefined && typeof q.required !== "boolean")
          throw new Error("required must be true or false.");
        if (["single_choice", "multiple_choice"].includes(q.type)) {
          if (
            !Array.isArray(q.options) ||
            !q.options.length ||
            q.options.some((o) => !id(o.id) || typeof o.label !== "string" || !o.label.trim()) ||
            new Set(q.options.map((o) => o.id)).size !== q.options.length
          )
            throw new Error(`Question ${q.id} needs unique, labeled options.`);
        } else if (q.options !== undefined) throw new Error(`Question ${q.id} cannot have options.`);
        for (const key of ["min_selections", "max_selections", "min_length", "max_length"]) {
          if (q[key] !== undefined && (!Number.isInteger(q[key]) || q[key] < 0))
            throw new Error(`${key} must be a nonnegative integer.`);
          if (q[key] !== undefined && q.type !== (key.endsWith("selections") ? "multiple_choice" : "text"))
            throw new Error(`${key} is not supported for ${q.type}.`);
        }
        if (q.type === "multiple_choice") {
          const min = q.min_selections ?? (q.required ? 1 : 0),
            max = q.max_selections ?? q.options.length;
          if (min > max || max > q.options.length || (q.required && min === 0))
            throw new Error("Selection limits are inconsistent.");
        }
        if (q.type === "text" && (q.min_length ?? 0) > (q.max_length ?? 10000))
          throw new Error("Text limits are inconsistent.");
        if (q.when) checkCondition(q.when);
        if (q.recommendation && (blank(q.recommendation.answer) || errorFor(q, q.recommendation.answer)))
          throw new Error(`Invalid recommendation for ${q.id}.`);
        known.set(q.id, q);
      }
    }
    if (known.size > 100) throw new Error("A feedback request supports at most 100 questions.");
    return p;
  }
  function matches(c, effective, byId) {
    if (!c) return true;
    if (c.all) return c.all.every((x) => matches(x, effective, byId));
    if (c.any) return c.any.some((x) => matches(x, effective, byId));
    // An unanswered or hidden parent never enables its descendants, including not_equals.
    if (!own(effective, c.question_id)) return false;
    const value = effective[c.question_id];
    if (!answered(byId.get(c.question_id), value)) return false;
    if (c.operator === "answered") return true;
    if (c.operator === "equals") return same(value, c.value);
    if (c.operator === "not_equals") return !same(value, c.value);
    if (c.operator === "contains") return Array.isArray(value) && value.includes(c.value);
    return false;
  }
  function visible(p, state) {
    const byId = new Map(questions(p).map((q) => [q.id, q])),
      effective = Object.create(null),
      ids = new Set();
    for (const g of p.groups) {
      const groupVisible = matches(g.when, effective, byId);
      for (const q of g.questions) {
        if (groupVisible && matches(q.when, effective, byId)) {
          ids.add(q.id);
          if (own(state.values, q.id)) effective[q.id] = state.values[q.id];
        }
      }
    }
    return ids;
  }
  function restore(p, raw) {
    const values = Object.create(null),
      comments = Object.create(null);
    const entries = Array.isArray(raw?.answers) ? raw.answers : [];
    const source = raw?.values || Object.fromEntries(entries.map((e) => [e.question_id, e.answer]));
    const notes = raw?.comments || Object.fromEntries(entries.map((e) => [e.question_id, e.comment]));
    for (const q of questions(p)) {
      const value = source[q.id];
      if (q.type === "text" && typeof value === "string") values[q.id] = value;
      if (["boolean", "checkbox"].includes(q.type) && typeof value === "boolean") values[q.id] = value;
      if (q.type === "single_choice" && q.options.some((o) => o.id === value)) values[q.id] = value;
      if (q.type === "multiple_choice" && Array.isArray(value))
        values[q.id] = q.options.filter((o) => value.includes(o.id)).map((o) => o.id);
      if (q.type !== "text" && typeof notes[q.id] === "string") comments[q.id] = notes[q.id];
    }
    return { values, comments };
  }
  function errors(p, state) {
    const shown = visible(p, state),
      out = Object.create(null);
    for (const q of questions(p))
      if (shown.has(q.id)) {
        const error = errorFor(q, state.values[q.id]);
        if (error) out[q.id] = error;
        if (q.type !== "text" && (state.comments[q.id] || "").length > 5000)
          out[q.id] = "Keep the comment under 5,000 characters.";
      }
    return out;
  }
  function decision(p, state) {
    const invalid = errors(p, state);
    if (Object.keys(invalid).length) {
      const e = new Error("Complete the highlighted questions before handing over.");
      e.questions = invalid;
      throw e;
    }
    const shown = visible(p, state),
      answers = [],
      unanswered = [],
      excluded = [];
    for (const q of questions(p)) {
      if (!shown.has(q.id)) {
        excluded.push(q.id);
        continue;
      }
      const value = state.values[q.id],
        hasAnswer = answered(q, value),
        comment = q.type === "text" ? "" : (state.comments[q.id] || "").trim();
      if (!hasAnswer) unanswered.push(q.id);
      if (hasAnswer || comment)
        answers.push({
          question_id: q.id,
          answer: hasAnswer ? (typeof value === "string" ? value.trim() : value) : null,
          comment,
        });
    }
    return { answers, unanswered, excluded };
  }
  function describe(q, value) {
    if (blank(value)) return "Not answered";
    if (q.type === "single_choice") return q.options.find((o) => o.id === value)?.label || String(value);
    if (q.type === "multiple_choice")
      return value.map((id) => q.options.find((o) => o.id === id)?.label || id).join(", ") || "None selected";
    if (q.type === "checkbox") return value ? "Confirmed" : "Not confirmed";
    if (q.type === "boolean") return value ? "Yes" : "No";
    return String(value);
  }
  const api = { validate, questions, visible, restore, errors, decision, answered, describe };
  if (typeof module !== "undefined") module.exports = api;
  else root.FeedbackCore = api;
})(globalThis);

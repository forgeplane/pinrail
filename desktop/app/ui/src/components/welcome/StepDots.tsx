// Where the person is in a short flow: a dot for each step, the current one
// wider.

export function StepDots({ count, current }: { count: number; current: number }) {
  return (
    <ol className="step-dots" aria-label={`Step ${current + 1} of ${count}`}>
      {Array.from({ length: count }, (_, i) => (
        <li key={i} className={i === current ? "is-current" : ""} aria-current={i === current ? "step" : undefined} />
      ))}
    </ol>
  );
}

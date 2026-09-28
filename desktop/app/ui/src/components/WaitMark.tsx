// The app's mark while something loads: the slash swings on the rail,
// between the two pins. It takes the text colour, so it suits either theme.

export function WaitMark({ size = 24 }: { size?: number }) {
  return (
    <svg className="wait-mark" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
      <rect x="1.5" y="13" width="21" height="3.4" rx="1.4" fill="currentColor" />
      <rect x="4.8" y="5" width="2.4" height="16" rx="1.2" fill="currentColor" />
      <rect x="16.8" y="5" width="2.4" height="16" rx="1.2" fill="currentColor" />
      <rect className="wait-mark-slash" x="10.8" y="4.2" width="2.4" height="16.8" rx="1.2" fill="#e5694f" />
    </svg>
  );
}

// The app's mark: pin, slash, pin.

export function Mark({ size }: { size: number }) {
  return (
    <svg className="setup-mark" width={size} height={size} viewBox="0 0 824 824" aria-hidden="true">
      <rect width="824" height="824" rx="185" fill="#221f1c" />
      <rect
        x="6"
        y="6"
        width="812"
        height="812"
        rx="180"
        fill="none"
        stroke="#f1ebdf"
        strokeOpacity="0.12"
        strokeWidth="12"
      />
      <g transform="translate(412 412) scale(24) translate(-12 -12.2)">
        <rect x="1.5" y="13" width="21" height="3.4" rx="1.4" fill="#f1ebdf" />
        <rect x="4.8" y="5" width="2.4" height="16" rx="1.2" fill="#f1ebdf" />
        <rect x="16.8" y="5" width="2.4" height="16" rx="1.2" fill="#f1ebdf" />
        <g transform="rotate(20 12 14.7)">
          <rect x="10.8" y="4.2" width="2.4" height="16.8" rx="1.2" fill="#e5694f" />
        </g>
      </g>
    </svg>
  );
}

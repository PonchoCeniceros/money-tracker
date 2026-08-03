// Small inline action icons, in the spirit of icon-only row actions (no
// "Editar"/"Borrar" text) — hand-drawn primitives instead of pulling in an
// icon library, matching this project's own-SVG approach (see DonutChart).
interface IconProps {
  size?: number;
}

export function PencilIcon({ size = 16 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <g transform="rotate(45 10 10)">
        <rect x="8.5" y="2" width="3" height="11" rx="1" fill="currentColor" />
        <rect x="8.5" y="13" width="3" height="2.5" fill="currentColor" opacity="0.5" />
        <polygon points="8.5,15.5 11.5,15.5 10,18" fill="currentColor" />
      </g>
    </svg>
  );
}

export function TrashIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4 6h12" />
      <path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V6" />
      <path d="M5.5 6l.7 10a1.5 1.5 0 0 0 1.5 1.4h4.6a1.5 1.5 0 0 0 1.5-1.4L14.5 6" />
      <path d="M8.3 9v5" />
      <path d="M11.7 9v5" />
    </svg>
  );
}

// Deposit: an arrow landing on a tray — money going into a bucket/account.
export function DepositIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M10 3v9" />
      <path d="M6.5 8.5 10 12l3.5-3.5" />
      <path d="M4 14.5h12" />
    </svg>
  );
}

// Withdraw: the mirror of deposit — an arrow leaving a tray.
export function WithdrawIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M10 17V8" />
      <path d="M6.5 11.5 10 8l3.5 3.5" />
      <path d="M4 5.5h12" />
    </svg>
  );
}

// Reconcile: a check inside a circle — "this balance is verified".
export function CheckCircleIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="10" cy="10" r="7" />
      <path d="M7 10.2 9 12.3 13 7.7" />
    </svg>
  );
}

// Archive: a lidded box.
export function ArchiveIcon({ size = 16 }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="3.5" y="5" width="13" height="3" rx="0.8" />
      <path d="M4.5 8v6.5a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V8" />
      <path d="M8 11h4" />
    </svg>
  );
}

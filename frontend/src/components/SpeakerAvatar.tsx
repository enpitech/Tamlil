'use client';

/**
 * One small monster face per speaker. The set is drawn here (no external
 * assets), so it is royalty-free by construction. Each speaker index gets the
 * next face and colour; "You" always gets the first one.
 */

const PALETTE = [
  { bg: '#F6C343', ink: '#1f2937' }, // yellow
  { bg: '#5BB974', ink: '#0f172a' }, // green
  { bg: '#E8574B', ink: '#1f2937' }, // red
  { bg: '#4BA3E3', ink: '#0f172a' }, // blue
  { bg: '#B388EB', ink: '#1f2937' }, // purple
  { bg: '#F59E0B', ink: '#1f2937' }, // orange
  { bg: '#F4A6B7', ink: '#1f2937' }, // pink
  { bg: '#2F855A', ink: '#0f172a' }, // dark green
  { bg: '#7DD3FC', ink: '#0f172a' }, // sky
  { bg: '#A3A3A3', ink: '#1f2937' }, // grey
  { bg: '#C026D3', ink: '#1f2937' }, // magenta
  { bg: '#0EA5A4', ink: '#0f172a' }, // teal
];

export function speakerColor(index: number): { bg: string; ink: string } {
  return PALETTE[((index % PALETTE.length) + PALETTE.length) % PALETTE.length];
}

/** Eye + mouth variants, keyed by speaker index. Coordinates are in a 40×40 box. */
function Face({ variant, ink }: { variant: number; ink: string }) {
  const eye = (cx: number, cy: number, r = 5) => (
    <g key={`${cx}-${cy}`}>
      <circle cx={cx} cy={cy} r={r} fill="#fff" />
      <circle cx={cx + r * 0.25} cy={cy} r={r * 0.45} fill={ink} />
    </g>
  );
  switch (variant % 12) {
    case 0: // three eyes, wide smile
      return (
        <>
          {eye(12, 15, 4)}{eye(20, 13, 4)}{eye(28, 15, 4)}
          <path d="M11 25q9 8 18 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
    case 1: // one big eye, tongue
      return (
        <>
          {eye(20, 15, 8)}
          <path d="M12 26q8 6 16 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
          <ellipse cx="22" cy="30" rx="3" ry="3.5" fill="#ef4444" />
        </>
      );
    case 2: // two eyes, wavy mouth
      return (
        <>
          {eye(13, 15)}{eye(27, 15)}
          <path d="M11 27c3-3 6 3 9 0s6 3 9 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
    case 3: // raised brows, cat mouth
      return (
        <>
          <path d="M9 9l6-2M31 9l-6-2" stroke={ink} strokeWidth="2.5" strokeLinecap="round" />
          {eye(13, 16)}{eye(27, 16)}
          <path d="M15 27q5-4 10 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
          <path d="M20 23v4" stroke={ink} strokeWidth="2.5" strokeLinecap="round" />
        </>
      );
    case 4: // surprised "o"
      return (
        <>
          {eye(13, 14)}{eye(27, 14)}
          <ellipse cx="20" cy="28" rx="4" ry="5" fill={ink} />
          <ellipse cx="20" cy="30" rx="2.5" ry="2" fill="#ef4444" />
        </>
      );
    case 5: // fangs
      return (
        <>
          {eye(13, 15)}{eye(27, 15)}
          <path d="M10 26q10 6 20 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
          <path d="M14 27l2 5 2-5M22 27l2 5 2-5" fill="#fff" stroke={ink} strokeWidth="1" />
        </>
      );
    case 6: // kawaii, tiny mouth and blush
      return (
        <>
          {eye(13, 16, 5.5)}{eye(27, 16, 5.5)}
          <circle cx="9" cy="23" r="2.5" fill="#fb7185" opacity="0.7" />
          <circle cx="31" cy="23" r="2.5" fill="#fb7185" opacity="0.7" />
          <path d="M17 27q3 3 6 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
    case 7: // tall grin with teeth
      return (
        <>
          {eye(13, 13, 4.5)}{eye(27, 13, 4.5)}
          <path d="M11 22h18v4a9 5 0 0 1-18 0z" fill={ink} />
          <rect x="14" y="22" width="12" height="3" fill="#fff" />
        </>
      );
    case 8: // sleepy, half-closed
      return (
        <>
          <path d="M8 15q5-4 10 0M22 15q5-4 10 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
          <path d="M14 26q6 4 12 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
    case 9: // one eye, side grin
      return (
        <>
          {eye(20, 16, 7)}
          <path d="M12 27q10 7 17-2" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
    case 10: // angry brows
      return (
        <>
          <path d="M9 9l7 3M31 9l-7 3" stroke={ink} strokeWidth="2.5" strokeLinecap="round" />
          {eye(13, 16)}{eye(27, 16)}
          <path d="M13 28q7-4 14 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
    default: // four eyes
      return (
        <>
          {eye(10, 13, 3.5)}{eye(17, 11, 3.5)}{eye(24, 11, 3.5)}{eye(31, 13, 3.5)}
          <path d="M11 25q9 7 18 0" stroke={ink} strokeWidth="2.5" fill="none" strokeLinecap="round" />
        </>
      );
  }
}

interface SpeakerAvatarProps {
  index: number;
  label?: string;
  size?: number;
  className?: string;
}

export function SpeakerAvatar({ index, label, size = 24, className = '' }: SpeakerAvatarProps) {
  const { bg, ink } = speakerColor(index);
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 40 40"
      role="img"
      aria-label={label ?? `Speaker ${index + 1}`}
      className={`flex-shrink-0 ${className}`}
    >
      <title>{label ?? `Speaker ${index + 1}`}</title>
      <rect width="40" height="40" rx="9" fill={bg} />
      <Face variant={index} ink={ink} />
    </svg>
  );
}

/** Avatar plus name, used at the start of a transcript line. */
export function SpeakerChip({ index, label, size = 22 }: { index: number; label: string; size?: number }) {
  const { bg } = speakerColor(index);
  return (
    <span className="inline-flex items-center gap-1.5 flex-shrink-0 align-middle" title={label}>
      <SpeakerAvatar index={index} label={label} size={size} />
      <span className="text-xs font-semibold" style={{ color: bg === '#F6C343' || bg === '#7DD3FC' ? '#92400e' : bg }}>
        {label}
      </span>
    </span>
  );
}

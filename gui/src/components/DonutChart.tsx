import { useState } from "react";
import { formatMoney } from "./Money";
import styles from "./DonutChart.module.css";

export interface DonutSlice {
  label: string;
  value: number;
  color: string;
}

const SIZE = 168;
const STROKE = 24;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
const GAP = 3;

export function DonutChart({
  title,
  subtitle,
  slices,
}: {
  title: string;
  subtitle?: string;
  slices: DonutSlice[];
}) {
  const [hovered, setHovered] = useState<string | null>(null);
  const visible = slices.filter((s) => s.value > 0);
  const total = visible.reduce((sum, s) => sum + s.value, 0);

  let cumulative = 0;

  return (
    <div className={styles.card}>
      <h3>{title}</h3>
      {subtitle && <p className={styles.subtitle}>{subtitle}</p>}
      {total <= 0 ? (
        <p className={styles.empty}>Sin datos para este período.</p>
      ) : (
        <div className={styles.body}>
          <svg
            width={SIZE}
            height={SIZE}
            viewBox={`0 0 ${SIZE} ${SIZE}`}
            role="img"
            aria-label={`${title}: ${visible.map((s) => `${s.label} ${((s.value / total) * 100).toFixed(0)}%`).join(", ")}`}
          >
            <circle
              cx={SIZE / 2}
              cy={SIZE / 2}
              r={RADIUS}
              fill="none"
              className={styles.track}
              strokeWidth={STROKE}
            />
            {visible.map((s) => {
              const frac = s.value / total;
              const len = Math.max(frac * CIRCUMFERENCE - GAP, 0);
              const dashoffset = -cumulative;
              cumulative += frac * CIRCUMFERENCE;
              const isHovered = hovered === s.label;
              const isDimmed = hovered !== null && !isHovered;
              return (
                <circle
                  key={s.label}
                  cx={SIZE / 2}
                  cy={SIZE / 2}
                  r={RADIUS}
                  fill="none"
                  strokeWidth={isHovered ? STROKE + 4 : STROKE}
                  strokeDasharray={`${len} ${CIRCUMFERENCE - len}`}
                  strokeDashoffset={dashoffset}
                  transform={`rotate(-90 ${SIZE / 2} ${SIZE / 2})`}
                  style={{ stroke: s.color, opacity: isDimmed ? 0.35 : 1 }}
                  className={styles.slice}
                  onMouseEnter={() => setHovered(s.label)}
                  onMouseLeave={() => setHovered(null)}
                >
                  <title>
                    {s.label}: {formatMoney(s.value)} ({(frac * 100).toFixed(0)}%)
                  </title>
                </circle>
              );
            })}
          </svg>
          <ul className={styles.legend}>
            {visible.map((s) => (
              <li
                key={s.label}
                className={hovered === s.label ? styles.legendItemActive : styles.legendItem}
                onMouseEnter={() => setHovered(s.label)}
                onMouseLeave={() => setHovered(null)}
              >
                <span className={styles.swatch} style={{ background: s.color }} />
                <span className={styles.legendLabel}>{s.label}</span>
                <span className={styles.legendValue}>
                  {formatMoney(s.value)} · {((s.value / total) * 100).toFixed(0)}%
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

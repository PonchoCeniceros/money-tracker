// Fixed categorical order — must match the circular slice order the palette
// was validated against (see --series-* in index.css). Never reassign; a
// concept not in this map folds into "Otros" (--series-other) instead of
// growing the palette.
const COLOR_MAP: Record<string, string> = {
  Alimentos: "var(--series-1)",
  Transporte: "var(--series-2)",
  Servicios: "var(--series-3)",
  Discrecional: "var(--series-4)",
  Extraordinario: "var(--series-5)",
  "Fondo de emergencia": "var(--series-6)",
  "Metas de ahorro": "var(--series-7)",
};

export const CONCEPT_ORDER = Object.keys(COLOR_MAP);

export function colorForConcept(concept: string): string {
  return COLOR_MAP[concept] ?? "var(--series-other)";
}

import { useState } from "react";
import { budgetsApi } from "../api/budgets";
import { conceptsApi } from "../api/concepts";
import { reportApi } from "../api/report";
import { accountsApi } from "../api/accounts";
import { entriesApi } from "../api/entries";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import { Money } from "../components/Money";
import { DonutChart } from "../components/DonutChart";
import { colorForConcept, CONCEPT_ORDER } from "../components/chartColors";
import { TrashIcon } from "../components/icons";
import ui from "../components/ui.module.css";

// Fixed circular order the palette was validated against — anything not
// listed here (a custom concept, or "Ahorro") sorts after, in the order it
// was first seen, so slot adjacency in the donut never changes at random.
function bySeriesOrder(a: { label: string }, b: { label: string }): number {
  const ia = CONCEPT_ORDER.indexOf(a.label);
  const ib = CONCEPT_ORDER.indexOf(b.label);
  if (ia === -1 && ib === -1) return 0;
  if (ia === -1) return 1;
  if (ib === -1) return -1;
  return ia - ib;
}

function currentPeriod(): string {
  return new Date().toISOString().slice(0, 7);
}

export default function Budgets() {
  const [period, setPeriod] = useState(currentPeriod());
  const budgets = useApi(() => budgetsApi.list(period));
  const concepts = useApi(() => conceptsApi.list("expense"));
  const report = useApi(() => reportApi.monthly(period));
  const accounts = useApi(() => accountsApi.list());
  const transfers = useApi(() => entriesApi.list({ period, kind: "transfer" }));

  // savings_contributions lumps every spending->emergency/target transfer
  // together; split it back out by destination account kind so the
  // automatic emergency-fund split (config.emergency_pct) shows up as its
  // own slice instead of hiding inside one generic "Ahorro" bucket.
  const accountKindByName = new Map((accounts.data ?? []).map((a) => [a.name, a.kind]));
  const emergencyContribution = (transfers.data ?? [])
    .filter((e) => e.to_account && accountKindByName.get(e.to_account) === "emergency")
    .reduce((sum, e) => sum + e.amount, 0);
  const targetContribution = (transfers.data ?? [])
    .filter((e) => e.to_account && accountKindByName.get(e.to_account) === "target")
    .reduce((sum, e) => sum + e.amount, 0);

  const [concept, setConcept] = useState("");
  const [limit, setLimit] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (!concept || !limit) {
      setError("Falta el concepto o el límite.");
      return;
    }
    setBusy(true);
    try {
      await budgetsApi.set({ concept, monthly_limit: Number(limit), period });
      setLimit("");
      bumpRevision();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(c: string) {
    try {
      await budgetsApi.remove(c, period);
      bumpRevision();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div>
      <div className={ui.row} style={{ justifyContent: "space-between" }}>
        <h1>Presupuestos</h1>
        <input
          type="month"
          className={ui.input}
          value={period}
          onChange={(e) => setPeriod(e.target.value)}
        />
      </div>
      <p className={ui.muted}>Informativo únicamente — nunca bloquea un gasto.</p>

      <ErrorBanner message={report.error ?? accounts.error ?? transfers.error} />
      {report.data && (
        <div className={ui.row} style={{ alignItems: "stretch", marginBottom: 16 }}>
          <DonutChart
            title="Distribución del presupuesto"
            subtitle="Lo que planeaste para este período"
            slices={report.data.budgets
              .map((b) => ({ label: b.concept, value: b.budgeted, color: colorForConcept(b.concept) }))
              .sort(bySeriesOrder)}
          />
          <DonutChart
            title="Distribución real"
            subtitle="Gasto por concepto + aportes a ahorro, este período"
            slices={[
              ...report.data.by_concept.map((c) => ({
                label: c.concept,
                value: c.total,
                color: colorForConcept(c.concept),
              })),
              ...(emergencyContribution > 0
                ? [
                    {
                      label: "Fondo de emergencia",
                      value: emergencyContribution,
                      color: colorForConcept("Fondo de emergencia"),
                    },
                  ]
                : []),
              ...(targetContribution > 0
                ? [{ label: "Metas de ahorro", value: targetContribution, color: colorForConcept("Metas de ahorro") }]
                : []),
            ].sort(bySeriesOrder)}
          />
        </div>
      )}

      <form className={ui.card} onSubmit={submit}>
        <ErrorBanner message={error} />
        <div className={ui.row}>
          <Field label="Concepto">
            <select
              className={ui.input}
              value={concept}
              onChange={(e) => setConcept(e.target.value)}
            >
              <option value="" disabled>
                Elegir…
              </option>
              {concepts.data?.map((c) => (
                <option key={c.name} value={c.name}>
                  {c.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Límite mensual ($)">
            <input
              className={ui.input}
              type="number"
              step="0.01"
              min="0"
              value={limit}
              onChange={(e) => setLimit(e.target.value)}
            />
          </Field>
          <button className={ui.button} disabled={busy} type="submit" style={{ alignSelf: "end" }}>
            Guardar
          </button>
        </div>
      </form>

      <ErrorBanner message={budgets.error} />
      <div className={ui.card}>
        <table className={ui.table}>
          <thead>
            <tr>
              <th>Concepto</th>
              <th>Límite</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {budgets.data?.map((b) => (
              <tr key={b.concept}>
                <td>{b.concept}</td>
                <td>
                  <Money amount={b.monthly_limit} />
                </td>
                <td>
                  <button
                    className={ui.iconButtonDanger}
                    title="Quitar"
                    aria-label="Quitar"
                    onClick={() => remove(b.concept)}
                  >
                    <TrashIcon />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {budgets.data && budgets.data.length === 0 && (
          <p className={ui.muted}>Sin presupuestos para {period}.</p>
        )}
      </div>
    </div>
  );
}

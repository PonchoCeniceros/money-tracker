import { useState } from "react";
import { budgetsApi } from "../api/budgets";
import { conceptsApi } from "../api/concepts";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import { Money } from "../components/Money";
import ui from "../components/ui.module.css";

function currentPeriod(): string {
  return new Date().toISOString().slice(0, 7);
}

export default function Budgets() {
  const [period, setPeriod] = useState(currentPeriod());
  const budgets = useApi(() => budgetsApi.list(period));
  const concepts = useApi(() => conceptsApi.list("expense"));

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
                  <button className={ui.buttonDanger} onClick={() => remove(b.concept)}>
                    Quitar
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

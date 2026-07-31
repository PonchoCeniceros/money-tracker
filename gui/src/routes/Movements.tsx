import { useState } from "react";
import { entriesApi } from "../api/entries";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import { Money } from "../components/Money";
import ui from "../components/ui.module.css";

function currentPeriod(): string {
  return new Date().toISOString().slice(0, 7);
}

export default function Movements() {
  const [period, setPeriod] = useState(currentPeriod());
  const [kind, setKind] = useState("");
  const [concept, setConcept] = useState("");

  const entries = useApi(() =>
    entriesApi.list({
      period: period || null,
      kind: kind || null,
      concept: concept || null,
    })
  );

  async function remove(id: number) {
    if (!confirm(`¿Borrar la entrada #${id}?`)) return;
    try {
      await entriesApi.remove(id);
      bumpRevision();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div>
      <h1>Movimientos</h1>
      <div className={ui.card}>
        <div className={ui.grid}>
          <Field label="Período">
            <input
              className={ui.input}
              type="month"
              value={period}
              onChange={(e) => setPeriod(e.target.value)}
            />
          </Field>
          <Field label="Tipo">
            <select className={ui.input} value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="">Todos</option>
              <option value="income">Ingreso</option>
              <option value="expense">Gasto</option>
              <option value="transfer">Transferencia</option>
              <option value="opening">Saldo inicial</option>
            </select>
          </Field>
          <Field label="Concepto">
            <input
              className={ui.input}
              value={concept}
              onChange={(e) => setConcept(e.target.value)}
              placeholder="(cualquiera)"
            />
          </Field>
        </div>
      </div>

      <ErrorBanner message={entries.error} />

      <div className={ui.card}>
        <table className={ui.table}>
          <thead>
            <tr>
              <th>#</th>
              <th>Fecha</th>
              <th>Tipo</th>
              <th>Monto</th>
              <th>De</th>
              <th>A</th>
              <th>Concepto</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {entries.data?.map((e) => (
              <tr key={e.id}>
                <td>#{e.id}</td>
                <td>{e.date}</td>
                <td>{e.kind}</td>
                <td>
                  <Money amount={e.amount} />
                </td>
                <td>{e.from_account ?? "—"}</td>
                <td>{e.to_account ?? "—"}</td>
                <td>{e.concept ?? "—"}</td>
                <td>
                  <button className={ui.buttonDanger} onClick={() => remove(e.id)}>
                    Borrar
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {entries.data && entries.data.length === 0 && (
          <p className={ui.muted}>No hay movimientos con estos filtros.</p>
        )}
      </div>
    </div>
  );
}

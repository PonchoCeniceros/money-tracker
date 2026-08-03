import { useState } from "react";
import { entriesApi } from "../api/entries";
import { accountsApi } from "../api/accounts";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import { Money } from "../components/Money";
import { PencilIcon, TrashIcon } from "../components/icons";
import type { Entry } from "../bindings/Entry";
import ui from "../components/ui.module.css";

function currentPeriod(): string {
  return new Date().toISOString().slice(0, 7);
}

interface EditState {
  date: string;
  amount: string;
  concept: string;
  subconcept: string;
  description: string;
  fromAccountId: string;
  toAccountId: string;
}

function toEditState(e: Entry): EditState {
  return {
    date: e.date,
    amount: String(e.amount),
    concept: e.concept ?? "",
    subconcept: e.subconcept ?? "",
    description: e.description ?? "",
    fromAccountId: e.from_account_id != null ? String(e.from_account_id) : "",
    toAccountId: e.to_account_id != null ? String(e.to_account_id) : "",
  };
}

export default function Movements() {
  const [period, setPeriod] = useState(currentPeriod());
  const [kind, setKind] = useState("");
  const [concept, setConcept] = useState("");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [edit, setEdit] = useState<EditState | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const entries = useApi(() =>
    entriesApi.list({
      period: period || null,
      kind: kind || null,
      concept: concept || null,
    })
  );
  const accounts = useApi(() => accountsApi.list(false));

  async function remove(id: number) {
    if (!confirm(`¿Borrar la entrada #${id}?`)) return;
    try {
      await entriesApi.remove(id);
      bumpRevision();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  function startEdit(e: Entry) {
    setEditingId(e.id);
    setEdit(toEditState(e));
    setSaveError(null);
  }

  function cancelEdit() {
    setEditingId(null);
    setEdit(null);
    setSaveError(null);
  }

  async function saveEdit(e: Entry) {
    if (!edit) return;
    setSaveError(null);
    try {
      await entriesApi.update(e.id, {
        date: edit.date,
        amount: Number(edit.amount),
        concept: e.kind === "expense" || e.kind === "income" ? edit.concept : null,
        subconcept: edit.subconcept || null,
        description: edit.description || null,
        from_account_id:
          e.kind === "expense" || e.kind === "transfer"
            ? Number(edit.fromAccountId)
            : null,
        to_account_id:
          e.kind === "income" || e.kind === "opening" || e.kind === "transfer"
            ? Number(edit.toAccountId)
            : null,
      });
      bumpRevision();
      cancelEdit();
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : String(err));
    }
  }

  const canEditFrom = (k: string) => k === "expense" || k === "transfer";
  const canEditTo = (k: string) => k === "income" || k === "opening" || k === "transfer";
  const canEditConcept = (k: string) => k === "expense" || k === "income";

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
      <ErrorBanner message={saveError} />

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
              <th>Subconcepto</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {entries.data?.map((e) =>
              editingId === e.id && edit ? (
                <tr key={e.id}>
                  <td>#{e.id}</td>
                  <td>
                    <input
                      className={ui.input}
                      type="date"
                      value={edit.date}
                      onChange={(ev) => setEdit({ ...edit, date: ev.target.value })}
                    />
                  </td>
                  <td>{e.kind}</td>
                  <td>
                    <input
                      className={ui.input}
                      type="number"
                      step="0.01"
                      value={edit.amount}
                      onChange={(ev) => setEdit({ ...edit, amount: ev.target.value })}
                    />
                  </td>
                  <td>
                    {canEditFrom(e.kind) ? (
                      <select
                        className={ui.input}
                        value={edit.fromAccountId}
                        onChange={(ev) => setEdit({ ...edit, fromAccountId: ev.target.value })}
                      >
                        {accounts.data?.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.name}
                          </option>
                        ))}
                      </select>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td>
                    {canEditTo(e.kind) ? (
                      <select
                        className={ui.input}
                        value={edit.toAccountId}
                        onChange={(ev) => setEdit({ ...edit, toAccountId: ev.target.value })}
                      >
                        {accounts.data?.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.name}
                          </option>
                        ))}
                      </select>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td>
                    {canEditConcept(e.kind) ? (
                      <input
                        className={ui.input}
                        value={edit.concept}
                        onChange={(ev) => setEdit({ ...edit, concept: ev.target.value })}
                      />
                    ) : (
                      "—"
                    )}
                  </td>
                  <td>
                    <input
                      className={ui.input}
                      value={edit.subconcept}
                      onChange={(ev) => setEdit({ ...edit, subconcept: ev.target.value })}
                    />
                  </td>
                  <td className={ui.row}>
                    <button className={ui.button} onClick={() => saveEdit(e)}>
                      Guardar
                    </button>
                    <button className={ui.buttonSecondary} onClick={cancelEdit}>
                      Cancelar
                    </button>
                  </td>
                </tr>
              ) : (
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
                  <td>{e.subconcept ?? "—"}</td>
                  <td className={ui.row}>
                    <button
                      className={ui.iconButton}
                      title="Editar"
                      aria-label="Editar"
                      onClick={() => startEdit(e)}
                    >
                      <PencilIcon />
                    </button>
                    <button
                      className={ui.iconButtonDanger}
                      title="Borrar"
                      aria-label="Borrar"
                      onClick={() => remove(e.id)}
                    >
                      <TrashIcon />
                    </button>
                  </td>
                </tr>
              )
            )}
          </tbody>
        </table>
        {entries.data && entries.data.length === 0 && (
          <p className={ui.muted}>No hay movimientos con estos filtros.</p>
        )}
      </div>
    </div>
  );
}

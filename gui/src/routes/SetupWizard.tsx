import { useState } from "react";
import { accountsApi, type AccountKindInput } from "../api/accounts";
import { setupApi } from "../api/setup";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import ui from "../components/ui.module.css";

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

interface DraftAccount {
  name: string;
  kind: AccountKindInput;
  target: string;
  limit: string;
  restricted: boolean;
}

function emptyDraft(): DraftAccount {
  return { name: "", kind: "spending", target: "", limit: "", restricted: false };
}

/** Shown when the database has no accounts yet — creates the starting set
 * of accounts, then optionally seeds their opening balances (as
 * `kind='opening'` entries, so they never count as this month's income). */
export default function SetupWizard({ onDone }: { onDone: () => void }) {
  const accounts = useApi(() => accountsApi.list(false));
  const [drafts, setDrafts] = useState<DraftAccount[]>([emptyDraft()]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const hasAccounts = (accounts.data?.length ?? 0) > 0;

  function updateDraft(i: number, patch: Partial<DraftAccount>) {
    setDrafts((ds) => ds.map((d, idx) => (idx === i ? { ...d, ...patch } : d)));
  }

  async function createAccounts(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const valid = drafts.filter((d) => d.name.trim());
    if (valid.length === 0) {
      setError("Agrega al menos una cuenta.");
      return;
    }
    setBusy(true);
    try {
      for (const d of valid) {
        await accountsApi.create({
          name: d.name.trim(),
          kind: d.kind,
          target_amount: d.kind === "target" ? Number(d.target || 0) : null,
          credit_limit: d.kind === "credit" && d.limit ? Number(d.limit) : null,
          restricted: d.restricted,
        });
      }
      bumpRevision();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ maxWidth: 640, margin: "40px auto" }}>
      <h1>Bienvenido a money-tracker</h1>
      <p className={ui.muted}>
        Antes de empezar, creá tus cuentas (efectivo, débito, el fondo de emergencia, etc.) y,
        si querés, cargá el saldo con el que arrancás.
      </p>

      {!hasAccounts && (
        <form className={ui.card} onSubmit={createAccounts}>
          <h3>1. Crear cuentas</h3>
          <ErrorBanner message={error} />
          {drafts.map((d, i) => (
            <div key={i} className={ui.grid} style={{ marginBottom: 8 }}>
              <Field label="Nombre">
                <input
                  className={ui.input}
                  value={d.name}
                  onChange={(e) => updateDraft(i, { name: e.target.value })}
                  placeholder="efectivo, débito, Fondo de emergencia…"
                />
              </Field>
              <Field label="Tipo">
                <select
                  className={ui.input}
                  value={d.kind}
                  onChange={(e) => updateDraft(i, { kind: e.target.value as AccountKindInput })}
                >
                  <option value="spending">Gasto</option>
                  <option value="emergency">Fondo de emergencia</option>
                  <option value="target">Bucket con meta</option>
                  <option value="credit">Tarjeta de crédito</option>
                </select>
              </Field>
              {d.kind === "target" && (
                <Field label="Meta ($)">
                  <input
                    className={ui.input}
                    type="number"
                    value={d.target}
                    onChange={(e) => updateDraft(i, { target: e.target.value })}
                  />
                </Field>
              )}
              {d.kind === "spending" && (
                <label className={ui.row} style={{ alignSelf: "end" }}>
                  <input
                    type="checkbox"
                    checked={d.restricted}
                    onChange={(e) => updateDraft(i, { restricted: e.target.checked })}
                  />
                  Restringida
                </label>
              )}
            </div>
          ))}
          <div className={ui.row}>
            <button
              className={ui.buttonSecondary}
              type="button"
              onClick={() => setDrafts((ds) => [...ds, emptyDraft()])}
            >
              + Agregar otra
            </button>
            <button className={ui.button} type="submit" disabled={busy}>
              Crear cuentas
            </button>
          </div>
        </form>
      )}

      {hasAccounts && accounts.data && <SeedStep accounts={accounts.data} onDone={onDone} />}
    </div>
  );
}

function SeedStep({
  accounts,
  onDone,
}: {
  accounts: { id: number; name: string }[];
  onDone: () => void;
}) {
  const [amounts, setAmounts] = useState<Record<string, string>>({});
  const [date, setDate] = useState(today());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const pairs: [string, number][] = accounts
      .map((a) => [a.name, Number(amounts[a.name] || 0)] as [string, number])
      .filter(([, amount]) => amount > 0);

    if (pairs.length === 0) {
      onDone();
      return;
    }
    setBusy(true);
    try {
      await setupApi.seed(pairs, date);
      bumpRevision();
      onDone();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.card} onSubmit={submit}>
      <h3>2. Saldos iniciales (opcional)</h3>
      <p className={ui.muted}>
        No cuentan como ingreso del mes — son el punto de partida.
      </p>
      <ErrorBanner message={error} />
      <div className={ui.grid}>
        {accounts.map((a) => (
          <Field key={a.id} label={a.name}>
            <input
              className={ui.input}
              type="number"
              step="0.01"
              min="0"
              value={amounts[a.name] ?? ""}
              onChange={(e) => setAmounts((m) => ({ ...m, [a.name]: e.target.value }))}
            />
          </Field>
        ))}
        <Field label="Fecha">
          <input
            className={ui.input}
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
          />
        </Field>
      </div>
      <div className={ui.row} style={{ marginTop: 12 }}>
        <button className={ui.button} type="submit" disabled={busy}>
          Terminar
        </button>
      </div>
    </form>
  );
}

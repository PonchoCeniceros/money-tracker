import { useState } from "react";
import { accountsApi, type AccountKindInput } from "../api/accounts";
import { bucketsApi } from "../api/buckets";
import { conceptsApi } from "../api/concepts";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import { Money } from "../components/Money";
import ui from "../components/ui.module.css";
import type { AccountBalance } from "../bindings/AccountBalance";

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

type RowAction = { kind: "deposit" | "withdraw" | "reconcile"; account: AccountBalance } | null;

function isBucketAction(
  a: RowAction
): a is { kind: "deposit" | "withdraw"; account: AccountBalance } {
  return a !== null && a.kind !== "reconcile";
}

export default function Accounts() {
  const accounts = useApi(() => accountsApi.list(true));
  const spendingAccounts = useApi(() => accountsApi.list(false));
  const [action, setAction] = useState<RowAction>(null);
  const [showCreate, setShowCreate] = useState(false);

  return (
    <div>
      <div className={ui.row} style={{ justifyContent: "space-between" }}>
        <h1>Cuentas</h1>
        <button className={ui.button} onClick={() => setShowCreate((v) => !v)}>
          {showCreate ? "Cancelar" : "+ Nueva cuenta"}
        </button>
      </div>

      <ErrorBanner message={accounts.error} />

      {showCreate && (
        <CreateAccountForm onDone={() => setShowCreate(false)} />
      )}

      <div className={ui.card}>
        <table className={ui.table}>
          <thead>
            <tr>
              <th>Cuenta</th>
              <th>Tipo</th>
              <th>Saldo</th>
              <th>Meta / Límite</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {accounts.data?.map((a) => (
              <tr key={a.id} style={a.archived ? { opacity: 0.5 } : undefined}>
                <td>
                  {a.name}
                  {!a.liquid && <span className={ui.badge} style={{ marginLeft: 6 }}>restringida</span>}
                  {a.archived && <span className={ui.badge} style={{ marginLeft: 6 }}>archivada</span>}
                </td>
                <td>{a.kind}</td>
                <td>
                  <Money amount={a.balance} />
                </td>
                <td>
                  {a.kind === "target" && a.target_amount != null && (
                    <Money amount={a.target_amount} />
                  )}
                  {a.kind === "credit" && a.credit_limit != null && (
                    <Money amount={a.credit_limit} />
                  )}
                </td>
                <td>
                  {!a.archived && (
                    <div className={ui.row}>
                      {(a.kind === "emergency" || a.kind === "target") && (
                        <>
                          <button
                            className={ui.buttonSecondary}
                            onClick={() => setAction({ kind: "deposit", account: a })}
                          >
                            Depositar
                          </button>
                          <button
                            className={ui.buttonSecondary}
                            onClick={() => setAction({ kind: "withdraw", account: a })}
                          >
                            Retirar
                          </button>
                        </>
                      )}
                      {a.kind === "spending" && (
                        <button
                          className={ui.buttonSecondary}
                          onClick={() => setAction({ kind: "reconcile", account: a })}
                        >
                          Cuadrar
                        </button>
                      )}
                      <button
                        className={ui.buttonDanger}
                        onClick={async () => {
                          try {
                            await accountsApi.archive(a.id);
                            bumpRevision();
                          } catch (e) {
                            alert(e instanceof Error ? e.message : String(e));
                          }
                        }}
                      >
                        Archivar
                      </button>
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {isBucketAction(action) && spendingAccounts.data && (
        <BucketActionForm
          action={action}
          spendingAccounts={spendingAccounts.data}
          onDone={() => setAction(null)}
        />
      )}
      {action && action.kind === "reconcile" && (
        <ReconcileForm account={action.account} onDone={() => setAction(null)} />
      )}
    </div>
  );
}

function CreateAccountForm({ onDone }: { onDone: () => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<AccountKindInput>("spending");
  const [target, setTarget] = useState("");
  const [limit, setLimit] = useState("");
  const [restricted, setRestricted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (!name) {
      setError("Falta el nombre.");
      return;
    }
    setBusy(true);
    try {
      await accountsApi.create({
        name,
        kind,
        target_amount: kind === "target" ? Number(target) : null,
        credit_limit: kind === "credit" && limit ? Number(limit) : null,
        restricted,
      });
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
      <ErrorBanner message={error} />
      <div className={ui.grid}>
        <Field label="Nombre">
          <input className={ui.input} value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <Field label="Tipo">
          <select
            className={ui.input}
            value={kind}
            onChange={(e) => setKind(e.target.value as AccountKindInput)}
          >
            <option value="spending">Gasto (efectivo/débito/vales)</option>
            <option value="emergency">Fondo de emergencia</option>
            <option value="target">Bucket con meta</option>
            <option value="credit">Tarjeta de crédito</option>
          </select>
        </Field>
        {kind === "target" && (
          <Field label="Meta ($)">
            <input
              className={ui.input}
              type="number"
              step="0.01"
              min="0"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              required
            />
          </Field>
        )}
        {kind === "credit" && (
          <Field label="Límite de crédito (opcional)">
            <input
              className={ui.input}
              type="number"
              step="0.01"
              min="0"
              value={limit}
              onChange={(e) => setLimit(e.target.value)}
            />
          </Field>
        )}
        {kind === "spending" && (
          <label className={ui.row} style={{ alignSelf: "end" }}>
            <input
              type="checkbox"
              checked={restricted}
              onChange={(e) => setRestricted(e.target.checked)}
            />
            Restringida (ej. vales — no recibe aporte automático a emergencia)
          </label>
        )}
      </div>
      <div style={{ marginTop: 12 }}>
        <button className={ui.button} disabled={busy} type="submit">
          Crear cuenta
        </button>
      </div>
    </form>
  );
}

function BucketActionForm({
  action,
  spendingAccounts,
  onDone,
}: {
  action: { kind: "deposit" | "withdraw"; account: AccountBalance };
  spendingAccounts: AccountBalance[];
  onDone: () => void;
}) {
  const [amount, setAmount] = useState("");
  const [counterpartId, setCounterpartId] = useState<number | "">("");
  const [date, setDate] = useState(today());
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const counterpart = counterpartId === "" ? spendingAccounts[0]?.id : counterpartId;
  const isDeposit = action.kind === "deposit";

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (!amount || !counterpart) {
      setError("Falta el monto o la cuenta.");
      return;
    }
    setBusy(true);
    try {
      if (isDeposit) {
        await bucketsApi.deposit(action.account.id, counterpart, Number(amount), date);
      } else {
        await bucketsApi.withdraw(action.account.id, counterpart, Number(amount), date);
      }
      bumpRevision();
      setSuccess(
        isDeposit
          ? "✓ Depositado."
          : "✓ Movido. Esto NO es un gasto — si ya lo gastaste, regístralo en Registrar → Gasto."
      );
      setAmount("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.card} onSubmit={submit}>
      <h3>
        {isDeposit ? "Depositar a" : "Retirar de"} '{action.account.name}'
      </h3>
      <ErrorBanner message={error} />
      {success && <div className={ui.notice}>{success}</div>}
      <div className={ui.grid}>
        <Field label="Monto">
          <input
            className={ui.input}
            type="number"
            step="0.01"
            min="0"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            required
          />
        </Field>
        <Field label={isDeposit ? "Desde" : "Hacia"}>
          <select
            className={ui.input}
            value={counterpart ?? ""}
            onChange={(e) => setCounterpartId(Number(e.target.value))}
          >
            {spendingAccounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </Field>
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
        <button className={ui.button} disabled={busy} type="submit">
          Confirmar
        </button>
        <button className={ui.buttonSecondary} type="button" onClick={onDone}>
          Cerrar
        </button>
      </div>
    </form>
  );
}

function ReconcileForm({ account, onDone }: { account: AccountBalance; onDone: () => void }) {
  const concepts = useApi(() => conceptsApi.list("expense"));
  const [actual, setActual] = useState("");
  const [concept, setConcept] = useState("Discrecional");
  const [date, setDate] = useState(today());
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (actual === "") {
      setError("Ingresa cuánto hay realmente en la cuenta.");
      return;
    }
    setBusy(true);
    try {
      const r = await accountsApi.reconcile(account.id, Number(actual), concept, date);
      bumpRevision();
      if (r.entry_id === null) {
        setResult("Sin diferencia — el saldo ya coincidía.");
      } else if (r.diff > 0) {
        setResult(`✓ Cuadre: $${r.diff.toFixed(2)} sin registrar → gasto de '${concept}'`);
      } else {
        setResult(`✓ Cuadre: $${(-r.diff).toFixed(2)} de más → ingreso de '${concept}'`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.card} onSubmit={submit}>
      <h3>Cuadrar '{account.name}'</h3>
      <p className={ui.muted}>
        Saldo actual (según el sistema): <Money amount={account.balance} />
      </p>
      <ErrorBanner message={error} />
      {result && <div className={ui.notice}>{result}</div>}
      <div className={ui.grid}>
        <Field label="Cuánto hay de verdad ($)">
          <input
            className={ui.input}
            type="number"
            step="0.01"
            min="0"
            value={actual}
            onChange={(e) => setActual(e.target.value)}
            required
          />
        </Field>
        <Field label="Concepto (para el ajuste)">
          <select className={ui.input} value={concept} onChange={(e) => setConcept(e.target.value)}>
            {concepts.data?.map((c) => (
              <option key={c.name} value={c.name}>
                {c.name}
              </option>
            ))}
          </select>
        </Field>
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
        <button className={ui.button} disabled={busy} type="submit">
          Cuadrar
        </button>
        <button className={ui.buttonSecondary} type="button" onClick={onDone}>
          Cerrar
        </button>
      </div>
    </form>
  );
}

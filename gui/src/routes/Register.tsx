import { useState } from "react";
import { accountsApi } from "../api/accounts";
import { conceptsApi } from "../api/concepts";
import { entriesApi } from "../api/entries";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import ui from "../components/ui.module.css";

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

type Tab = "expense" | "income" | "transfer";

export default function Register() {
  const [tab, setTab] = useState<Tab>("expense");
  const accounts = useApi(() => accountsApi.list(false));

  return (
    <div>
      <h1>Registrar</h1>
      <div className={ui.tabs}>
        <button
          className={tab === "expense" ? ui.tabActive : ui.tab}
          onClick={() => setTab("expense")}
        >
          Gasto
        </button>
        <button
          className={tab === "income" ? ui.tabActive : ui.tab}
          onClick={() => setTab("income")}
        >
          Ingreso
        </button>
        <button
          className={tab === "transfer" ? ui.tabActive : ui.tab}
          onClick={() => setTab("transfer")}
        >
          Transferencia
        </button>
      </div>

      <ErrorBanner message={accounts.error} />
      {accounts.data && accounts.data.length === 0 && (
        <div className={ui.notice}>
          No hay cuentas todavía. Créalas primero en la pestaña Cuentas.
        </div>
      )}

      {accounts.data && accounts.data.length > 0 && (
        <>
          {tab === "expense" && <ExpenseForm accounts={accounts.data} />}
          {tab === "income" && <IncomeForm accounts={accounts.data} />}
          {tab === "transfer" && <TransferForm accounts={accounts.data} />}
        </>
      )}
    </div>
  );
}

interface AccountOption {
  id: number;
  name: string;
  liquid: boolean;
  kind: string;
}

function ExpenseForm({ accounts }: { accounts: AccountOption[] }) {
  const concepts = useApi(() => conceptsApi.list("expense"));
  const [amount, setAmount] = useState("");
  const [concept, setConcept] = useState("");
  const [accountId, setAccountId] = useState<number | "">("");
  const [subconcept, setSubconcept] = useState("");
  const [description, setDescription] = useState("");
  const [date, setDate] = useState(today());
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const defaultAccount = accountId === "" ? accounts[0]?.id : accountId;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setSuccess(null);
    if (!amount || !concept || !defaultAccount) {
      setError("Falta monto, concepto o cuenta.");
      return;
    }
    setBusy(true);
    try {
      const id = await entriesApi.addExpense({
        date,
        amount: Number(amount),
        from_account_id: defaultAccount,
        concept,
        subconcept: subconcept || null,
        description: description || null,
      });
      setSuccess(`✓ Gasto registrado (#${id})`);
      setAmount("");
      setSubconcept("");
      setDescription("");
      bumpRevision();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.card} onSubmit={submit}>
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
        <Field label="Concepto">
          <select
            className={ui.input}
            value={concept}
            onChange={(e) => setConcept(e.target.value)}
            required
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
        <Field label="Cuenta">
          <select
            className={ui.input}
            value={defaultAccount ?? ""}
            onChange={(e) => setAccountId(Number(e.target.value))}
          >
            {accounts.map((a) => (
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
        <Field label="Subconcepto (opcional)">
          <input
            className={ui.input}
            value={subconcept}
            onChange={(e) => setSubconcept(e.target.value)}
          />
        </Field>
        <Field label="Descripción (opcional)">
          <input
            className={ui.input}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
      </div>
      <div style={{ marginTop: 12 }}>
        <button className={ui.button} disabled={busy} type="submit">
          Registrar gasto
        </button>
      </div>
    </form>
  );
}

function IncomeForm({ accounts }: { accounts: AccountOption[] }) {
  const concepts = useApi(() => conceptsApi.list("income"));
  const [amount, setAmount] = useState("");
  const [concept, setConcept] = useState("");
  const [accountId, setAccountId] = useState<number | "">("");
  const [description, setDescription] = useState("");
  const [date, setDate] = useState(today());
  const [splitEmergency, setSplitEmergency] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const targetAccount = accountId === "" ? accounts[0]?.id : accountId;
  const targetAccountObj = accounts.find((a) => a.id === targetAccount);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setSuccess(null);
    if (!amount || !concept || !targetAccount) {
      setError("Falta monto, concepto o cuenta.");
      return;
    }
    setBusy(true);
    try {
      const result = await entriesApi.addIncome({
        date,
        amount: Number(amount),
        to_account_id: targetAccount,
        concept,
        description: description || null,
        split_emergency: splitEmergency,
      });
      let msg = `✓ Ingreso registrado (#${result.entry_id})`;
      if (result.emergency) {
        msg += ` — se aportaron $${result.emergency[1].toFixed(2)} a '${result.emergency[0]}'`;
      }
      setSuccess(msg);
      setAmount("");
      setDescription("");
      bumpRevision();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.card} onSubmit={submit}>
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
        <Field label="Concepto">
          <select
            className={ui.input}
            value={concept}
            onChange={(e) => setConcept(e.target.value)}
            required
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
        <Field label="Cuenta destino">
          <select
            className={ui.input}
            value={targetAccount ?? ""}
            onChange={(e) => setAccountId(Number(e.target.value))}
          >
            {accounts.map((a) => (
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
        <Field label="Descripción (opcional)">
          <input
            className={ui.input}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
      </div>
      <label className={ui.row} style={{ marginTop: 12 }}>
        <input
          type="checkbox"
          checked={splitEmergency}
          onChange={(e) => setSplitEmergency(e.target.checked)}
        />
        Aportar % al fondo de emergencia
      </label>
      {targetAccountObj && !targetAccountObj.liquid && splitEmergency && (
        <div className={ui.notice}>
          '{targetAccountObj.name}' es una cuenta restringida — el aporte no se aplicará aunque
          esta casilla esté marcada.
        </div>
      )}
      <div style={{ marginTop: 12 }}>
        <button className={ui.button} disabled={busy} type="submit">
          Registrar ingreso
        </button>
      </div>
    </form>
  );
}

function TransferForm({ accounts }: { accounts: AccountOption[] }) {
  const [amount, setAmount] = useState("");
  const [fromId, setFromId] = useState<number | "">("");
  const [toId, setToId] = useState<number | "">("");
  const [description, setDescription] = useState("");
  const [date, setDate] = useState(today());
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const from = fromId === "" ? accounts[0]?.id : fromId;
  const to = toId === "" ? accounts[1]?.id ?? accounts[0]?.id : toId;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setSuccess(null);
    if (!amount || !from || !to) {
      setError("Falta monto, cuenta origen o destino.");
      return;
    }
    if (from === to) {
      setError("Origen y destino no pueden ser la misma cuenta.");
      return;
    }
    setBusy(true);
    try {
      const id = await entriesApi.addTransfer({
        date,
        amount: Number(amount),
        from_account_id: from,
        to_account_id: to,
        description: description || null,
      });
      setSuccess(`✓ Transferencia registrada (#${id}) — no cuenta como gasto ni ingreso`);
      setAmount("");
      setDescription("");
      bumpRevision();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.card} onSubmit={submit}>
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
        <Field label="De">
          <select
            className={ui.input}
            value={from ?? ""}
            onChange={(e) => setFromId(Number(e.target.value))}
          >
            {accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="A">
          <select
            className={ui.input}
            value={to ?? ""}
            onChange={(e) => setToId(Number(e.target.value))}
          >
            {accounts.map((a) => (
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
        <Field label="Descripción (opcional)">
          <input
            className={ui.input}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
      </div>
      <div style={{ marginTop: 12 }}>
        <button className={ui.button} disabled={busy} type="submit">
          Transferir
        </button>
      </div>
    </form>
  );
}

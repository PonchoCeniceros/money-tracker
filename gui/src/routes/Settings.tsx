import { useEffect, useState } from "react";
import { accountsApi } from "../api/accounts";
import { configApi } from "../api/config";
import { conceptsApi } from "../api/concepts";
import { useApi, bumpRevision } from "../hooks/useApi";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import ui from "../components/ui.module.css";

export default function Settings() {
  const accounts = useApi(() => accountsApi.list(false));
  const config = useApi(() => configApi.list());
  const concepts = useApi(() => conceptsApi.list());

  return (
    <div>
      <h1>Ajustes</h1>

      <ErrorBanner message={config.error} />
      <div className={ui.card}>
        <h3>Configuración</h3>
        <div className={ui.grid}>
          <ConfigSelect
            label="Cuenta por defecto (gastos)"
            configKey="default_account"
            currentValue={config.data}
            options={accounts.data?.map((a) => a.name) ?? []}
          />
          <ConfigSelect
            label="Cuenta por defecto (ingresos)"
            configKey="income_account"
            currentValue={config.data}
            options={accounts.data?.map((a) => a.name) ?? []}
          />
          <ConfigSelect
            label="Concepto del sobre de efectivo"
            configKey="cash_concept"
            currentValue={config.data}
            options={concepts.data?.filter((c) => c.concept_type !== "income").map((c) => c.name) ?? []}
          />
          <ConfigNumber
            label="% al fondo de emergencia"
            configKey="emergency_pct"
            currentValue={config.data}
          />
        </div>
      </div>

      <ErrorBanner message={concepts.error} />
      <div className={ui.card}>
        <h3>Conceptos</h3>
        <ConceptForm />
        <table className={ui.table} style={{ marginTop: 12 }}>
          <thead>
            <tr>
              <th>Nombre</th>
              <th>Tipo</th>
            </tr>
          </thead>
          <tbody>
            {concepts.data?.map((c) => (
              <tr key={c.name}>
                <td>{c.name}</td>
                <td>{c.concept_type}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ConfigSelect({
  label,
  configKey,
  currentValue,
  options,
}: {
  label: string;
  configKey: string;
  currentValue: { key: string; value: string }[] | null;
  options: string[];
}) {
  const existing = currentValue?.find((c) => c.key === configKey)?.value ?? "";
  const [value, setValue] = useState(existing);

  useEffect(() => setValue(existing), [existing]);

  async function save(v: string) {
    setValue(v);
    try {
      await configApi.set(configKey, v);
      bumpRevision();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <Field label={label}>
      <select className={ui.input} value={value} onChange={(e) => save(e.target.value)}>
        <option value="" disabled>
          (sin definir)
        </option>
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    </Field>
  );
}

function ConfigNumber({
  label,
  configKey,
  currentValue,
}: {
  label: string;
  configKey: string;
  currentValue: { key: string; value: string }[] | null;
}) {
  const existing = currentValue?.find((c) => c.key === configKey)?.value ?? "";
  const [value, setValue] = useState(existing);

  useEffect(() => setValue(existing), [existing]);

  async function save() {
    try {
      await configApi.set(configKey, value);
      bumpRevision();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <Field label={label}>
      <input
        className={ui.input}
        type="number"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={save}
      />
    </Field>
  );
}

function ConceptForm() {
  const [name, setName] = useState("");
  const [type, setType] = useState<"expense" | "income" | "both">("expense");
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
      await conceptsApi.add(name, type);
      setName("");
      bumpRevision();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className={ui.row} onSubmit={submit}>
      <ErrorBanner message={error} />
      <Field label="Nombre">
        <input className={ui.input} value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label="Tipo">
        <select
          className={ui.input}
          value={type}
          onChange={(e) => setType(e.target.value as "expense" | "income" | "both")}
        >
          <option value="expense">Gasto</option>
          <option value="income">Ingreso</option>
          <option value="both">Ambos</option>
        </select>
      </Field>
      <button className={ui.button} disabled={busy} type="submit" style={{ alignSelf: "end" }}>
        Agregar
      </button>
    </form>
  );
}

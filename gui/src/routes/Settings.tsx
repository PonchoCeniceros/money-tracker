import { useEffect, useState } from "react";
import { accountsApi } from "../api/accounts";
import { configApi } from "../api/config";
import { conceptsApi } from "../api/concepts";
import { syncApi } from "../api/sync";
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

      <SyncCard />

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
          <ConfigNumber
            label="Gasto mensual histórico de referencia ($, opcional)"
            configKey="baseline_monthly_expense"
            currentValue={config.data}
          />
        </div>
        <p className={ui.muted} style={{ marginTop: 8, fontSize: 12 }}>
          El gasto histórico solo se usa en "Meses de colchón" del Dashboard mientras no haya
          ningún mes real registrado en la app — en cuanto exista uno, se ignora por completo.
        </p>
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

function SyncCard() {
  const status = useApi(() => syncApi.status());
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const s = status.data;

  async function login(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await syncApi.login({ email, password });
      setEmail("");
      setPassword("");
      bumpRevision();
      status.reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function logout() {
    setBusy(true);
    try {
      await syncApi.logout();
      bumpRevision();
      status.reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function syncNow() {
    setBusy(true);
    try {
      await syncApi.poll();
      bumpRevision();
      status.reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  if (!s) {
    return <ErrorBanner message={status.error} />;
  }

  if (!s.remote_configured) {
    return (
      <div className={ui.card}>
        <h3>Sincronización</h3>
        <p className={ui.muted}>
          Modo local — esta instalación guarda todo en el archivo de la máquina.
          Para sincronizar entre dispositivos, configura Supabase (URL + publishable key)
          en <code>~/.money-tracker/config.toml</code> o con las variables de entorno{" "}
          <code>MONEY_TRACKER_SUPABASE_URL</code>/<code>MONEY_TRACKER_SUPABASE_KEY</code>.
        </p>
      </div>
    );
  }

  return (
    <div className={ui.card}>
      <h3>Sincronización (Supabase)</h3>
      <ErrorBanner message={error} />

      {s.logged_in ? (
        <div>
          <p className={ui.muted}>
            Sesión iniciada{ s.email ? ` como ${s.email}` : ""}.
            {s.remote_revision != null && ` Revisión remota: ${s.remote_revision}.`}
            {s.mirror_cursor != null && ` Espejo local: revisión ${s.mirror_cursor}.`}
          </p>
          {s.warning && <ErrorBanner message={s.warning} />}
          <div className={ui.row}>
            <button className={ui.button} disabled={busy} onClick={syncNow}>
              Sincronizar ahora
            </button>
            <button className={ui.buttonSecondary} disabled={busy} onClick={logout}>
              Cerrar sesión
            </button>
          </div>
        </div>
      ) : (
        <form className={ui.grid} onSubmit={login} style={{ marginTop: 8 }}>
          <Field label="Email">
            <input
              className={ui.input}
              type="email"
              value={email}
              autoComplete="username"
              onChange={(e) => setEmail(e.target.value)}
            />
          </Field>
          <Field label="Contraseña">
            <input
              className={ui.input}
              type="password"
              value={password}
              autoComplete="current-password"
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
          <button className={ui.button} disabled={busy} type="submit" style={{ alignSelf: "end" }}>
            Iniciar sesión
          </button>
        </form>
      )}
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

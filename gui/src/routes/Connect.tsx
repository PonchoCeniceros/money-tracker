import { useState } from "react";
import { syncApi } from "../api/sync";
import { Field } from "../components/Field";
import { ErrorBanner } from "../components/ErrorBanner";
import ui from "../components/ui.module.css";

/** Shown instead of the whole app when `ledger_status` fails with a connection
 * problem. `kind` says which: missing config, no/expired session, or a schema
 * version this build doesn't expect (not fixable from here). */
export default function Connect({
  kind,
  message,
  onConnected,
}: {
  kind: string;
  message: string;
  onConnected: () => void;
}) {
  const needsProject = kind === "not_configured";
  const [url, setUrl] = useState("");
  const [key, setKey] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (kind === "schema_mismatch") {
    return (
      <div className={ui.card} style={{ maxWidth: 560, margin: "48px auto" }}>
        <h2>El esquema de Supabase no coincide</h2>
        <ErrorBanner message={message} />
        <p className={ui.muted}>
          Cuando lo corrijas (aplicando el archivo indicado en el SQL Editor, o actualizando la app), vuelve a
          abrir la app.
        </p>
      </div>
    );
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await syncApi.login({
        url: needsProject ? url.trim() : undefined,
        key: needsProject ? key.trim() : undefined,
        email: email.trim(),
        password,
      });
      setPassword("");
      onConnected();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={ui.card} style={{ maxWidth: 560, margin: "48px auto" }}>
      <h2>Conectar con Supabase</h2>
      <p className={ui.muted}>
        {needsProject
          ? "money-tracker guarda tus datos en tu proyecto de Supabase. Pega la URL del proyecto y su publishable key (Project Settings → API), e inicia sesión con tu usuario."
          : "Inicia sesión con tu usuario de Supabase para continuar."}
      </p>
      <ErrorBanner message={error ?? (needsProject ? null : message)} />
      <form className={ui.grid} onSubmit={submit} style={{ marginTop: 8 }}>
        {needsProject && (
          <>
            <Field label="URL del proyecto">
              <input
                className={ui.input}
                placeholder="https://<ref>.supabase.co"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
              />
            </Field>
            <Field label="Publishable key">
              <input
                className={ui.input}
                placeholder="sb_publishable_…"
                value={key}
                onChange={(e) => setKey(e.target.value)}
              />
            </Field>
          </>
        )}
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
        <button
          className={ui.button}
          disabled={busy || !email || !password || (needsProject && (!url || !key))}
          type="submit"
          style={{ alignSelf: "end" }}
        >
          {busy ? "Conectando…" : "Conectar"}
        </button>
      </form>
    </div>
  );
}

import { useCallback, useEffect, useState } from "react";
import { accountsApi } from "./api/accounts";
import { ApiCallError, SESSION_KINDS, SESSION_LOST_EVENT } from "./api/client";
import { CONNECT_KINDS, syncApi } from "./api/sync";
import { backupApi } from "./api/backup";
import { useApi, useRefetchOnFocus, bumpRevision } from "./hooks/useApi";
import { useSync } from "./hooks/useSync";
import Dashboard from "./routes/Dashboard";
import Register from "./routes/Register";
import Accounts from "./routes/Accounts";
import Movements from "./routes/Movements";
import Budgets from "./routes/Budgets";
import Settings from "./routes/Settings";
import SetupWizard from "./routes/SetupWizard";
import Connect from "./routes/Connect";
import styles from "./App.module.css";
import ui from "./components/ui.module.css";

type Connection =
  | { state: "checking" }
  | { state: "ok" }
  | { state: "needs_connect"; kind: string; message: string };

/** Core messages point at the CLI (`db remote login`); in the GUI, say it in GUI terms. */
function connectMessage(kind: string, message: string): string {
  if (SESSION_KINDS.includes(kind)) {
    return "Tu sesión de Supabase expiró o no está iniciada. Inicia sesión de nuevo para continuar.";
  }
  return message;
}

type View = "dashboard" | "register" | "accounts" | "movements" | "budgets" | "settings";

const TABS: { id: View; label: string }[] = [
  { id: "dashboard", label: "Dashboard" },
  { id: "register", label: "Registrar" },
  { id: "accounts", label: "Cuentas" },
  { id: "movements", label: "Movimientos" },
  { id: "budgets", label: "Presupuestos" },
  { id: "settings", label: "Ajustes" },
];

function App() {
  useRefetchOnFocus();
  useSync();
  const [view, setView] = useState<View>("dashboard");
  const [spinning, setSpinning] = useState(false);
  const accounts = useApi(() => accountsApi.list(false));
  const [connection, setConnection] = useState<Connection>({ state: "checking" });

  const checkConnection = useCallback(async () => {
    try {
      await syncApi.ledgerStatus();
      setConnection({ state: "ok" });
    } catch (e) {
      if (e instanceof ApiCallError && CONNECT_KINDS.includes(e.kind)) {
        setConnection({ state: "needs_connect", kind: e.kind, message: connectMessage(e.kind, e.message) });
      } else {
        // Network hiccups etc.: let the views show their own error banners.
        setConnection({ state: "ok" });
      }
    }
  }, []);

  useEffect(() => {
    checkConnection();
  }, [checkConnection]);

  // The session can die while the app is open (expired or revoked refresh token):
  // any call that hits it sends us back to the Connect screen.
  useEffect(() => {
    const onLost = (e: Event) => {
      const kind = (e as CustomEvent<string>).detail ?? "auth_needed";
      setConnection({ state: "needs_connect", kind, message: connectMessage(kind, "") });
    };
    window.addEventListener(SESSION_LOST_EVENT, onLost);
    return () => window.removeEventListener(SESSION_LOST_EVENT, onLost);
  }, []);

  // Lazy automatic backup, once connected: never blocks the UI, only reports.
  const [backupNotice, setBackupNotice] = useState<{ ok: boolean; text: string } | null>(null);
  useEffect(() => {
    if (connection.state !== "ok") return;
    backupApi
      .auto()
      .then((info) => {
        if (info) setBackupNotice({ ok: true, text: `Respaldo automático: ${info.path}` });
      })
      .catch((e) =>
        setBackupNotice({
          ok: false,
          text: `No se pudo hacer el respaldo automático (${e instanceof Error ? e.message : e}). Se reintentará la próxima vez.`,
        })
      );
  }, [connection.state]);

  function refresh() {
    bumpRevision();
    setSpinning(true);
    setTimeout(() => setSpinning(false), 500);
  }

  // Gates, in order: connection → setup wizard (no accounts yet) → tabs.
  if (connection.state === "checking") {
    return <div className={styles.app} />;
  }
  if (connection.state === "needs_connect") {
    return (
      <div className={styles.app}>
        <main className={styles.main}>
          <Connect
            kind={connection.kind}
            message={connection.message}
            onConnected={async () => {
              await checkConnection();
              bumpRevision();
            }}
          />
        </main>
      </div>
    );
  }

  // No accounts yet: this is a fresh database. Gate the whole app behind
  // the setup wizard rather than showing an empty dashboard everywhere.
  if (accounts.data && accounts.data.length === 0) {
    return (
      <div className={styles.app}>
        <main className={styles.main}>
          <SetupWizard onDone={() => bumpRevision()} />
        </main>
      </div>
    );
  }

  return (
    <div className={styles.app}>
      <nav className={styles.nav}>
        <div className={styles.brand}>money-tracker</div>
        <div className={styles.tabs}>
          {TABS.map((t) => (
            <button
              key={t.id}
              className={view === t.id ? styles.tabActive : styles.tab}
              onClick={() => setView(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <button
          className={ui.buttonSecondary}
          style={{ marginLeft: "auto" }}
          onClick={refresh}
          title="Volver a cargar los datos"
        >
          <span className={spinning ? styles.spin : undefined}>↻</span> Actualizar
        </button>
      </nav>
      <main className={styles.main}>
        {backupNotice && (
          <div
            className={backupNotice.ok ? ui.notice : ui.error}
            onClick={() => setBackupNotice(null)}
            title="Clic para cerrar"
            style={{ cursor: "pointer", marginBottom: 12 }}
          >
            {backupNotice.text}
          </div>
        )}
        {view === "dashboard" && <Dashboard />}
        {view === "register" && <Register />}
        {view === "accounts" && <Accounts />}
        {view === "movements" && <Movements />}
        {view === "budgets" && <Budgets />}
        {view === "settings" && <Settings />}
      </main>
    </div>
  );
}

export default App;

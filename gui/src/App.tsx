import { useCallback, useEffect, useState } from "react";
import { accountsApi } from "./api/accounts";
import { ApiCallError } from "./api/client";
import { CONNECT_KINDS, syncApi } from "./api/sync";
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
        setConnection({ state: "needs_connect", kind: e.kind, message: e.message });
      } else {
        // Network hiccups etc.: let the views show their own error banners.
        setConnection({ state: "ok" });
      }
    }
  }, []);

  useEffect(() => {
    checkConnection();
  }, [checkConnection]);

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

import { useState } from "react";
import { accountsApi } from "./api/accounts";
import { useApi, useRefetchOnFocus, bumpRevision } from "./hooks/useApi";
import Dashboard from "./routes/Dashboard";
import Register from "./routes/Register";
import Accounts from "./routes/Accounts";
import Movements from "./routes/Movements";
import Budgets from "./routes/Budgets";
import Settings from "./routes/Settings";
import SetupWizard from "./routes/SetupWizard";
import styles from "./App.module.css";

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
  const [view, setView] = useState<View>("dashboard");
  const accounts = useApi(() => accountsApi.list(false));

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

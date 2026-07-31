import { useState } from "react";
import { reportApi } from "../api/report";
import { useApi } from "../hooks/useApi";
import { Money } from "../components/Money";
import { ErrorBanner } from "../components/ErrorBanner";
import ui from "../components/ui.module.css";

function currentPeriod(): string {
  return new Date().toISOString().slice(0, 7);
}

export default function Dashboard() {
  const [period, setPeriod] = useState(currentPeriod());
  const report = useApi(() => reportApi.monthly(period));
  const netWorth = useApi(() => reportApi.netWorth());

  return (
    <div>
      <div className={ui.row} style={{ justifyContent: "space-between", marginBottom: 16 }}>
        <h1>Dashboard</h1>
        <input
          type="month"
          className={ui.input}
          value={period}
          onChange={(e) => setPeriod(e.target.value)}
        />
      </div>

      <ErrorBanner message={report.error ?? netWorth.error} />

      {report.data && (
        <div className={ui.card}>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Gasto del mes (devengado)</span>
            <span className={ui.statValue}>
              <Money amount={report.data.total_expense} />
            </span>
          </div>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Salida real de efectivo</span>
            <span className={ui.statValue}>
              <Money amount={report.data.cash_out} />
            </span>
          </div>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Ingreso del mes</span>
            <span className={ui.statValue}>
              <Money amount={report.data.total_income} />
            </span>
          </div>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Flujo neto (devengado)</span>
            <span className={ui.statValue}>
              <Money amount={report.data.net_flow} />
            </span>
          </div>
          {report.data.savings_contributions > 0 && (
            <div className={ui.stat}>
              <span className={ui.statLabel}>Aportes a ahorro</span>
              <span className={ui.statValue}>
                <Money amount={report.data.savings_contributions} />
              </span>
            </div>
          )}
          {report.data.savings_withdrawals > 0 && (
            <div className={ui.stat}>
              <span className={ui.statLabel}>Retiros de ahorro</span>
              <span className={ui.statValue}>
                <Money amount={report.data.savings_withdrawals} />
              </span>
            </div>
          )}
          {report.data.card_payments > 0 && (
            <div className={ui.stat}>
              <span className={ui.statLabel}>Pagos de tarjeta</span>
              <span className={ui.statValue}>
                <Money amount={report.data.card_payments} />
              </span>
            </div>
          )}
        </div>
      )}

      {report.data && report.data.by_concept.length > 0 && (
        <div className={ui.card}>
          <h3>Gastos por concepto</h3>
          <table className={ui.table}>
            <thead>
              <tr>
                <th>Concepto</th>
                <th>Gastado</th>
                <th>Presup.</th>
                <th>%</th>
                <th>#</th>
              </tr>
            </thead>
            <tbody>
              {report.data.by_concept.map((c) => {
                const budget = report.data!.budgets.find((b) => b.concept === c.concept);
                return (
                  <tr key={c.concept}>
                    <td>{c.concept}</td>
                    <td>
                      <Money amount={c.total} />
                    </td>
                    <td>{budget ? <Money amount={budget.budgeted} /> : "—"}</td>
                    <td>
                      {budget ? (
                        <span
                          style={{
                            color: budget.pct > 100 ? "var(--danger)" : undefined,
                          }}
                        >
                          {budget.pct.toFixed(0)}%
                        </span>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td>{c.count}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {netWorth.data && (
        <div className={ui.card}>
          <h3>Cuentas</h3>
          <table className={ui.table}>
            <thead>
              <tr>
                <th>Cuenta</th>
                <th>Tipo</th>
                <th>Saldo</th>
                <th>Meta / Límite</th>
              </tr>
            </thead>
            <tbody>
              {netWorth.data.accounts.map((a) => (
                <tr key={a.id}>
                  <td>
                    {a.name}
                    {!a.liquid && <span className={ui.badge} style={{ marginLeft: 6 }}>restringida</span>}
                  </td>
                  <td>{a.kind}</td>
                  <td>
                    <Money amount={a.balance} />
                  </td>
                  <td>
                    {a.kind === "target" && a.target_amount != null && (
                      <>
                        <Money amount={a.target_amount} /> (
                        {((a.balance / a.target_amount) * 100).toFixed(0)}%)
                      </>
                    )}
                    {a.kind === "credit" && a.credit_limit != null && (
                      <>
                        deuda <Money amount={Math.max(0, -a.balance)} /> · disponible{" "}
                        <Money amount={a.credit_limit + Math.min(0, a.balance)} />
                      </>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          <div style={{ marginTop: 16 }}>
            <div className={ui.stat}>
              <span className={ui.statLabel}>Efectivo disponible</span>
              <span className={ui.statValue}>
                <Money amount={netWorth.data.cash_on_hand} />
              </span>
            </div>
            <div className={ui.stat}>
              <span className={ui.statLabel}>Ahorro</span>
              <span className={ui.statValue}>
                <Money amount={netWorth.data.savings} />
              </span>
            </div>
            <div className={ui.stat}>
              <span className={ui.statLabel}>Deuda de tarjeta</span>
              <span className={ui.statValue}>
                <Money amount={netWorth.data.credit_debt} />
              </span>
            </div>
            <div className={ui.stat}>
              <span className={ui.statLabel}>Patrimonio neto</span>
              <span className={ui.statValue}>
                <Money amount={netWorth.data.net} />
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

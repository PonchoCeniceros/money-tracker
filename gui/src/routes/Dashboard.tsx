import { useState } from "react";
import { reportApi } from "../api/report";
import { configApi } from "../api/config";
import { useApi } from "../hooks/useApi";
import { Money } from "../components/Money";
import { ErrorBanner } from "../components/ErrorBanner";
import ui from "../components/ui.module.css";

function currentPeriod(): string {
  return new Date().toISOString().slice(0, 7);
}

/** `period` shifted back `monthsBack` calendar months, same "YYYY-MM" shape. */
function shiftPeriod(period: string, monthsBack: number): string {
  const [y, m] = period.split("-").map(Number);
  const d = new Date(Date.UTC(y, m - 1 - monthsBack, 1));
  return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}`;
}

export default function Dashboard() {
  const [period, setPeriod] = useState(currentPeriod());
  const report = useApi(() => reportApi.monthly(period));
  const netWorth = useApi(() => reportApi.netWorth());
  // Burn rate for "meses de colchón": average total_expense over the last 3
  // months that actually have activity, so a brand-new database (no history
  // yet) doesn't get diluted by zero-expense months and understate the risk.
  const burnHistory = useApi(() =>
    Promise.all([0, 1, 2].map((monthsBack) => reportApi.monthly(shiftPeriod(period, monthsBack)))),
  );
  const config = useApi(() => configApi.list());
  const expenseMonths = (burnHistory.data ?? []).map((r) => r.total_expense).filter((v) => v > 0);
  // Before there's enough real history in the app, fall back to a
  // config-seeded historical burn rate (e.g. from a legacy spreadsheet) so
  // "meses de colchón" shows a real-world estimate from day one instead of
  // "—". The moment a real month exists, it takes over completely — the
  // baseline is never blended with real data, just a placeholder until it.
  const baselineExpense = Number(
    config.data?.find((c) => c.key === "baseline_monthly_expense")?.value ?? "",
  );
  const usingBaseline = expenseMonths.length === 0 && baselineExpense > 0;
  const avgMonthlyExpense =
    expenseMonths.length > 0
      ? expenseMonths.reduce((a, b) => a + b, 0) / expenseMonths.length
      : usingBaseline
        ? baselineExpense
        : 0;

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

      <ErrorBanner message={report.error ?? netWorth.error ?? burnHistory.error ?? config.error} />

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

      {report.data && netWorth.data && (
        <div className={ui.card}>
          <h3>Métricas</h3>
          <p className={ui.muted} style={{ marginTop: -8, marginBottom: 12, fontSize: 12 }}>
            Tasa de ahorro y % discrecional son de este período; meses de colchón usa{" "}
            {usingBaseline
              ? "tu gasto promedio histórico de referencia (config: baseline_monthly_expense) porque todavía no hay meses reales registrados"
              : `el promedio de gasto de los últimos ${expenseMonths.length || 3} meses con actividad`}{" "}
            como "burn rate".
          </p>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Tasa de ahorro</span>
            <span className={ui.statValue}>
              {report.data.total_income > 0
                ? `${((report.data.savings_contributions / report.data.total_income) * 100).toFixed(0)}%`
                : "—"}
            </span>
          </div>
          <div className={ui.stat}>
            <span className={ui.statLabel}>% Discrecional</span>
            <span className={ui.statValue}>
              {report.data.total_expense > 0
                ? `${(
                    ((report.data.by_concept.find((c) => c.concept === "Discrecional")?.total ?? 0) /
                      report.data.total_expense) *
                    100
                  ).toFixed(0)}%`
                : "—"}
            </span>
          </div>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Uso de crédito</span>
            <span className={ui.statValue}>
              {(() => {
                const creditLimit = netWorth.data.accounts
                  .filter((a) => a.kind === "credit" && a.credit_limit != null)
                  .reduce((sum, a) => sum + (a.credit_limit ?? 0), 0);
                return creditLimit > 0
                  ? `${((netWorth.data.credit_debt / creditLimit) * 100).toFixed(0)}%`
                  : "—";
              })()}
            </span>
          </div>
          <div className={ui.stat}>
            <span className={ui.statLabel}>Meses de colchón</span>
            <span className={ui.statValue}>
              {avgMonthlyExpense > 0
                ? `${(netWorth.data.savings / avgMonthlyExpense).toFixed(1)} meses`
                : "—"}
            </span>
          </div>
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
                    {a.kind === "target" &&
                      (a.target_amount != null ? (
                        <>
                          <Money amount={a.target_amount} /> (
                          {((a.balance / a.target_amount) * 100).toFixed(0)}%)
                        </>
                      ) : (
                        "—"
                      ))}
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
              <span className={`${ui.statValue} ${ui.gradientText}`} style={{ fontSize: 20 }}>
                <Money amount={netWorth.data.net} />
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

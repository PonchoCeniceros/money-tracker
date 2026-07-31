import { call } from "./client";
import type { MonthlyReport } from "../bindings/MonthlyReport";
import type { NetWorth } from "../bindings/NetWorth";

export const reportApi = {
  monthly: (period: string) => call<MonthlyReport>("monthly_report", { period }),
  netWorth: (asOf?: string | null) => call<NetWorth>("net_worth", { asOf: asOf ?? null }),
};

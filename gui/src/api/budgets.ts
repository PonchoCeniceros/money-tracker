import { call } from "./client";
import type { Budget } from "../bindings/Budget";

export interface SetBudgetInput {
  concept: string;
  monthly_limit: number;
  period: string;
}

export const budgetsApi = {
  set: (input: SetBudgetInput) => call<void>("set_budget", { input }),
  list: (period: string) => call<Budget[]>("list_budgets", { period }),
  remove: (concept: string, period: string) => call<void>("delete_budget", { concept, period }),
};

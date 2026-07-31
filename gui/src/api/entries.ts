import { call } from "./client";
import type { Entry } from "../bindings/Entry";

export interface ExpenseInput {
  date: string;
  amount: number;
  from_account_id: number;
  concept: string;
  subconcept?: string | null;
  description?: string | null;
}

export interface IncomeInput {
  date: string;
  amount: number;
  to_account_id: number;
  concept: string;
  description?: string | null;
  split_emergency: boolean;
}

export interface IncomeOutput {
  entry_id: number;
  emergency: [string, number] | null;
}

export interface TransferInput {
  date: string;
  amount: number;
  from_account_id: number;
  to_account_id: number;
  description?: string | null;
}

export interface EntryFilterInput {
  period?: string | null;
  kind?: string | null;
  concept?: string | null;
  account_id?: number | null;
  limit?: number | null;
}

export const entriesApi = {
  addExpense: (input: ExpenseInput) => call<number>("add_expense", { input }),
  addIncome: (input: IncomeInput) => call<IncomeOutput>("add_income", { input }),
  addTransfer: (input: TransferInput) => call<number>("add_transfer", { input }),
  list: (filter: EntryFilterInput = {}) => call<Entry[]>("list_entries", { filter }),
  remove: (id: number) => call<void>("delete_entry", { id }),
};

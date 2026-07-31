import { call } from "./client";
import type { AccountBalance } from "../bindings/AccountBalance";

export type AccountKindInput = "spending" | "emergency" | "target" | "credit";

export interface NewAccountInput {
  name: string;
  kind: AccountKindInput;
  target_amount?: number | null;
  credit_limit?: number | null;
  restricted: boolean;
}

export interface ReconcileOutput {
  entry_id: number | null;
  diff: number;
}

export const accountsApi = {
  list: (includeArchived = false) =>
    call<AccountBalance[]>("list_accounts", { includeArchived }),

  create: (input: NewAccountInput) => call<number>("create_account", { input }),

  archive: (id: number, force = false) => call<void>("archive_account", { id, force }),

  reconcile: (id: number, actual: number, concept: string, date: string) =>
    call<ReconcileOutput>("reconcile_account", { id, actual, concept, date }),
};

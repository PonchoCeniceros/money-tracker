import { call } from "./client";
import type { LedgerStatus } from "../bindings/LedgerStatus";

export type { LedgerStatus };

/** Mirrors `commands/sync.rs::ConnectionInfo`. */
export interface ConnectionInfo {
  url: string | null;
  configured: boolean;
  logged_in: boolean;
  email: string | null;
  /** "llavero del sistema" | "archivo" */
  token_storage: string;
  last_backup: { at: string; path: string; revision: number; schema_version: number } | null;
}

export interface LoginInput {
  /** Only needed the first time, or to switch projects. */
  url?: string;
  key?: string;
  email: string;
  password: string;
}

/** Error kinds that mean "show the Connect screen instead of the app". */
export const CONNECT_KINDS = ["not_configured", "auth", "auth_needed", "schema_mismatch"];

export const syncApi = {
  ledgerStatus: () => call<LedgerStatus>("ledger_status"),
  connectionInfo: () => call<ConnectionInfo>("connection_info"),
  login: (input: LoginInput) => call<void>("remote_login", { input }),
  logout: () => call<void>("remote_logout"),
};

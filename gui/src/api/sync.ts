import { call } from "./client";

export interface SyncStatus {
  remote_configured: boolean;
  logged_in: boolean;
  email: string | null;
  remote_revision: number | null;
  mirror_cursor: number | null;
  warning: string | null;
}

export interface SyncPollResult {
  cursor: number | null;
  warning: string | null;
}

export interface LoginInput {
  email: string;
  password: string;
}

export const syncApi = {
  status: () => call<SyncStatus>("sync_status"),
  poll: () => call<SyncPollResult>("sync_poll"),
  login: (input: LoginInput) => call<void>("remote_login", { input }),
  logout: () => call<void>("remote_logout"),
};
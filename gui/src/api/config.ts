import { call } from "./client";

export interface ConfigEntry {
  key: string;
  value: string;
}

export const configApi = {
  get: (key: string) => call<string | null>("get_config", { key }),
  set: (key: string, value: string) => call<void>("set_config", { key, value }),
  list: () => call<ConfigEntry[]>("list_config"),
};

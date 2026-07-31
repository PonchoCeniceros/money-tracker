import { call } from "./client";

export interface SeedOutput {
  seeded: [string, number][];
}

export const setupApi = {
  isSeeded: () => call<boolean>("is_seeded"),
  seed: (accounts: [string, number][], date: string) =>
    call<SeedOutput>("seed", { input: { accounts, date } }),
};

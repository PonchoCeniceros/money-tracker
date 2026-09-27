import { call } from "./client";
import type { BackupInfo } from "../bindings/BackupInfo";

export type { BackupInfo };

export const backupApi = {
  /** `dest`: folder or new file; empty = ~/.money-tracker/backups/. */
  create: (dest?: string) => call<BackupInfo>("backup_create", { input: { dest: dest || null } }),
  /** `null` when no backup was due (last one is 7 days old or less). */
  auto: () => call<BackupInfo | null>("backup_auto"),
};

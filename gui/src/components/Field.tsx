import type { ReactNode } from "react";
import ui from "./ui.module.css";

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className={ui.field}>
      <span className={ui.fieldLabel}>{label}</span>
      {children}
    </label>
  );
}

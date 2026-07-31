import { call } from "./client";
import type { Concept } from "../bindings/Concept";

export const conceptsApi = {
  list: (typeFilter?: "expense" | "income" | null) => call<Concept[]>("list_concepts", { typeFilter: typeFilter ?? null }),
  add: (name: string, conceptType: "expense" | "income" | "both") =>
    call<void>("add_concept", { name, conceptType }),
};

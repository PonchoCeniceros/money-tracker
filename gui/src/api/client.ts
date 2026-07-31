import { invoke } from "@tauri-apps/api/core";

/** Mirrors `gui/src-tauri/src/error.rs::ApiError`. */
export interface ApiErrorShape {
  kind: string;
  message: string;
}

export class ApiCallError extends Error {
  kind: string;
  constructor(err: ApiErrorShape) {
    super(err.message);
    this.kind = err.kind;
  }
}

function isApiErrorShape(e: unknown): e is ApiErrorShape {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "message" in e &&
    typeof (e as ApiErrorShape).message === "string"
  );
}

/** Thin wrapper around `invoke` that turns the Rust-side `ApiError` into a
 * typed JS error instead of an opaque rejection. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isApiErrorShape(e)) {
      throw new ApiCallError(e);
    }
    throw e;
  }
}

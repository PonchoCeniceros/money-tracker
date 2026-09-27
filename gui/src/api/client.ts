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

/** Error kinds that mean the Supabase session is gone (never started, expired or revoked). */
export const SESSION_KINDS = ["auth", "auth_needed"];

/** Fired on `window` whenever any call fails with a session error, so the app can
 * switch to the Connect screen instead of every view showing its own error. */
export const SESSION_LOST_EVENT = "mt:session-lost";

/** Thin wrapper around `invoke` that turns the Rust-side `ApiError` into a
 * typed JS error instead of an opaque rejection. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isApiErrorShape(e)) {
      if (SESSION_KINDS.includes(e.kind) && cmd !== "remote_login") {
        window.dispatchEvent(new CustomEvent(SESSION_LOST_EVENT, { detail: e.kind }));
      }
      throw new ApiCallError(e);
    }
    throw e;
  }
}

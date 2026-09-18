# Contract: Remote API (Supabase)

**Branch**: `001-supabase-backend` | **Spec**: [spec.md](../spec.md) | **Research**: [../research.md](../research.md)

Contrato de la interfaz externa entre `money_core` (SupabaseBackend) y el proyecto de Supabase del
usuario. HTTP/HTTPS únicamente (puerto 443). Los valores `<project-ref>` y la key publicable vienen
de config/env (nunca versionados el refresh token ni el password).

## 1. Autenticación (GoTrue)

| Flujo | Endpoint | Cuerpo / Headers | Respuesta útil |
|---|---|---|---|
| Login | `POST /auth/v1/token?grant_type=password` | body `{ email, password }`, header `apikey: <publishable>` | `access_token`, `refresh_token`, `expires_in` |
| Refresh | `POST /auth/v1/token?grant_type=refresh_token` | body `{ refresh_token }` | nuevo `access_token`/`refresh_token` |
| Logout | `POST /auth/v1/logout` | `Authorization: Bearer <jwt>` | `204` |

Reglas: header `apikey` != `Authorization`. El JWT va solo en `Authorization: Bearer`. Ante
`invalid_grant` (refresh vencido/revocado) → re-prompt de credenciales. 401 en cualquier llamada →
refrescar y reintentar una vez.

## 2. Data API (PostgREST)

Base: `https://<project-ref>.supabase.co/rest/v1` · Headers: `apikey` + `Authorization: Bearer <jwt>`.
Solo el rol `authenticated` tiene permisos (RLS por `user_id = auth.uid()`).

| Recurso | Método | Notas |
|---|---|---|
| `accounts` | GET/POST/PATCH/DELETE | PATCH de archived para archivar (con verificación de saldo previa en money_core) |
| `entries` | GET/POST/PATCH/DELETE | Filtros de rango semestral: `date=gte.<start>&date=lte.<end-1day>`; kind/concept filtros `=?`; join de nombres vía `?select=*,from_account:accounts!entries_from_account_id_fkey(name),to_account:...` |
| `budgets` / `concepts` / `config` | GET/POST/PATCH/DELETE | upserts con `ON CONFLICT` (PostgREST `POST ... Prefer: resolution=merge-duplicates`) |
| `account_balances` | GET | solo select; sin RLS-filter extra (la VIEW es `security_invoker`) |
| `sync_state` | GET | `?select=revision` — fila única |
| `rpc/apply_entries` | POST | ver §3 |

Convenciones de V/O:

- Fechas: strings `YYYY-MM-DD` (columna `date`) — idéntico a los modelos/ts-rs.
- Ids: `bigint` serializado como número JSON — compatible con `#[ts(type="number")]`.
- Insert con id devuelto: `POST ... Prefer: return=representation&select=id` para alimentar el espejo.
- Errores: códigos Postgres (`23505`, `23514`, `40001`) mapeados a `AppError::Invalid` con mensaje
  accionable.

## 3. RPC atómico

`POST /rest/v1/rpc/apply_entries`

```
{
  "entries": [ { "date": "...", "kind": "income", "amount": 1000, "to_account_id": 1, ... },
               { "kind": "transfer", "amount": 100, "from_account_id": 1, "to_account_id": 2, ... } ],
  "expected_balance": null,          // opcional: referencia para cheques de overdraft
  "account_id": null
}
```

Garantías: transacción única; valida CHECKs e índices server-side; realiza los cheques de
sobredisponibilidad (target/emergency y cupo de crédito) dentro de la misma transacción; respuesta
con `[{ id, ...}, { id, ...}]` para poder espejar. RLS del rol autenticado.

## 4. Realtime (opcional, no en v1)

Si se adopta: WebSocket `wss://<ref>.supabase.co/realtime/v1` con `apikey` + upgrade con JWT del
usuario, canal `postgres_changes` sobre las tablas del libro. El polling de `sync_state` sigue
siendo el camino de backfill y el criterio de aceptación (< 1 min) no depende de esto.

## 5. Criterios de aceptación

1. Sin sesión válida, todo GET/POST a las tablas del libro devuelve `401`/vacío (RLS) — ningún dato
   expuesto a una key sin JWT.
2. Un POST que viole `amount <= 0` o cree un segundo `emergency` activo es rechazado con 4xx
   mapeable a `AppError::Invalid`.
3. `apply_entries` aplica el split completo o nada (fallo inyectado en la 2.ª fila no deja
   ingreso sin transferencia ni viceversa).
4. Con la misma data, las salidas de `report` (devengado vs salida real) contra la API coinciden con
   el cálculo en memoria de `ledger.rs`.
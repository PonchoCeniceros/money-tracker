# Research: Supabase como fuente de la base de datos

**Branch**: `001-supabase-backend` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Formato por decisión: **Decision** / **Rationale** / **Alternatives considered**.

## R1. Protocolo de acceso

- **Decision**: Data API de **PostgREST** (`https://<ref>.supabase.co/rest/v1/...`) sobre HTTPS, con la
  **key publicable** (`sb_publishable_*`) en el header `apikey` y el **JWT del usuario** en
  `Authorization: Bearer`. En Rust: `reqwest` + `postgrest-rs` para construir filtros (o `reqwest`
  directo si se quiere minimizar superficie).
- **Rationale**: Es el camino documentado por Supabase para clientes distribuidos (desktop/CLI);
  RLS + Auth solo funcionan correctamente sobre la Data API (PostgREST impersona el rol
  `authenticated` desde el JWT). HTTPS corre al 443 (sin problemas de IPv6/puerto 5432/NAT en las
  redes domésticas). No hay pool de conexiones que gestionar ni caché delante de `/rest/v1` (cada
  lectura es fresca, clave para la consistencia del espejo).
- **Alternatives**: (a) Postgres nativo vía `sqlx`/`tokio-postgres` — evasión de RLS (rol `postgres`),
  IPv6 por defecto, gestión de pool → **rechazado**. (b) SDK Rust monolítico de Supabase —
  inmaduro/alfa; se prefiere la pila mínima estable (`postgrest-rs`, auth con ~2 POSTs) → rechazado.

## R2. Auth y seguridad de un solo usuario

- **Decision**: Supabase Auth (GoTrue), flujo email/password: `POST /auth/v1/token?grant_type=password`
  → `access_token` (JWT ~1 h) + `refresh_token`. El refresh token se guarda en el **OS keyring**
  (crate `keyring`: macOS Keychain, Windows Credential Manager, Linux Secret Service) con fallback a
  archivo `0600` en headless. La contraseña **nunca se persiste** (re-prompt en `invalid_grant`).
  Refresco vía `grant_type=refresh_token` ante 401 o expiración. RLS **obligatoria en todas las
  tablas** anclada a `auth.uid()` (columna `user_id`) con signups deshabilitados.
- **Rationale**: La key publicable es público-segura (supabase la lista explícitamente como segura
  para "desktop apps, CLIs, executables...") pero viaja en el binario; sin RLS, cualquiera que la
  extraiga tendría acceso total. RLS es la única barrera real "{key extraído} → {datos}". El keyring
  es el estándar de facto cross-platform (lo usa el propio CLI de Supabase). Gestionar un solo
  usuario no elimina la necesidad: un segundo signup accidental no debe ver nada.
- **Alternatives**: OAuth/PKCE en navegador (sin beneficio para un usuario, más superficie) → rechazado.
  Password en archivo plano de `$HOME` (credential real en disco) → rechazado. Key legacy `anon`/
  `service_role` (eliminadas "late 2026") → se construye con `sb_publishable_*` desde el día uno.

## R3. Reglas contables en el servidor (integridad)

- **Decision**: Replicar el DDL SQLite en Postgres casi 1:1: `amount numeric check (amount > 0)`,
  `kind check in (...)`, índices parciales (p. ej. `create unique index ... on accounts (kind) where
  kind='emergency' and archived=false`), CHECK multi-columna de la forma de entrada, y una VIEW
  `account_balances` con `with (security_invoker = true)` (Postgres 15+) que respeta RLS de sus tablas base.
- **Rationale**: Todos los constructores mapean de forma limpia (los índices parciales son SQL
  estándar y la forma de entrada es un CHECK). PostgREST impone server-side: un POST violando un
  CHECK/índice devuelve 4xx/23514/23505 → ni el CLI, ni la GUI, ni un tercero con la key pueden
  corromper el libro. Es el mismo rol que cumple el esquema SQLite hoy.
- **Alternatives**: Triggers/procedimientos para todas las reglas (innecesario; CHECK+índice cubren)
  → rechazado. Solo validación en Rust (anula el diseño actual de integridad por esquema) → rechazado.

## R4. Gestión de esquema (migraciones)

- **Decision**: CLI de Supabase + carpeta `supabase/migrations/` de `.sql` versionados + `supabase db
  push`. Prohibido editar esquema desde el SQL Editor/Table Editor del dashboard (rompe el tracking).
- **Rationale**: Es el flujo documentado actual; para un solo proyecto sin CI, `supabase link` +
  `db push` es el equilibrio: historial auditable, reproducible y coherente con la gestión del repo.
  Las migraciones sirven además de *contrato* para mantener el DDL del espejo SQLite paritario.
- **Alternatives**: SQL Editor manual (rápido pero desincroniza `db push`) → rechazado. Migrator desde
  la app en Rust (el DDL no pasa por PostgREST; las migraciones son de desarrollador, no runtime)
  → rechazado. CI/CD con `supabase setup-cli` (innecesario a esta escala, se puede añadir luego).

## R5. Propagación entre instalaciones (< 1 min)

- **Decision**: **Polling REST** como mecanismo base: cada instalación ejecuta en background un poll
  cada ~30 s a `GET /rest/v1/sync_state?select=revision` (fila única `revision` incrementada por un
  **trigger** en cada insert/update/delete de las tablas del libro); si cambió, descarga solo las
  filas con `updated_at > last_sync` y actualiza su espejo local. La instalación escritora actualiza
  su espejo en línea inmediatamente tras cada operación (requisito duro de FR-009, sin red).
- **Rationale**: Cumple holgadamente el < 1 min con ~30 s de ventana + fetch. Es ~40 líneas de Rust
  (`tokio::spawn`/`Interval` + GET), cero riesgo de protocolo, misma lógica en CLI y GUI (que ya
  tiene `bumpRevision()` para refetch tras mutación — el poll simplemente la invoca). PostgREST no
  cachea. Realtime en Rust es el punto débil (clientes community beta, entrega no garantizada) → no
  apto como mecanismo único.
- **Alternatives**: **Realtime `postgres_changes`** (mejor UX ~200-300 ms pero entrega no garantizada,
  cliente beta, requiere backfill de todos modos) → se deja como acelerador opcional futuro.
  Listen/Notify o CDC lógica (no expuesto a clientes finales, overkill) → rechazado.

## R6. Credenciales y secretos

- **Decision**:
  | Secreto | Dónde vive |
  |---|---|
  | URL del proyecto + key publicable | `~/.money-tracker/config.toml` (0600) o env `MONEY_TRACKER_SUPABASE_URL` / `MONEY_TRACKER_SUPABASE_KEY` (env gana) |
  | Refresh token + email | OS keyring (`keyring` crate); fallback archivo `0600` headless |
  | Password | nunca persistida; re-prompt en `invalid_grant` |
  | Secret/service key | nunca dentro de la app (solo CI/edge functions si algún día se necesitara) |

- **Rationale**: Nada secreto se commitea; la única credencial en-repo legítima es la key publicable
  (diseñada para eso). Keyring es idiomático y portable. El patron env-overrides es coherente con el
  `MONEY_TRACKER_DB` actual.
- **Alternatives**: Password en disco → rechazado. Key secreta embebida → rechazado (bypasa RLS).

## R7. Gotchas y decisiones de detalle (aplican al diseño)

- **Fechas**: columna `date` (Postgres), no `timestamptz` — los JSON de V/O y los filtros de rango
  permanecen en el formato exacto `YYYY-MM-DD` que ya esperan modelos y bindings ts-rs.
- **Ids**: `bigint generated always as identity` (no `bigint` default). PostgREST serializa `int8`
  como JSON *number* → coincide con los overrides ts-rs `#[ts(type="number")]`. Ids < 2^53 verificado
  por el volumen personal.
- **Ids del espejo**: los ids los acuña el remoto vía `INSERT ... RETURNING` / `Prefere:
  return=representation`; el espejo hace upsert con el id devuelto. En la migración se mantiene un
  mapa en memoria `old_id → new_id` (los ids solo aparecen en `entry rm`/`entry edit`; ningún ref.
  persistente se rompe). No hay tabla de mapping persistente.
- **Atomicidad**: las operaciones read-modify-write (split de emergencia, cheques de overdraft) se
  envuelven en un RPC `rpc/apply_entries(jsonb)` (PL/pgSQL SECURITY INVOKER bajo transacción);
  PostgREST respeta RLS del llamador. "Última escritura ganadora" para ediciones concurrentes.
- **Mirror**: DDL del espejo estructuralmente idéntico al remoto (mismas columnas/CHECKs/índices). El
  `account_balances` del espejo sigue siendo la misma VIEW. La suite `scenarios.rs` corre contra ambos.
- **Realtime**: público sin JWT se corta a 24 h; si se adopta, subir la conexión con el JWT. Para v1:
  polling solamente.
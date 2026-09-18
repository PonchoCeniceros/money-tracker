# Data Model — Supabase como fuente de la base de datos

**Branch**: `001-supabase-backend` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Modelo de datos remoto (Supabase/Postgres), el contrato del espejo SQLite local y las estructuras
derivadas en memoria de `money_core`. Las entidades de dominio del spec se mapean directamente:

## 1. Entidades del *spec* → implementación

| Entidad (spec) | Tabla | Notas |
|---|---|---|
| Cuenta | `accounts` | misma semántica que el DDL SQLite (kind, target_amount, credit_limit, liquid, archived) + `user_id` |
| Entrada | `entries` | libro mayor: amount > 0, dirección por kind, CHECK de forma, date `YYYY-MM-DD` |
| Presupuesto | `budgets` | UNIQUE(concept, period); informativo, nunca bloquea |
| Concepto | `concepts` | catálogo con `concept_type` |
| Configuración | `config` | key/value (`emergency_pct`, `default_account`, `income_account`, `cash_concept`) |
| — | `sync_state` | fila única con `revision bigint` para detección de cambios inter-instalación |
| — | `account_balances` | VIEW derivada, expuesta solo lectura |

## 2. Esquema remoto (Postgres)

Cada tabla del libro mayor lleva `user_id uuid not null default auth.uid() references auth.users(id)` y
`updated_at timestamptz not null default now()`. Las CHECKs/índices replican el DDL SQLite:

- `accounts`: `id bigint identity PK`, `name text not null unique`, `kind check in
  ('spending','emergency','target','credit')`, `target_amount numeric check (null or > 0)` con CHECK
  `kind='target' or target_amount is null`, `credit_limit` análogo con `kind='credit'`, `liquid bool`
  default true, `archived bool` default false, índice único parcial `(kind) where kind='emergency'
  and archived=false`.
- `entries`: `id bigint identity PK`, `date date not null`, `kind check in ('income','expense',
  'transfer','opening')`, `amount numeric not null check (amount > 0)`, `from_account_id bigint
  references accounts(id)`, `to_account_id bigint references accounts(id)`, `concept text references
  concepts(name)`, `subconcept text`, `description text`, + CHECK de forma:
  `(kind in ('income','opening') and from is null and to is not null) or (kind='expense' and from is
  not null and to is null) or (kind='transfer' and from is not null and to is not null and from <> to)`.
- `budgets`: `concept`, `monthly_limit numeric check (> 0)`, `period` (text `YYYY-MM`, CHECK formato),
  `unique (concept, period)`.
- `concepts`: `name text unique`, `concept_type check in ('expense','income','both')`.
- `config`: `key text pk`, `value text not null`.
- VIEW `account_balances` `with (security_invoker = true)` — bal. por cuenta derivado de `entries`
  (suma a `to` − suma de `from`), mismo contrato que la VIEW SQLite.
- `sync_state`: `id int primary key check (id = 1)`, `revision bigint not null`. Trigger común en
  `accounts/entries/budgets/concepts/config`: `revision = revision + 1; new.updated_at = now()`.

`grant select on account_balances to authenticated; grant select, insert, update, delete on
accounts, entries, budgets, concepts, config to authenticated; revoke all from anon;` + políticas
RLS `using/with check ((select auth.uid()) = user_id)` en cada tabla.

Función RPC `apply_entries(entries jsonb, expected_balance numeric, account_id bigint)` — inserta
N entradas en una transacción y valida overdraft/cupo según reglas actuales; usada para el split de
emergencia (ingreso + transferencia) y escrituras read-modify-write.

## 3. Espejo local SQLite (contrato de paridad)

- El espejo usa el **mismo DDL** (columnas, tipo, CHECKs, índices, VIEW `account_balances`) que el
  esquema remoto; cambia el tipo de `id` (igual, `INTEGER PK AUTOINCREMENT` funcionando como bigint).
- `user_id`, `updated_at` y `sync_state` también existen en el espejo (el poll usa `updated_at` como
  high-water mark) aunque `user_id` siempre tenga el valor del usuario local en la sesión.
- **Escrituras del espejo**: solo vía el decorador `MirroringBackend` tras una operación remota
  exitosa (upsert por id) o vía `sync::refresh` (poll). Nunca escritura directa de handlers.
- Desviación permitida: tamaño de fechas (texto `YYYY-MM-DD` vs `date`); se normaliza a string al
  cruzar PostgREST.

## 4. Derivación en memoria (`storage/ledger.rs`)

`money_core` deja de depender de una sola vista SQL para la liquidación; un módulo puro deriva:

- `derive_balances(accounts, entries) -> Vec<AccountBalance>` — mismo resultado que `account_balances`
  (suma a `to` − suma de `from`, ROUND 2), incluye `progress_pct()`/`debt()` sobre modelos existentes.
- `monthly_report(period, entries, accounts, budgets)` — cálculo devengado vs salida real de
  efectivo, desgloses (flujo/ahorro/crédito), por concepto y presupuesto vs real — misma semántica
  que `report_service` actual.
- `net_worth(as_of, ...)` y `balance_as_of(account, date)` — desde snapshot del libro.

**Justificación**: un único origen en Rust de la matemática, testable con el mismo conjunto de datos
contra ambos backends (Principio I y IV); remotamente se evita duplicar agregaciones en SQL de
Postgres (Principio V).

## 5. Estado local persistido (fuera del libro)

- `~/.money-tracker/config.toml` (0600): `supabase_url`, `supabase_publishable_key`, `mirror_path`.
- Keyring: `refresh_token`, `email`.
- Env: `MONEY_TRACKER_SUPABASE_URL`, `MONEY_TRACKER_SUPABASE_KEY` (ganan sobre config),
  `MONEY_TRACKER_DB` (ruta del espejo para pruebas).
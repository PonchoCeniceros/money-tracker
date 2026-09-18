# Implementation Plan: Supabase como fuente de la base de datos

**Branch**: `001-supabase-backend` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-supabase-backend/spec.md`

## Summary

Sustituir el almacenamiento operativo del libro mayor: Supabase pasa a ser la fuente alojada única
de lectura/escritura (siempre en línea), conservando la base SQLite local como **espejo/respaldo**
que se actualiza tras cada operación exitosa. Un solo usuario autenticado por Supabase Auth (email +
password), con RLS sobre cada tabla para que nada se lea/escriba sin identidad válida. La propagación
entre instalaciones usa polling de un marcador de revisión (~30 s, < 1 min). Migración explícita de
un solo paso del historial existente.

**Enfoque técnico** (consolidado en [research.md](research.md)): acceso vía Data API de PostgREST
(HTTPS) con key publicable + JWT del usuario; las reglas contables actuales se replican en DDL de
Postgres (CHECKs, índice único parcial, view `security_invoker`); `money_core` adquiere una capa de
almacenamiento (`LedgerBackend`) con dos implementaciones (SQLite y Supabase) y **deriva saldos y
reportes en memoria** desde el libro mayor, de modo que la matemática financiera tiene un único
origen en Rust testable. La escritura atómica multi-fila (ingreso + split de emergencia) se expone
como RPC `apply_entries` para preservar la atomicidad hoy garantizada por transacciones locales.

## Technical Context

**Language/Version**: Rust 2021 (workspace: `money_core` lib, `cli` bin, `gui` Tauri v2) + TypeScript/React en `gui/src`. Cliente de red: `reqwest` (rustls) via feature-gate tras la capa de almacenamiento.

**Primary Dependencies**: Para la capa Supabase: `reqwest` (HTTP/HTTPS, Tauri/CLI), `postgrest-rs` (construcción de filtros sobre la Data API) o `reqwest` directo si se prefiere minimizar superficie; `keyring` (macOS Keychain / Windows Credential Manager / Linux Secret Service) para el refresh token; `serde_json` ya presente. GoTrue se llama con 2 POSTs de `reqwest` (login y refresh); se evita adoptar un SDK Supabase monolítico (inmaduro). Frontend: querida sin cambios de librería (los bindings ts-rs ya traen `number`).

**Storage**: PostgreSQL alojado (Supabase) como fuente de verdad; SQLite local (rusqlite) como espejo/respaldo con esquema paritario. Las reglas de integridad se duplican en ambos DDLs.

**Testing**: `cargo test --workspace` — la suite existente (44 tests: unit por servicio + `scenarios.rs` de caja negra) debe seguir pasando contra `SqliteBackend` (in-memory). Se añade un arnés backend-agnóstico para la suite de escenarios y una suite de humo contra un Supabase local (`supabase db start`) o instancia de desarrollo. Frontend: `tsc --noEmit`. Portones: `cargo build --workspace`, `cargo clippy --workspace --all-targets`.

**Target Platform**: macOS + Linux (CLI) y window nativa Tauri (GUI), contra un servicio alojado. Sin móvil.

**Project Type**: Library + CLI + desktop app (Tauri), integrados en un workspace Rust.

**Performance Goals**: Propagación inter-instalación < 1 min (poll ~30 s). Operaciones contables: ida y vuelta HTTPS < 2 s percibido. Volumen: libro personal (miles de filas/año) → derivación en memoria trivial.

**Constraints**: Sistema DEBE operar siempre en línea (sin operaciones offline; rechazo claro sin red). El espejo SQLite no debe recibir escrituras directas durante la operación normal (solo réplica). Credenciales de sesión fuera del repo. Los `i64` (ids) deben viajar como números JSON (`bigint` en PG, `number` en ts-rs). Fechas siguen como texto `YYYY-MM-DD` (columna `date` en PG, no `timestamptz`).

**Scale/Scope**: Un solo usuario (misma persona, varias instalaciones). Sin roles ni cuentas compartidas. Sin motor de resolución de conflictos (última escritura ganadora en la fuente alojada).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Honrado tras el rediseño (ver [research.md](research.md) para el detalle):

- **Principio I (Modelo Primero)**: toda la lógica de negocio y derivación financiera permanece en
  `money_core` (derivación de saldos/reportes en memoria, cálculo de split, orquestación del espejo).
  `money_core` sigue siendo librería pura: no imprime, no pregunta, no parsea argumentos ni formatea
  salida. `reqwest` no es dependencia CLI/GUI, por lo que el árbol de `money_core` sigue sin
  `clap`/`dialoguer`/`tabled`/`tauri`.
- **Principio II (Handlers delgados)**: `cli` y `gui` solo construyen el backend de sesión (login,
  entorno) y delegan; No reimplementan consultas ni reglas. El polling inter-instalación y el
  refresco del espejo viven en `money_core` (módulo `sync`), no en los handlers.
- **Principio III (El libro mayor es la única fuente de verdad)**: los saldos jamás se almacenan
  (se derivan en memoria y como VIEW en ambos DDLs); las reglas no negociables siguen impuestas por
  el esquema (CHECKs + índice único parcial) en Postgres y reforzadas con RLS.
- **Principio IV (Test-First)**: los 44 tests existentes se preservan contra `SqliteBackend`; los
  invariantes (split, transferencias, `opening`, rechazo de esquema legacy, presupuesto no bloqueante)
  se cubren contra ambos backends.
- **Principio V (Simplicidad)**: se descartan: driver Postgres nativo (evasión de RLS), SDK Supabase
  monolítico (inmaduro), Realtime como mecanismo base (reemplazado por polling simple), y edición
  offline. La complejidad inherente (trait de almacenamiento, RPC atómico) se justifica en
  Complexity Tracking.

**GATE: APROBADO — sin violaciones injustificadas.**

## Project Structure

### Documentation (this feature)

```text
specs/001-supabase-backend/
├── plan.md              # Este archivo
├── research.md          # Phase 0: decisiones técnicas + alternativas
├── data-model.md        # Phase 1: entidades remotas, mirror, derivación
├── quickstart.md        # Phase 1: guía de validación end-to-end
├── contracts/           # Phase 1: contratos de interfaz
│   ├── storage-contract.md     # Trait LedgerBackend (operaciones de dinero_core)
│   ├── remote-api-contract.md  # Endpoints/RPCs/auth de Supabase
│   ├── mirror-contract.md      # Paridad de esquema y semántica del espejo
│   └── cli-gui-contract.md     # Superficie nueva de CLI y GUI
└── tasks.md             # Phase 2 (/speckit.tasks — no lo crea /speckit.plan)
```

### Source Code (repository root)

```text
money_core/
├── Cargo.toml                     # + deps: reqwest, keyring, (feat remota)
├── src/
│   ├── db.rs                      # DDL SQLite (igual que hoy) + open_db() para el espejo/pruebas/migración
│   ├── storage/
│   │   ├── mod.rs                 # trait LedgerBackend + tipos de consulta
│   │   ├── sqlite.rs              # SqliteBackend (las queries hoy en services se mudan aquí)
│   │   ├── remote.rs              # SupabaseBackend (PostgREST + GoTrue + session)
│   │   └── ledger.rs              # Derivación en memoria (balances, reporte mensual, net_worth)
│   ├── sync/
│   │   ├── mod.rs                 # MirroringBackend (decorador remoto→espejo) + poller de revisión
│   │   └── migrate.rs             # Migración de un solo paso (local → remoto, remapea ids)
│   ├── models/                    # sin cambios de forma (ids i64 → number via ts-rs)
│   ├── services/                  # reescritos sobre &dyn LedgerBackend
│   └── error.rs                   # + variantes Remote/Network/Auth
├── tests/scenarios.rs             # arnés backend-agnóstico (SqliteBackend in-memory + opcional Supabase)
supabase/
└── migrations/
    └── 0001_initial.sql           # DDL Postgres: tablas + CHECKs + índice único parcial
                                   #   + view account_balances (security_invoker) + RLS + RPC apply_entries
                                   #   + sync_state (revision bump) + updated_at
cli/src/commands/
└── db/
    ├── remote_status.rs           # db remote status
    ├── remote_login.rs            # db remote login (email+password → keyring)
    ├── remote_logout.rs           # db remote logout
    └── remote_migrate.rs          # db remote migrate (un solo paso)
gui/src-tauri/src/
├── state.rs                       # AppState { backend: Mutex<dyn LedgerBackend>, session, mirror }
└── commands/{auth, sync, ...}.rs  # wrappers delgados (login/logout/status, existentes reciben backend)
gui/src/
├── routes/Settings.tsx            # login/logout de Supabase + estado de sincronización
└── hooks/useSync.ts               # poll ~30 s → bumpRevision()
```

**Structure Decision**: Se elige la estructura "library + dos handlers" existente, sin proyectos
nuevos. La novedad es una capa `storage/` y `sync/` dentro de `money_core` (no crates nuevos) y un
directorio `supabase/migrations/` versionado en el repo. Se rechaza agregar un crate de infraestructura separado (Principio V).

## Complexity Tracking

> La gobernanza exige justificar el aumento de complejidad contra el Principio V.

| Violación | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Trait `LedgerBackend` + dos implementaciones | Dos roles de almacenamiento reales e ineludibles: fuente alojada autoritativa (remota) y espejo/pruebas/migración (SQLite). Sin el trait, la lógica de servicios se duplicaría entre SQL y REST | Reescribir servicios directamente contra PostgREST y mantener los actuales para SQLite (duplicación total de cada servicio). Conservar SOLO SQLite local-first (contradice la decisión aclarada: Supabase es la única vía de escritura) |
| Derivación en memoria de saldos/reportes | La fuente remota no impone una única vista; computar en Rust da un único origen testable de la matemática y funciona igual contra espejo y remoto | Duplicar las agregaciones como VIEWs/funciones en Postgres además de las de SQLite (dos implementaciones de la misma regla, riesgo de divergencia) |
| RPC atómico `apply_entries` (PL/pgSQL) | El split de emergencia (ingreso + transferencia, con cheques de overdraft) hoy es una transacción local; en REST stateless no hay atomicidad | Insertar filas por POSTs separados sin transacción (riesgo de split parcialmente aplicado), o introducir sincronización/coordinator extra |
| RLS en todas las tablas + columna `user_id` | La key publicable viaja en el binario; RLS es la única barrera entre un key extraído y los datos. Modelo de un solo usuario no exime | Omisión de RLS por "un solo usuario" (exposición total si un tercero acepta la key del binario) |

## Fases

- **Phase 0 — Research**: [research.md](research.md) (decisiones: protocolo, auth+RLS, DDL, esquema migrate, propagación, credenciales, gotchas).
- **Phase 1 — Design & Contracts**: [data-model.md](data-model.md) + [contracts/](contracts/) + [quickstart.md](quickstart.md). Re-check de la Constitución: **APROBADO** (arriba).
- **Phase 2 — Tasks**: `/speckit.tasks` (fuera de alcance de este comando).
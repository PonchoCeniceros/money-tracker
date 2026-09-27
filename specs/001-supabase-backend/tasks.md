---

description: "Lista de tareas para la implementación de Supabase como base de datos"

---

# Tareas: Supabase como fuente de la base de datos

**Entrada**: Documentos de diseño de `/specs/001-supabase-backend/`

**Prerrequisitos**: [plan.md](plan.md) (obligatorio), [spec.md](spec.md) (obligatorio para user stories), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/)

**Pruebas**: El proyecto exige Test-First (Constitución, Principio IV): la suite existente debe seguir
pasando y `scenarios.rs` se vuelve backend-agnóstico. Los tests se escriben y fallan antes de la
implementación que resuelven.

**Organización**: Tareas agrupadas por user story para permitir implementación y pruebas independientes de cada historia.

## Formato: `[ID] [P?] [Historia] Descripción`

- **[P]**: Puede correr en paralelo (archivos distintos, sin dependencias)
- **[Historia]**: A qué user story pertenece la tarea (p. ej. US1, US2, US3)
- Incluir rutas de archivo exactas en las descripciones

## Convenciones de rutas

- **Workspace Rust**: `money_core/`, `cli/`, `gui/src-tauri/` (raíz del repo), `gui/src/` (frontend), `supabase/migrations/`

---

## Fase 1: Configuración (Infraestructura compartida)

**Propósito**: Inicialización del proyecto y estructura básica

- [X] T001 Agregar dependencias a `money_core/Cargo.toml`: `reqwest` (rustls), `postgrest-rs`, `keyring` y `serde_json` (si no está). Mantener la feature `ts-rs` desactivada por defecto; no tocar `serde`/`thiserror`.
- [X] T002 Crear `supabase/migrations/0001_initial.sql` — DDL Postgres según [data-model.md](data-model.md) §2 con constraints VERBATIM: `amount numeric not null check (amount > 0)`; `kind check in ('income','expense','transfer','opening')`; CHECK de forma `(kind in ('income','opening') and from is null and to is not null) or (kind='expense' and from is not null and to is null) or (kind='transfer' and from is not null and to is not null and from <> to)`; índice único parcial `accounts (kind) where kind='emergency' and archived=false`; `create unique index ... on budgets (concept, period)`; VIEW `account_balances with (security_invoker = true)`; `sync_state` fila única; triggers `revision = revision + 1; new.updated_at = now()` en accounts/entries/budgets/concepts/config; grants (`revoke all from anon; grant ... to authenticated`); políticas RLS `using/with check ((select auth.uid()) = user_id)` en cada tabla; función RPC `apply_entries`.
- [X] T003 [P] Crear `money_core/src/settings.rs` — carga de `~/.money-tracker/config.toml` (modo 0600): `supabase_url`, `supabase_publishable_key`, `mirror_path`; las env `MONEY_TRACKER_SUPABASE_URL`/`MONEY_TRACKER_SUPABASE_KEY` ganan sobre el archivo; `MONEY_TRACKER_DB` sigue siendo la ruta del espejo (reusar `db_path()` de `money_core/src/db.rs`).
- [X] T004 [P] Agregar variantes a `AppError` en `money_core/src/error.rs`: `Remote(String)`, `Network(#[from] reqwest::Error)`, `Auth(String)`, `InvalidGrant`. Mantener `From<rusqlite::Error>` y las variantes actuales intactas.

---

## Fase 2: Fundamentos (Prerrequisitos bloqueantes)

**Propósito**: Infraestructura núcleo que DEBE completarse antes de implementar cualquier user story

**⚠️ CRÍTICO**: Ningún trabajo de user story puede comenzar hasta que esta fase esté completa

- [X] T005 Definir el trait `LedgerBackend` en `money_core/src/storage/mod.rs` según [contracts/storage-contract.md](contracts/storage-contract.md): métodos de accounts (insert/list/get/find_by_name/emergency/archive), entries (insert/update/delete/get/list con `EntryFilter`), `apply_income_with_split`, CRUD config/budgets/concepts, `accounts_snapshot`/`entries_snapshot`, y sync (`remote_revision`, `pull_changes_since(LedgerDelta)`, `apply_remote_snapshot`). `type Error: Into<AppError>`.
- [X] T006 [P] Implementar `SqliteBackend` en `money_core/src/storage/sqlite.rs` moviendo las queries que hoy viven en `money_core/src/services/*.rs` (accounts, entries+filtro, config, presupuestos, conceptos). `apply_income_with_split` usa transacción local reutilizando la semántica de `with_checked_source` (sobregiro y cupo de crédito). `insert_entry` devuelve la fila con `last_insert_rowid` y nombres de cuenta.
- [X] T007 [P] Implementar `SupabaseBackend` en `money_core/src/storage/remote.rs` según [contracts/remote-api-contract.md](contracts/remote-api-contract.md): cliente `reqwest`; CRUD por PostgREST con headers `apikey: <publishable>` + `Authorization: Bearer <jwt>`; filtros de fecha `?date=gte..&date=lte..`; joins de nombres vía `?select=*,from_account:accounts!...(...),to_account:...`; `Prefer: return=representation&select=id` para leer los ids acuñados; `apply_income_with_split` vía `POST /rest/v1/rpc/apply_entries`; `remote_revision` leyendo `sync_state`; `pull_changes_since` con `updated_at > watermark`; mapeo de errores Postgres (`23505`, `23514`) a `AppError::Invalid` con mensaje accionable.
- [X] T008 [P] Implementar `money_core/src/storage/ledger.rs` — derivación en memoria pura: `derive_balances(accounts, entries) -> Vec<AccountBalance>` con la semántica de la VIEW `account_balances` (suma a `to` − suma de `from`, ROUND 2), `monthly_report(period, entries, accounts, budgets)` replicando la semántica del `report_service` actual (gasto devengado vs salida real de efectivo + desgloses flujo/ahorro/crédito + by_concept + budget vs actual), `net_worth(as_of)` y `balance_as_of(account, date)`.
- [X] T009 Refactorizar `money_core/src/services/*.rs` (account_service, entry_service, report_service, setup_service) para operar sobre `&dyn LedgerBackend` en lugar de `&rusqlite::Connection`; los reportes delegan en `ledger.rs`; eliminar el SQL inline de los servicios. `setup_service::seed` y `add_income_with_emergency_split` pasan por `apply_income_with_split`.
- [X] T010 [P] Crear `money_core/src/auth.rs` — GoTrue: `login(email, password)` y `refresh(refresh_token)` vía `POST /auth/v1/token?grant_type=password|refresh_token`; `logout(jwt)`; sesión en memoria; persistencia del `refresh_token`+email en OS keyring (crate `keyring`) con fallback a archivo `0600` en headless; **nunca persiste el password**; ante `invalid_grant` → `AppError::InvalidGrant`.
- [X] T011 Implementar `money_core/src/sync/mod.rs` — `LedgerMirror` (wrapper de `SqliteBackend`), decorador `MirroringBackend` que tras cada éxito remoto hace upsert por id en el espejo (falla del espejo → advertencia no silenciosa), y `sync::poll_once(backend, mirror, watermark) -> Result<bool>` que compara `remote_revision` y, si cambió, aplica `pull_changes_since` al espejo (contrato [mirror-contract.md](contracts/mirror-contract.md) §2).
- [X] T012 Refactorizar `cli/src/main.rs` y `cli/src/commands/*.rs` para construir el backend desde `settings.rs` + sesión de `auth.rs` y delegar en `&dyn LedgerBackend`; reemplazar el uso directo de `db::open_db()` (excepto bootstrap del espejo y `db status/reset` que siguen operando sobre el archivo); los comandos de escritura fallan con `AppError::Auth` claro si no hay sesión.
- [X] T013 [P] Refactorizar `gui/src-tauri/src/state.rs` (`AppState { backend: Mutex<Box<dyn LedgerBackend>>, session: ... }`) y `gui/src-tauri/src/commands/*.rs` para usar el backend; ampliar `From<AppError>` en `gui/src-tauri/src/error.rs` a los nuevos variants (`Remote`/`Network`/`Auth`/`InvalidGrant`).
- [ ] T014 Refactorizar `money_core/tests/scenarios.rs` a un harness backend-agnóstico (`Box<dyn LedgerBackend>`): por defecto `SqliteBackend` in-memory (los 44 tests actuales siguen pasando, mismos asserts); si `MONEY_TRACKER_SUPABASE_URL` está presente, correr también contra `SupabaseBackend`. Escribir los tests antes/durante T005–T009 para que fallen primero.

**Punto de control**: Fundamentos listos — la implementación de user stories puede comenzar en paralelo

---

## Fase 3: User Story 1 - Base de datos alojada como única fuente de verdad (Prioridad: P1) 🎯 MVP

**Objetivo**: Todas las operaciones actuales (gasto, ingreso+split, transferencia, setup, cuadre, presupuesto, reporte, config, conceptos) leen/escriben en Supabase; el espejo SQLite se actualiza tras cada éxito. Cero regresión de cifras.

**Prueba independiente**: quickstart.md §1 + §6 (smoke: login → setup → income con split → report; paridad OK en `db remote status`; suite completa pasando sobre ambos backends).

- [X] T015 [US1] Mover los handlers CLI/GUI con SQL crudo (conceptos, presupuestos, config en `cli/src/commands/{concept,budget,config}.rs` y `gui/src-tauri/src/commands/{concepts,budgets,config}.rs`) a los métodos del `LedgerBackend` definidos en T005/T006/T007 — eliminar todo SQL directo de los handlers (Principio II).
- [X] T016 [P] [US1] Implementar `db remote login` y `db remote logout` en `cli/src/commands/db.rs` (delegan en `money_core/src/auth.rs`; `login` con flags `--email`/`--password`/`--yes` o dialoguer; `logout` revoca y limpia keyring).
- [X] T017 [US1] Agregar el arranque de sesión en `cli/src/main.rs`/`cli/src/commands/helpers.rs`: si no hay sesión ni credenciales env válidas, error claro indicando el comando de login antes de operar; si hay refresh token, refrescar una vez y reintentar la operación (regla del [contracts/remote-api-contract.md](contracts/remote-api-contract.md) §1).
- [X] T018 [US1] Agregar panel de inicio de sesión de Supabase en `gui/src/routes/Settings.tsx` (email+password → `auth::login`) con persistencia vía keyring y guard de sesión; `gui/src/routes/SetupWizard.tsx` opera contra el remoto (login primero, luego seed).
- [ ] T019 [US1] Verificar US1 ejecutando [quickstart.md](quickstart.md) §1 y §6 contra un Supabase de desarrollo: paridad espejo/remoto OK en `db remote status`; reporte devengado vs salida real idéntico al cálculo de `ledger.rs`; los 44 tests pasan.

**Punto de control**: En este punto, la User Story 1 debe ser totalmente funcional y testeable de forma independiente

---

## Fase 4: User Story 2 - Acceso desde cualquier lugar y dispositivo (Prioridad: P2)

**Objetivo**: Dos instalaciones contra el mismo libro: los cambios de una aparecen en la otra en < 1 min (poll ~30 s) y cada una mantiene su espejo al día por polling.

**Prueba independiente**: quickstart.md §4 — GUI y CLI con espejos distintos; un `transfer` del CLI aparece en la GUI en < 1 min sin reload manual.

- [X] T020 [P] [US2] Implementar `db remote status` en `cli/src/commands/db.rs` usando `money_core/src/sync`: muestra sesión (email/expiración), `revision` remota, watermark del espejo, totales por tabla y paridad espejo vs remoto (contrato [cli-gui-contract.md](contracts/cli-gui-contract.md) §1).
- [X] T021 [US2] Implementar el `sync::poll_once` del lado GUI: comando Tauri delgado `sync_poll` en `gui/src-tauri/src/commands/sync.rs` (llama a `money_core::sync::poll_once` con el watermark del espejo) y hook `gui/src/hooks/useSync.ts` que lo ejecuta cada ~30 s y, si la revisión cambió, invoca `bumpRevision()` de `gui/src/hooks/useApi.ts`; añadir banner/estado de sincronización en `gui/src/routes/Settings.tsx`.
- [X] T022 [US2] Asegurar que el SetupWizard y el login detectan revisión remota no vacía (no machacar data): antes de seed/setup remoto, `sync_state` con revision > 0 + entries > 0 → confirmación explícita.
- [ ] T023 [US2] Verificar US2 con [quickstart.md](quickstart.md) §4 (dos instalaciones, espejos distintos, propagación < 1 min sin pérdida/duplicación).

**Punto de control**: En este punto, las User Stories 1 Y 2 deben funcionar ambas de forma independiente

---

## Fase 5: User Story 3 - Migración de los datos existentes (Prioridad: P3)

**Objetivo**: Traslado explícito en un solo paso del historial local (cuentas, entradas, presupuestos, conceptos, config) al remoto preservando cifras; rechazo de esquema legacy; recuperación íntegra desde el espejo ante pérdida del remoto.

**Prueba independiente**: quickstart.md §3 y §5 — migración de una base local poblada con reportes de período idénticos antes/después; restauración completa desde el espejo simulando pérdida del remoto.

- [X] T024 [US3] Implementar `money_core/src/sync/migrate.rs`: `migrate(source: Option<&Path>, remote, mirror)` — valida el esquema del source (base legacy `transactions`/`buckets` → `AppError::LegacySchema` con ruta); comprueba el remoto (data distinta → error salvo `--force`); copia en orden concepts → config → budgets → accounts → entries con remapeo de ids en memoria (data-model.md §2/§3); puebla el espejo desde cero con `apply_remote_snapshot`.
- [X] T025 [US3] Implementar `db remote migrate` en `cli/src/commands/db.rs` con flags `--source <ruta>` (por defecto `MONEY_TRACKER_DB`/archivo actual) y `--force`, delegando en `money_core/src/sync/migrate.rs` (contrato [cli-gui-contract.md](contracts/cli-gui-contract.md) §1).
- [ ] T026 [US3] Implementar verificación de integridad/restauración desde el espejo en `money_core/src/sync/mod.rs` (`PRAGMA integrity_check` + comparación de totales vs `remote_revision`) expuesta en `db remote status`; documentar el procedimiento de restauración (lectura íntegra del espejo) en [quickstart.md](quickstart.md) §5.
- [ ] T027 [US3] Verificar US3 con [quickstart.md](quickstart.md) §3 y §5: migración con base real (reportes idénticos, SC-003), fallo accionable sobre esquema legacy (FR-007) y recuperación completa desde el espejo (SC-005).

**Punto de control**: Todas las user stories deben ser ahora funcionales de forma independiente

---

## Fase 6: Pulido y preocupaciones transversales

**Propósito**: Mejoras que afectan a múltiples user stories

- [X] T028 [P] Actualizar `AGENTS.md` y `README.md`: comandos `db remote *`, env `MONEY_TRACKER_SUPABASE_URL`/`MONEY_TRACKER_SUPABASE_KEY`, semántica del espejo de `MONEY_TRACKER_DB`, flujo de migración de `supabase db push`.
- [X] T029 [P] Regenerar los bindings de la GUI si cambiaron los modelos exportados (`cargo test -p money_core --features ts-rs`) y verificar que `gui/src/bindings/*.ts` sigan con `number`/`number | null` en ids.
- [X] T030 [P] Ejecutar las puertas de calidad: `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `tsc --noEmit` (en `gui/`) — todo en verde.
- [ ] T031 Limpieza: eliminar SQL muerto en `money_core/src/services/*.rs` y handlers; mantener `db status`/`db reset` sobre el archivo (espejo); verificar que ningún flujo normal muta el espejo fuera del decorador/poll.
- [ ] T032 [P] Revisión de seguridad: confirmar RLS en TODAS las tablas/VIEW (nada legible para `anon`), ausencia de secretos versionados (`.gitignore` `~/.money-tracker/`), refresh token solo en keyring/0600, key publicable solo en config/env documentada en [contracts/remote-api-contract.md](contracts/remote-api-contract.md).

---

## Dependencias y orden de ejecución

### Dependencias de fases

- **Setup (Fase 1)**: Sin dependencias — puede comenzar de inmediato
- **Fundamentos (Fase 2)**: Dependen del fin de Setup — BLOQUEAN todas las user stories
- **User Stories (Fase 3+)**: Todas dependen del fin de la fase de Fundamentos
  - Se recomienda secuencial (P1 → P2 → P3) para este proyecto (desarrollador único)
- **Pulido (Fase final)**: Depende de que todas las user stories deseadas estén completas

### Dependencias entre user stories

- **User Story 1 (P1)**: Depende de Fundamentos (T005–T014). Sin dependencia de otras historias. **Es el MVP.**
- **User Story 2 (P2)**: Depende de Fundamentos + US1 (necesita sesión y backends operativos; `sync` usa `MirroringBackend` ya presente). Testeable independientemente con dos instalaciones.
- **User Story 3 (P3)**: Depende de Fundamentos + US1 (necesita `SupabaseBackend`, espejo y auth). Usa el remoto→espejo pero es independiente de US2.

### Dentro de cada user story

- Las tareas de prueba (T014, T019, T023, T027) se escriben/verifican contra la historia correspondiente; los invariantes de regresión (Principio IV) se mantienen en verde en todo momento.
- Modelos/trait antes que servicios; servicios antes que handlers; core antes que integración.

### Oportunidades de paralelismo

- Setup: T002 y el par (T003, T004) en paralelo
- Fundamentos: T006, T007, T008, T010 en paralelo (archivos distintos); T009 y T011 dependen de T005/T006/T007; T012/T013 dependen de T005–T011; T014 corre en paralelo como harness
- US1: T015 (handlers) paralelo a T016 (db login/logout)
- US2: T020 paralelo a T021/T022 (GUI)
- Las historias no deben tocarse entre sí salvo integración final

---

## Ejemplo de paralelismo: User Story 1

```bash
# Modelos/backends (en Fundamentos) en paralelo:
Tarea: "Implementar SqliteBackend en money_core/src/storage/sqlite.rs (T006)"
Tarea: "Implementar SupabaseBackend en money_core/src/storage/remote.rs (T007)"


# Handlers de US1 en paralelo:
Tarea: "Mover CRUD raw de concepts/budgets/config a LedgerBackend en cli/src/commands/*.rs y gui/src-tauri/src/commands/*.rs (T015)"

Tarea: "Implementar db remote login/logout en cli/src/commands/db.rs (T016)"
```

---

## Estrategia de implementación

### MVP primero (solo User Story 1)

1. Completar la Fase 1: Setup (deps, migración SQL, settings, errores)
2. Completar la Fase 2: Fundamentos (CRÍTICO — bloquea todas las historias)
3. Completar la Fase 3: User Story 1 + verificación quickstart §1/§6
4. **DETENERSE y VALIDAR**: mismo reporte, espejo paritario, 44 tests en verde
5. Deploy/demo: `supabase db push` a producción y primera operación real

### Entrega incremental

1. Setup + Fundamentos → Fundamentos listos
2. User Story 1 → Prueba → (MVP)
3. User Story 2 → Prueba de propagación
4. User Story 3 → Migración de la base real
5. Pulido → Documentación, puertas de calidad, revisión de seguridad

### Estrategia para equipos paralelos

Con un solo desarrollador: secuencial. Con equipos: A=US1, B=US2 (tras Fundamentos), C=US3
(tras US1), integrando por fases sin compartir archivos.

---

## Notas

- [P] tareas = archivos distintos, sin dependencias
- [Historia] etiqueta cada tarea a su user story para trazabilidad
- Cada user story debe ser completable y testeable de forma independiente
- Los tests (T014, validaciones de quickstart) se escriben antes y fallan antes que la implementación
- Commit al terminar cada tarea o grupo lógico
- Detenerse en cualquier punto de control para validar la historia de forma independiente
- Evitar: tareas vagas, conflictos por el mismo archivo, dependencias entre historias que rompan la independencia
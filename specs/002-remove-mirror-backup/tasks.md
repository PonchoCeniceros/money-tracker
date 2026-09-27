---
description: "Tasks for 002-remove-mirror-backup"
---

# Tasks: Supabase como única base de datos, con respaldos y esquema versionado

**Input**: Design documents from `/specs/002-remove-mirror-backup/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Incluidos. La constitución (principio IV) exige pruebas de las reglas y cobertura de regresión, y el spec las
pide explícitamente (FR-019, SC-006, SC-008). Donde se indica "primero", la prueba se escribe antes que la
implementación y debe fallar.

**Organization**: Por historia de usuario. **Nota de orden**: el núcleo de US4 (reglas al dominio y `MemoryBackend`) y
el archivo SQL `0002` (de US3) van en *Foundational*, porque US1 y US2 no pueden avanzar sin ellos (ver plan, "Orden de
implementación sugerido"). Las fases de US3 y US4 conservan lo que es propio de cada historia.

**Convenciones**: rutas relativas a la raíz del repo; `rN` = sección de [research.md](research.md); los contratos van en
[contracts/](contracts/). Tareas marcadas **(manual)**: las hace el usuario en el dashboard de Supabase o en su máquina.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro archivo, sin depender de tareas pendientes)
- **[Story]**: US1, US2, US3, US4

---

## Phase 1: Setup

**Purpose**: Preparar el repo sin cambiar comportamiento.

- [X] T001 Correr `cargo test --workspace` y anotar el total de pruebas que pasan (hoy 54: 46 de `money_core` + 8
      de escenarios) como línea base para SC-006, en `specs/002-remove-mirror-backup/checklists/requirements.md`,
      sección Notes.
      **Hecho (2026-09-26)**: línea base = 54 pruebas (46 `money_core` + 8 escenarios). Anotado aquí y no en el checklist,
      porque `/speckit.implement` prohíbe modificarlo. Además se corrigió una condición de carrera previa entre las dos
      pruebas de `settings.rs`, que modifican variables de entorno globales en paralelo.
- [X] T002 `git mv supabase/migrations/0001_initial.sql supabase/sql/0001_setup.sql` **sin cambiar ni un byte** del
      contenido (FR-013); borrar la carpeta vacía `supabase/migrations/`; crear `supabase/tests/`.
- [X] T003 En `money_core/Cargo.toml`: agregar `test-support = []` en `[features]` y, en `[dev-dependencies]`,
      `money_core = { path = ".", features = ["test-support"] }` (r3). Confirmar que `cargo test -p money_core` compila.

---

## Phase 2: Foundational (bloquea todas las historias)

**Purpose**: Reglas en el dominio, almacén en memoria, `0002` y API nueva del almacén. SQLite **sigue existiendo** al
terminar esta fase, y todas las pruebas deben pasar.

### Reglas al dominio (núcleo de US4)

- [X] T004 Primero: crear `money_core/src/rules.rs` solo con pruebas `#[cfg(test)]` que fallen, según la tabla de
      "Validaciones del dominio" de [data-model.md](data-model.md):
      - `check_source`: retiro de `target`/`emergency` mayor al saldo → error; tarjeta con límite 3000 y saldo -1902.38:
        cargo de 1097.62 pasa, de 1097.63 falla, de 2000 falla; tarjeta sin límite → pasa; `spending` nunca bloquea.
      - `check_new_emergency`, `validate_budget_limit` (`limit <= 0` → error), `validate_concept_type` (solo
        `expense`/`income`/`both`).
      - `validate_entry_update`: los casos de `update_rejects_setting_the_wrong_side_for_the_kind` y
        `update_rejects_turning_a_transfer_into_a_self_transfer` (`entry_service.rs`).
      - `emergency_split`: destino líquido + fondo activo → `Some(amount * pct / 100)`; destino restringido → `None`;
        sin fondo → `None`.
- [X] T005 Implementar las funciones de `money_core/src/rules.rs` (r1) portando la lógica y los **textos de error
      exactos** de `storage/sqlite.rs:198-230` (sobregiro/crédito, deuda = `max(-saldo, 0)`), `sqlite.rs:366-421`
      (forma al editar), `sqlite.rs:576` (tipo de concepto) y `sqlite.rs:606` (presupuesto). Registrar
      `pub mod rules;` en `money_core/src/lib.rs`. T004 pasa.
- [X] T006 Implementar `money_core/src/storage/memory.rs` (`MemoryBackend`), según la sección "Garantías" y
      "`MemoryBackend`" de [storage-contract.md](contracts/storage-contract.md), con el trait **actual** (r3):
      - estado en un `Mutex`; ids desde 1; nombres únicos de cuenta y concepto → `AppError::Invalid`; ids inexistentes
        → `AppError::NotFound`;
      - orden: cuentas por `kind, name`; movimientos por `date desc, id desc`; conceptos, presupuestos y configuración
        como en `sqlite.rs:522-646`;
      - `push_entries` atómico; `MemoryBackend::seeded()` con los 10 conceptos de `db.rs:9-20` y `emergency_pct = 10`;
      - declarar `pub mod memory;` en `money_core/src/storage/mod.rs` con `#[cfg(any(test, feature = "test-support"))]`.
- [X] T007 En `money_core/src/services/entry_service.rs`:
      - antes de `be.push_entries`, validar cada movimiento con `rules::check_source`, contra los saldos de
        `be.list_accounts(true)` ya ajustados por los movimientos anteriores del mismo lote;
      - `add_income_with_emergency_split` usa `rules::emergency_split` (la regla de `:71-103` sale de ahí);
      - la edición llama a `rules::validate_entry_update(&be.get_entry(id)?, upd)`, sin revalidar sobregiro
        (se conserva el comportamiento de `:128`);
      - agregar `pub fn emergency_split_preview(be, to_account_id, amount) -> Result<Option<SplitPreview>>`, con
        `SplitPreview { pct: f64, amount: f64 }`.
- [X] T008 [P] En `money_core/src/services/account_service.rs`, `create_account` llama a
      `rules::check_new_emergency(be.emergency_account()?.as_ref())` cuando `kind == AccountKind::Emergency`.
- [X] T009 [P] Crear `money_core/src/services/budget_service.rs` (`set`, que valida con `rules::validate_budget_limit`
      y luego llama a `be.set_budget`; `list`; `remove`) y `money_core/src/services/concept_service.rs` (`list`; `add`,
      que valida con `rules::validate_concept_type`). Registrarlos en `money_core/src/services/mod.rs` (r2).
- [X] T010 Cambiar las funciones que crean el almacén en las pruebas por `MemoryBackend::seeded()`: `setup()` en
      `money_core/src/services/account_service.rs:108`, `entry_service.rs:143` y `report_service.rs:106`; `setup_db()`
      en `setup_service.rs:53`; `fresh_db()` en `money_core/tests/scenarios.rs:17`. **No tocar ninguna aserción.**
      Agregar en `entry_service.rs` la prueba `batch_is_atomic_when_second_entry_overdraws` (porta
      `push_entries_is_atomic_on_overdraft` de `sqlite.rs`). Las 33 pruebas de servicios y escenarios pasan.
- [X] T011 Los handlers usan los servicios nuevos:
      - CLI: `cli/src/commands/budget.rs:128,144` → `budget_service`; `concept.rs:35,60`, `add.rs:57` e `income.rs:57`
        → `concept_service`;
      - GUI: `gui/src-tauri/src/commands/budgets.rs` y `concepts.rs` → los mismos servicios.
- [X] T012 Quitar de `money_core/src/storage/remote.rs` las validaciones duplicadas que ya viven en `rules.rs`: la forma
      al editar en `update_entry` (`:378-444`), el límite en `set_budget` (`:534`) y el tipo en `add_concept` (`:524`).
      El SQL de Supabase sigue siendo la segunda defensa.

### Esquema `0002` y API nueva del almacén

- [X] T013 Crear `supabase/sql/0002_schema_version.sql` con los puntos 1–8 de "Contenido de `0002`" de
      [schema-contract.md](contracts/schema-contract.md) y r6. Requisitos:
      - todo entre `begin;` y `commit;`;
      - guarda inicial: abortar si `to_regclass('public.entries') is null` o si
        `to_regclass('public.schema_version') is not null`;
      - `apply_entries` con la **misma firma** (`returns table(entry_id bigint, …)`), `perform … for update` sobre la
        cuenta origen, validación de la cuenta destino y la fórmula `greatest(-v_balance, 0) + v_amt > v_credit_lim`;
      - `ledger_status()` y `export_ledger()`: `returns jsonb`, `security invoker`, `stable`, con la forma JSON exacta
        del contrato (arreglos vacíos como `[]`);
      - al final, `insert into public.schema_version (id, version) values (1, 2)`.
- [X] T014 [P] Crear `money_core/src/schema.rs` con `pub const EXPECTED_SCHEMA_VERSION: i64 = 2;` y una prueba que lea
      `concat!(env!("CARGO_MANIFEST_DIR"), "/../supabase/sql")` y verifique tres cosas: nombres con el patrón
      `NNNN_[a-z0-9_]+.sql`, números consecutivos desde `0001`, y el mayor igual a la constante (r5). Registrar en
      `money_core/src/lib.rs`.
- [X] T015 [P] Agregar a `money_core/src/models/` los tipos `LedgerStatus`, `LedgerSnapshot` y `BackupInfo` de
      [data-model.md](data-model.md), con ts-rs detrás de la feature: `#[ts(type = "number")]` en cada `i64`,
      `export_to = "../../gui/src/bindings/"`, y `path` de `BackupInfo` como `string`. Exportarlos en
      `models/mod.rs`.
- [X] T016 En `money_core/src/error.rs`, agregar `NotConfigured(String)` y `SchemaMismatch { found: i64, expected: i64
      }`, con los mensajes de la tabla "Errores de arranque" de [cli-gui-contract.md](contracts/cli-gui-contract.md). En
      `gui/src-tauri/src/error.rs`, mapearlos a los kinds `not_configured` y `schema_mismatch`.
- [X] T017 En `money_core/src/storage/mod.rs`, agregar al trait `status() -> Result<LedgerStatus>`, `export_snapshot()
      -> Result<LedgerSnapshot>` y `session_email() -> Option<String>`. Por ahora tienen implementación por defecto que
      devuelve `AppError::Invalid("unsupported")`, para que `SqliteBackend` compile; US1 las vuelve obligatorias.
- [X] T018 Implementar esos tres métodos en `MemoryBackend` (`money_core/src/storage/memory.rs`), según
      [storage-contract.md](contracts/storage-contract.md), y en `SupabaseBackend` (`money_core/src/storage/remote.rs`):
      `status` → `POST /rest/v1/rpc/ledger_status`; `export_snapshot` → `POST /rest/v1/rpc/export_ledger`, mapeado a
      `LedgerSnapshot`; `session_email` → `None` por ahora (T021 lo completa).
- [ ] T019 **(manual)** En el SQL Editor del proyecto de Supabase **de prueba**, aplicar `supabase/sql/0001_setup.sql` y
      luego `0002_schema_version.sql`; confirmar que `select * from public.schema_version` devuelve `2`
      (quickstart §0 y §2, pasos 1–2).

**Checkpoint**: `cargo test --workspace` pasa, con las pruebas sobre `MemoryBackend` y SQLite todavía presente. El
proyecto de prueba ya está en la versión de esquema 2.

---

## Phase 3: User Story 1 — Supabase como única base de datos (P1) 🎯 MVP

**Goal**: Sin SQLite, sin modo local ni espejo; CLI y GUI conectan solo a Supabase, y sin configuración explican qué
hacer.

**Independent Test**: quickstart §3 (sin la parte de esquema desfasado) y §7 (primeros tres puntos): operaciones desde
CLI y GUI con las mismas cifras, ningún `.db` creado ni modificado, y un mensaje claro sin configuración.

### Tests for User Story 1

- [X] T020 [P] [US1] Primero: en `money_core/src/storage/remote.rs`, extraer la construcción de la consulta de
      movimientos a una función pura, y probar que un período genera `and=(date.gte.<lo>,date.lt.<hi>)` sin chocar con
      el `or=` del filtro por cuenta. Probar también que el ayudante de paginación sigue pidiendo páginas mientras
      lleguen 1000 filas y para con una página incompleta (r7).

### Implementation for User Story 1

- [X] T021 [US1] En `money_core/src/auth.rs` y `money_core/src/settings.rs`:
      - `TokenResponse` (`auth.rs:17-21`) agrega `user: { id, email }`; se guarda en la sesión en memoria y se expone
        con `session_user()`; `SupabaseBackend::session_email` lo usa (r9);
      - la entrada del llavero pasa a `supabase_refresh_token:<project-ref>`, donde `<project-ref>` es el subdominio de
        `supabase_url` (r15);
      - `Settings` agrega `token_storage: TokenStorage { Keychain, File }` (serde; si falta o es desconocido,
        `Keychain`), y `save_settings` lo conserva;
      - con `File`, `load`, `persist` y `delete` usan solo `~/.money-tracker/refresh_token` (`0600`) y **nunca llaman a
        `keyring()`**;
      - `logout` borra el archivo o la entrada del llavero, según el modo (r17, FR-024).
- [X] T022 [US1] En `money_core/src/storage/remote.rs`: corregir el filtro de período (`:185`) según T020, y paginar de
      1000 en 1000 con orden estable `entries` (`entries_view`), `raw_accounts`, `list_concepts`, `list_budgets` y
      `list_config`. T020 pasa.
- [X] T023 [US1] En `money_core/src/storage/mod.rs`, crear `pub fn connect(settings: &Settings) -> Result<Box<dyn
      LedgerBackend>>` con los pasos 1 y 2 de "Conexión" de [storage-contract.md](contracts/storage-contract.md). El
      paso 3 (versión) se agrega en T045. Prueba: `Settings` sin URL o sin key → `AppError::NotConfigured`, sin red.
- [X] T024 [US1] Portar, antes de borrar, las pruebas de reglas del modelo de `money_core/src/db.rs`:
      `target_account_allows_open_ended_bucket` y `spending_account_rejects_target_amount` → pruebas de los
      constructores de `NewAccount` en `money_core/src/models/account.rs`; `only_one_active_emergency_account` ya está
      cubierta en T004.
- [X] T025 [US1] Borrar SQLite (r13):
      - `money_core/src/db.rs`, `money_core/src/sync/` y `money_core/src/storage/sqlite.rs`;
      - `pub mod db`, `pub mod sync` y `pub use db::open_db` de `lib.rs`; `settings::mirror_path`;
      - los errores `AppError::{Database, LegacySchema, SchemaTooNew, SchemaTooOld}` (y sus kinds en
        `gui/src-tauri/src/error.rs`);
      - del trait: `remote_revision`, `pull_changes_since`, `apply_remote_snapshot`, `sync_cursor`,
        `reset_mirror_cursor`, `poll_sync`, `take_sync_warning` y el tipo `LedgerDelta`; en `remote.rs`,
        `pull_changes_since` (`:591-643`);
      - `rusqlite` de `money_core/Cargo.toml`, `cli/Cargo.toml` y `gui/src-tauri/Cargo.toml`;
      - hacer obligatorios `status`, `export_snapshot` y `session_email`.

      Listar en el mensaje de commit las pruebas eliminadas, con la razón de la tabla r4.
- [X] T026 [US1] En `cli/src/commands/helpers.rs:13-15`, `backend()` llama a
      `money_core::storage::connect(&Settings::load())`. Los errores `NotConfigured` e `InvalidGrant` se imprimen con
      los mensajes de [cli-gui-contract.md](contracts/cli-gui-contract.md).
- [X] T027 [US1] En `cli/src/commands/db.rs`:
      - eliminar `Status` (`:98-121`), `Reset` (`:123+`), `Remote Migrate` (`:263-310`) y `Remote Sync` (`:312-327`),
        con sus imports (`:4-7`);
      - reescribir `Remote Status` (`:212-261`) con el formato de [cli-gui-contract.md](contracts/cli-gui-contract.md):
        Conexión, Sesión (email · guardada en archivo o llavero) y Revisión, con `—` en Esquema y Último respaldo hasta
        US2 y US3.
- [X] T028 [US1] En `gui/src-tauri/src/state.rs`:
      - `backend: Mutex<Option<Box<dyn LedgerBackend>>>`, y un ayudante `with_backend(|be| …) -> ApiResult<T>` que lo
        construye con `connect` al primer uso;
      - quitar el `expect` de `gui/src-tauri/src/lib.rs:9`;
      - cambiar todos los comandos de `gui/src-tauri/src/commands/*.rs` para que usen `with_backend`.
- [X] T029 [US1] En `gui/src-tauri/src/commands/sync.rs`:
      - eliminar `sync_status` y `sync_poll`;
      - agregar `ledger_status` y `connection_info` (`{ url?, email?, configured, logged_in, token_storage }`);
      - `remote_login` acepta `{ url?, key?, email, password }`: guarda url/key si vienen, inicia sesión y pone el
        backend en `None` para que se reconstruya;
      - `remote_logout` también descarta el backend;
      - registrar los cambios en `gui/src-tauri/src/lib.rs`.
- [X] T030 [P] [US1] En `gui/src/api/sync.ts`, reemplazar `SyncStatus`/`SyncPollResult`/`poll` por `ledgerStatus()`,
      `connectionInfo()`, `login({ url?, key?, email, password })` y `logout()`, con sus tipos.
- [X] T031 [US1] En `gui/src/hooks/useSync.ts`, sondear `ledgerStatus()` cada 30 s y llamar a `bumpRevision()` solo
      cuando cambie `revision` (r12).
- [X] T032 [US1] Crear `gui/src/routes/Connect.tsx` (+ `Connect.module.css`) con URL, key publicable, email y
      contraseña que llaman a `login`. En `gui/src/App.tsx`, al montar, llamar a `ledgerStatus()`: si falla con
      `not_configured` o `auth_needed`, mostrar `Connect` en lugar de todo lo demás. Orden de gates: conexión →
      SetupWizard → pestañas.
- [X] T033 [US1] En `gui/src/routes/Settings.tsx`, en el SyncCard (`:86-205`): quitar "Espejo local", "Sincronizar
      ahora" y el texto de "Modo local" (`:142-153`, `:161-177`); mostrar conexión, email, dónde está guardada la
      sesión y revisión.
- [X] T034 [US1] Regenerar los bindings con `cargo test -p money_core --features ts-rs` y correr `npx tsc --noEmit` en
      `gui/`.
- [ ] T035 [US1] **(manual)** Validar contra el proyecto de prueba: quickstart §1, §3 (sin el punto de esquema
      desfasado) y §7 (primeros tres puntos). Incluye SC-001 (`ls ~/.money-tracker/*.db` no cambia) y SC-010 con
      `token_storage = "file"`.

**Checkpoint**: la app funciona solo con Supabase y no existe ningún código de SQLite.

---

## Phase 4: User Story 2 — Respaldo y restauración (P2)

**Goal**: `db backup`, botón en la GUI y respaldo automático perezoso de 7 días; el respaldo se restaura en el SQL Editor.

**Independent Test**: quickstart §4, §5 y §6 (simulacro de restauración), más el punto de respaldo de §7.

### Tests for User Story 2

- [ ] T036 [P] [US2] Primero: en `money_core/src/services/backup_service.rs`, `#[cfg(test)]`, pruebas de
      `render_sql(snapshot, email)` con un `LedgerSnapshot` fijo, comparando contra un texto esperado completo. Cubren
      todo lo de "Estructura del archivo" y "Reglas de generación" de
      [backup-contract.md](contracts/backup-contract.md):
      - encabezado con fecha, revisión, versión y email; guardas de versión y de proyecto vacío;
      - `_restore_user` con el email y la marca `RESTAURAR COMO`;
      - `insert` en orden concepts → accounts → entries → budgets → config; filas por `id` ascendente;
      - `O'Brien` → `'O''Brien'`; `null`; `true`/`false`; `f64` exacto;
      - una tabla vacía no genera `insert`; líneas `setval` por tabla; `begin;`/`commit;`.
- [ ] T037 [P] [US2] Primero: pruebas de `is_due(now, record)` (sin registro → `true`; exactamente 7 días → `false`; 7
      días + 1 s → `true`; registro ilegible → se trata como ausente) y del registro `last-backup.toml` (ida y vuelta
      con `at` RFC 3339, `path`, `revision` y `schema_version`) en `money_core/src/services/backup_service.rs`.
- [ ] T038 [P] [US2] Primero: pruebas de escritura en `backup_service.rs`, usando `std::env::temp_dir()` con un
      subdirectorio único por prueba:
      - un nombre existente recibe el sufijo `-2`; nunca se sobrescribe;
      - permisos `0600`;
      - `-o` a un archivo existente → error; `-o` a una carpeta → nombre por defecto adentro;
      - si la escritura falla, no queda archivo;
      - `create` con `MemoryBackend::seeded()` actualiza el registro.

### Implementation for User Story 2

- [ ] T039 [US2] En `money_core/src/settings.rs`, agregar `backups_dir()` (`config_dir()/backups`) y
      `last_backup_path()` (`config_dir()/last-backup.toml`).
- [ ] T040 [US2] Implementar `money_core/src/services/backup_service.rs` (r9, r10,
      [backup-contract.md](contracts/backup-contract.md)) y registrarlo en `services/mod.rs`. T036–T038 pasan:
      - `render_sql(&LedgerSnapshot, email: &str) -> String`;
      - `create(be, dest: Option<&Path>) -> Result<BackupInfo>`: `export_snapshot` → `render_sql` con
        `be.session_email()` → escritura `create_new` + `0600` (con sufijo) → registro;
      - `read_record()` / `write_record()` de `LastBackupRecord`;
      - `is_due(now: DateTime<Local>, record: Option<&LastBackupRecord>) -> bool`;
      - `run_auto_if_due(now, connect: impl FnOnce() -> Result<Box<dyn LedgerBackend>>) ->
        Option<Result<BackupInfo>>`, que solo llama a `connect` si `is_due`.
- [ ] T041 [US2] En `cli/src/commands/db.rs`, agregar `Backup { #[arg(short = 'o', long)] output: Option<PathBuf> }` →
      `backup_service::create`, que imprime `Respaldo: <ruta> (N movimientos)`. En `Remote Status`, la línea "Último
      respaldo" sale de `read_record()` (o `—`).
- [ ] T042 [US2] En `cli/src/main.rs:40-61`, después del `match`, solo si `result.is_ok()` y el comando no es `Db(Backup)`,
      `Db(Remote(Login))` ni `Db(Remote(Logout))`: llamar a `backup_service::run_auto_if_due(Local::now(),
      commands::helpers::backend)`. Imprimir en stderr los mensajes de éxito o falla de
      [backup-contract.md](contracts/backup-contract.md); omitir en silencio si el error es `NotConfigured`; **nunca**
      cambiar el código de salida.
- [ ] T043 [US2] Crear `gui/src-tauri/src/commands/backup.rs` con `backup_create { dest?: String } -> BackupInfo` y
      `backup_auto() -> Option<BackupInfo>`, usando `with_backend`. Agregar `last_backup` a `connection_info`
      (`commands/sync.rs`). Registrar en `gui/src-tauri/src/lib.rs` y declarar en `commands/mod.rs`.
- [ ] T044 [US2] En el frontend: crear `gui/src/api/backup.ts`; en `gui/src/routes/Settings.tsx`, tarjeta **Respaldo**
      (botón "Respaldar ahora", fecha y ruta del último respaldo, campo de ruta opcional); en `gui/src/App.tsx`, después
      de pasar el gate de conexión, llamar a `backup_auto()` sin bloquear el render y mostrar un aviso breve de éxito o
      advertencia. Regenerar los bindings (`BackupInfo`) y correr `npx tsc --noEmit`.
- [ ] T045 [US2] **(manual)** Validar quickstart §4, §5 y §6 (simulacro de restauración en el proyecto de prueba, con
      `diff` de reportes por período) y el punto de respaldo de §7. Registrar el tiempo del simulacro (SC-005: menos
      de 15 min).

**Checkpoint**: respaldos manuales y automáticos funcionando, y un respaldo restaurado con cifras idénticas.

---

## Phase 5: User Story 3 — Cambios de esquema seguros y versionados (P3)

**Goal**: la app revisa la versión del esquema; hay verificación y guía del esquema. El SQL `0002` ya existe (T013).

**Independent Test**: quickstart §2 (pasos 3–4 y `curl` sin sesión) y el punto de esquema desfasado de §3.

### Tests for User Story 3

- [ ] T046 [P] [US3] Primero: en `money_core/src/schema.rs`, pruebas de `check_schema(found, expected) -> Result<()>`:
      igual → `Ok`; `found < expected` → `SchemaMismatch`, con un mensaje que nombra el archivo
      `supabase/sql/000{expected}_…` a aplicar; `found > expected` → `SchemaMismatch`, con un mensaje de "actualiza la
      app".

### Implementation for User Story 3

- [ ] T047 [US3] Implementar `check_schema` en `money_core/src/schema.rs` y usarlo como paso 3 de
      `storage::connect` (`money_core/src/storage/mod.rs`), con `status()?.schema_version`. T046 pasa.
- [ ] T048 [US3] CLI: en `cli/src/commands/db.rs`, la línea "Esquema" de `Remote Status` muestra `versión N (la app
      espera M)`; `SchemaMismatch` se imprime con los mensajes de "Esquema atrasado" y "App atrasada" de
      [cli-gui-contract.md](contracts/cli-gui-contract.md).
- [ ] T049 [US3] GUI: en `gui/src/App.tsx` y `gui/src/routes/Connect.tsx`, el kind `schema_mismatch` muestra el mensaje
      de versión en lugar del formulario. En Ajustes se muestra la versión del esquema.
- [ ] T050 [P] [US3] Crear `supabase/tests/verify.sql` según "`supabase/tests/verify.sql`" de
      [schema-contract.md](contracts/schema-contract.md): todo dentro de `begin; … rollback;`, los 8 casos mínimos, cada
      uno en un bloque que espera la excepción (si no la hay, `raise exception 'FALLÓ: …'`), y al final `raise notice
      'verify.sql: N/N rechazos confirmados'`.
- [ ] T051 [P] [US3] Crear `supabase/README.md` (FR-020) con:
      - qué hace cada tabla, vista y función (a partir de `0001` y `0002`);
      - cómo aplicar un archivo en el SQL Editor y cómo ver la versión (`select * from public.schema_version`);
      - la plantilla para un archivo nuevo (la estructura obligatoria de
        [schema-contract.md](contracts/schema-contract.md)), incluida la regla "no editar un archivo aplicado" y la de
        subir `EXPECTED_SCHEMA_VERSION`;
      - cómo correr `verify.sql` y las dos llamadas `curl` sin sesión;
      - el procedimiento de restauración, con enlace al formato del respaldo.

      Tablas de 145 caracteres o menos.
- [ ] T052 [US3] **(manual)** En el proyecto de prueba: volver a correr `0002` (debe rechazarse sin cambios), correr
      `verify.sql` (N/N), hacer las dos llamadas `curl` sin sesión (quickstart §2), y simular el esquema desfasado
      (quickstart §3, último punto; después regresar la versión a `2`).

**Checkpoint**: aplicar archivos fuera de orden es imposible, y la app avisa de cualquier desfase.

---

## Phase 6: User Story 4 — Reglas contables probadas sin internet (P4)

**Goal**: cerrar FR-018 y FR-019: ninguna regla duplicada en los handlers, y la suite completa sin red en menos de
10 s. El núcleo (T004–T012) ya se hizo en *Foundational*.

**Independent Test**: quickstart §1, con el Wi-Fi apagado.

- [ ] T053 [US4] En `cli/src/commands/income.rs:107-127`, reemplazar la lectura de `emergency_pct` y la decisión de si
      aplica el reparto por `entry_service::emergency_split_preview`; el texto de la pregunta usa `pct` y `amount` del
      resultado.
- [ ] T054 [US4] En `gui/src-tauri/src/commands/entries.rs`, agregar `income_split_preview { to_account_id, amount } ->
      Option<SplitPreview>` y registrarlo en `lib.rs`. En `gui/src/api/entries.ts`, su wrapper. En
      `gui/src/routes/Register.tsx:309-313`, el aviso de reparto usa el resultado en lugar de deducirlo de `liquid`.
- [ ] T055 [US4] Verificar una sola implementación: `grep -rn "Exceeds credit limit\|Insufficient balance\|Limit must be
      positive" --include=*.rs money_core cli gui/src-tauri` solo debe encontrar `money_core/src/rules.rs`. Con la red
      apagada, `time cargo test --workspace` pasa en menos de 10 s (SC-008).

**Checkpoint**: cada regla contable existe una sola vez y se prueba sin red.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T056 [P] Reescribir `README.md` (FR-022):
      - Supabase pasa a ser **requisito** y va en el camino principal de instalación (crear proyecto, aplicar
        `supabase/sql/*` en el SQL Editor, `db remote login`, `token_storage`); se quita el apéndice "opcional";
      - eliminar modo local, espejo, `MONEY_TRACKER_DB`, `db status`/`db reset`, `db remote sync`/`migrate` y `supabase
        db push`;
      - documentar `db backup`, el respaldo automático, la restauración (enlace a `supabase/README.md`) y la pantalla
        Conexión de la GUI;
      - actualizar la sección 1.6 con el aviso de binario viejo contra esquema nuevo;
      - agregar `baseline_monthly_expense` a la tabla de claves de configuración.

      Conservar el orden intro → instalación → core → CLI → GUI → ejemplos, con tablas y líneas de 145 caracteres o
      menos.
- [ ] T057 [P] Actualizar `AGENTS.md`:
      - estructura (sin `rusqlite`; con `rules.rs`, `memory.rs`, `schema.rs` y los servicios nuevos);
      - build y pruebas (conteo nuevo; pruebas con `MemoryBackend`, sin `MONEY_TRACKER_DB`);
      - layout; comandos del CLI;
      - reemplazar "Schema versioning" por el flujo de `supabase/sql/` + `schema_version` +
        `EXPECTED_SCHEMA_VERSION`;
      - "Data model", con el esquema de Supabase.
- [ ] T058 [P] Borrar `scripts/migrar_a_supabase.sh`. En `scripts/setup_inicial.sh` y `scripts/presupuesto.sh`, cambiar
      los comentarios que recomiendan `MONEY_TRACKER_DB` por `MONEY_TRACKER_CONFIG`, apuntando a un proyecto de prueba.
- [ ] T059 Enmendar `.specify/memory/constitution.md` de 1.0.1 a 2.0.0 según r14:
      - principio II sin "modo WAL"; cláusula legacy del principio V declarada **deprecada**, no borrada;
      - portones de calidad: pruebas con `MemoryBackend`, verificación en un proyecto de prueba, "no editar un archivo
        de esquema aplicado" y "subir `EXPECTED_SCHEMA_VERSION` + `verify.sql` antes de producción";
      - registro de la enmienda con la versión anterior, la nueva y las secciones cambiadas;
      - actualizar la fecha de "Última enmienda".
- [ ] T060 Portones finales:
      - `cargo build --workspace`, `cargo test --workspace` y `cargo clippy --workspace --all-targets` sin warnings;
      - `npx tsc --noEmit` en `gui/`;
      - `cargo tree -p money_core` sin `clap`, `dialoguer`, `tabled`, `tauri` ni `rusqlite`.

      Comparar el conteo de pruebas contra la línea base de T001, explicando las diferencias con la tabla r4.
- [ ] T061 **(manual)** Puesta en producción, quickstart §8 en orden:
      1. copia previa en JSON;
      2. aplicar `0002` a producción;
      3. instalar el binario (README 1.6) e iniciar sesión una vez (con `token_storage = "file"` si se desea);
      4. `db remote status` → esquema 2/2;
      5. `db backup`;
      6. decidir aparte sobre los movimientos #105–#107;
      7. opcional: borrar `~/.money-tracker/data.db` y los `money-tracker-backup-*.db*` del home.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001–T003)**: sin dependencias.
- **Foundational (T004–T019)**: depende de Setup y **bloquea todas las historias**. Orden interno:
  - T004 → T005 → (T006, T008, T009) → T007 → T010 → (T011, T012);
  - T013 y T014 en paralelo con lo anterior;
  - T015 → T016 → T017 → T018; T019 depende de T013.
- **US1 (T020–T035)**: depende de Foundational.
- **US2 (T036–T045)**: depende de Foundational, y de US1 en T023 (`connect`), T028 (`with_backend`) y T029
  (`connection_info`).
- **US3 (T046–T052)**: depende de Foundational y de T023 (`connect`). T050 y T051 solo dependen de T013.
- **US4 (T053–T055)**: depende de Foundational (T007 `emergency_split_preview`) y de T028 (GUI).
- **Polish (T056–T061)**: depende de todas las historias. T061 es lo último, y solo con T035, T045 y T052 aprobadas.

### User Story Dependencies

- **US1**: independiente, una vez terminada Foundational. Es el MVP técnico.
- **US2**: necesita la conexión de US1, porque sin SQLite el respaldo solo puede leer de Supabase. Por eso **no debe ir
  a producción sin US2** (spec, "Why this priority").
- **US3**: el SQL ya está en Foundational; su parte de app solo necesita `connect` (T023).
- **US4**: su núcleo está en Foundational; lo restante toca handlers de US1.

### Within Each User Story

- Las pruebas marcadas "Primero" se escriben antes y deben fallar.
- En el core: modelos → servicios → almacén. Después: handlers (CLI, luego backend Tauri, luego frontend).
- Cada fase termina con su validación manual contra el proyecto de prueba.

## Parallel Opportunities

- **Foundational**: T013 (SQL) y T014 (`schema.rs`) en paralelo con T004–T012 (Rust); T008 y T009 en paralelo entre sí;
  T015 en paralelo con T006.
- **US1**: T030 (frontend) en paralelo con T021–T027 (core y CLI) una vez listo T029; T020 primero.
- **US2**: T036, T037 y T038 en paralelo (distintos módulos de pruebas del mismo archivo; si se prefiere, un solo
  commit).
- **US3**: T050 (`verify.sql`) y T051 (`supabase/README.md`) en paralelo con T046–T049.
- **Polish**: T056, T057 y T058 en paralelo.

### Parallel Example: Foundational

```text
Agente A: T004 → T005 → T006 → T007 → T010   (money_core: reglas + MemoryBackend + servicios)
Agente B: T013                               (supabase/sql/0002_schema_version.sql)
Agente C: T014, T015                         (schema.rs, models nuevos)
```

### Parallel Example: User Story 3

```text
Agente A: T046 → T047 → T048 → T049   (revisión de versión en app)
Agente B: T050                         (supabase/tests/verify.sql)
Agente C: T051                         (supabase/README.md)
```

## Implementation Strategy

### MVP

1. Phase 1 + Phase 2. Checkpoint: pruebas en verde sobre `MemoryBackend` y proyecto de prueba en versión 2.
2. Phase 3 (US1). Checkpoint: la app solo habla con Supabase, sin rastro de SQLite.
3. **No publicar todavía**: sin US2 no hay protección contra la pérdida del proyecto.
4. Phase 4 (US2). Checkpoint: simulacro de restauración aprobado. Este es el **mínimo publicable**.

### Incremental Delivery

- Después del mínimo publicable: US3 (la revisión de versión evita el problema del binario viejo), luego US4 y luego
  Polish.
- La puesta en producción (T061) va al final de todo, con los portones de T060 en verde y la constitución enmendada
  (T059).

### Commits

Un commit por tarea o grupo coherente, con el formato del repo (`[002] …`). Los commits que borran pruebas (T025)
listan cada una con su razón (constitución, principio IV).

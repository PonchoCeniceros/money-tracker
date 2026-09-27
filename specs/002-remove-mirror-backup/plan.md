# Implementation Plan: Supabase como única base de datos, con respaldos y esquema versionado

**Branch**: `002-remove-mirror-backup` | **Date**: 2026-09-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-remove-mirror-backup/spec.md`

## Summary

Supabase pasa a ser la única base de datos: se eliminan SQLite, el modo local, el espejo y los
comandos que solo existían por ellos. Para no perder la protección que daba el espejo, se agrega un
respaldo en forma de **archivo SQL**. Se puede hacer a mano (`db backup`) y se hace solo si el último
tiene más de 7 días (al terminar un comando del CLI o al abrir la GUI). Se restaura pegándolo en el SQL
Editor de un proyecto nuevo, después de los archivos de esquema.

**Enfoque técnico** (detalle en [research.md](research.md)):

- Las reglas contables salen de los backends y van a un módulo de dominio puro (`rules.rs`). Ahí se
  prueban con un almacén en memoria, sin red, sin Docker y sin SQL (R1, R3).
- El esquema de Supabase se versiona con archivos numerados que se validan a sí mismos
  (`schema_version`), y la app revisa la versión al conectar (R5).
- El primer archivo nuevo, `0002`, corrige los defectos encontrados en el esquema en vivo (R6):
  - la fórmula del límite de crédito, que hoy deja pasar cargos por encima del disponible;
  - `sync_state`, que hoy se puede leer y escribir sin sesión;
  - las funciones, que hoy se pueden ejecutar sin sesión;
  - la falta de bloqueo de la cuenta en el RPC de escritura.
- También se corrigen dos defectos del cliente (R7): las listas no paginan, así que se truncarían
  pasando de 1000 filas; y el filtro por período usa un OR y no filtra.
- La foto del respaldo sale de un solo RPC, `export_ledger`, para que sea consistente (R8).

## Technical Context

**Language/Version**: Rust 2021 (workspace: `money_core` lib, `cli` bin, `gui` Tauri v2) + TypeScript/React 19 en
`gui/src`. SQL de Postgres 15+ (Supabase) para los archivos de esquema.

**Primary Dependencies**: Ya presentes: `reqwest` (blocking, rustls), `serde`/`serde_json`, `toml`, `chrono`, `keyring`,
`thiserror`, `ts-rs` (feature opcional). Se **quita** `rusqlite` de `money_core`, `cli` y `gui/src-tauri`. No se agregan
dependencias.

**Storage**: Supabase (Postgres + PostgREST + Auth) como único almacén. En disco local quedan solo `config.toml`, el
archivo del refresh token (si no hay llavero), `last-backup.toml` y la carpeta `backups/`, todo en `~/.money-tracker/`.

**Testing**: `cargo test --workspace` con `MemoryBackend` (feature `test-support`), sin red. Suite actual: 54 pruebas;
10 se eliminan o reescriben según research R4. Se agregan pruebas para `rules.rs`, `backup_service` (SQL generado,
escape, `is_due`, no sobrescribir) y la de `EXPECTED_SCHEMA_VERSION` contra `supabase/sql/`. Frontend: `npx tsc
--noEmit`. SQL: `supabase/tests/verify.sql` a mano en un proyecto de prueba.

**Target Platform**: macOS y Linux (CLI) y ventana nativa Tauri (GUI), contra Supabase alojado.

**Project Type**: Librería + CLI + app de escritorio (workspace Rust) + archivos SQL de esquema.

**Performance Goals**:
- Respaldo de unos 1,100 movimientos en menos de 30 s (SC-004): un solo RPC, más la generación de texto en memoria.
- La suite de pruebas completa en menos de 10 s (SC-008).
- La revisión del respaldo automático no hace llamadas de red cuando no toca respaldar.

**Constraints**:
- Ningún archivo de base de datos local (SC-001).
- El respaldo nunca sobrescribe y se crea con permisos 0600.
- Un archivo de esquema aplicado no se edita.
- La firma de `apply_entries` no cambia, para no romper un binario viejo durante la transición (R16).
- Los mensajes de error de las reglas conservan su texto actual.

**Scale/Scope**: Un usuario, dos o más instalaciones, del orden de miles de movimientos por año.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluado contra la constitución **1.0.1**.

| Principio / portón | Estado | Nota |
|---|---|---|
| I. Modelo primero | ✅ Mejora | Las reglas salen de los backends y van a `rules.rs` |
| II. Handlers delgados | ✅ Mejora, ⚠️ texto | Sin reglas en `income.rs`; budget y concept por servicios; "modo WAL" obsoleto |
| III. El libro es la fuente de verdad | ✅ | Saldos derivados; Supabase conserva CHECKs e índice único |
| IV. Test-first por la API pública | ✅ | 10 pruebas se eliminan o reescriben con su razón (R4) |
| V. Simplicidad | ✅, ⚠️ texto | Sin restauración ni dependencias nuevas; "esquema legacy" sin objeto |
| Portón: `MONEY_TRACKER_DB` en pruebas | ❌ → enmienda | Lo sustituyen `MemoryBackend` y un proyecto de prueba |
| Portón: build, test, clippy, tsc | ✅ | Sin cambios |
| Portón: bindings ts-rs | ✅ | Se regeneran por `LedgerStatus` y `BackupInfo` |

Detalle de las notas:

- **Principio I**: además de `rules.rs`, los servicios nuevos (`budget_service`, `concept_service`,
  `backup_service`) viven en `money_core`.
- **Principio II**: el texto "compartir el mismo libro mayor (modo WAL)" describe SQLite y queda obsoleto (R14).
- **Principio IV**: ninguna aserción se debilita; solo cambia cómo se crea el almacén.
- **Principio V**: la cláusula "rechazar bases con esquema previo al rediseño" queda sin objeto al no haber base
  local (R14).

**Resultado**: pasa, con una violación justificada que se resuelve dentro de la misma feature: la enmienda **2.0.0**
de la constitución (FR-023, research R14). Ver *Complexity Tracking*.

**Re-evaluación después del diseño (Phase 1)**: sin cambios. Los contratos no agregan lógica en los handlers: los
comandos Tauri nuevos, `db backup` y `db remote status` llaman cada uno a una función de `money_core`.
`backup_service` escribe archivos, igual que ya lo hace `settings.rs`; generar el texto SQL es serialización, no
presentación, así que no viola el principio I. La única lógica nueva en la GUI es el orden de los gates (conexión →
wizard → pestañas), que es presentación.

## Project Structure

### Documentation (this feature)

```text
specs/002-remove-mirror-backup/
├── plan.md              # este archivo
├── research.md          # R1–R16: decisiones técnicas
├── data-model.md        # entidades nuevas, las que cambian y las que se eliminan
├── quickstart.md        # validación de punta a punta y puesta en producción
├── contracts/
│   ├── storage-contract.md   # LedgerBackend después de 002 + connect()
│   ├── schema-contract.md    # archivos SQL, guardas, 0002, RPC, verify.sql
│   ├── backup-contract.md    # formato del respaldo y reglas del automático
│   └── cli-gui-contract.md   # comandos, errores y pantallas
├── checklists/requirements.md
└── tasks.md             # lo genera /speckit.tasks
```

### Source Code (repository root)

```text
money_core/
  src/
    rules.rs                 # NUEVO: reglas contables puras (R1)
    schema.rs                # NUEVO: EXPECTED_SCHEMA_VERSION + prueba contra supabase/sql/ (R5)
    auth.rs                  # CAMBIA: guarda user {id, email}; llavero por proyecto; modo archivo (R9, R15, R17)
    settings.rs              # CAMBIA: sin mirror_path; + token_storage, backups_dir(), last_backup_path()
    error.rs                 # CAMBIA: + NotConfigured, SchemaMismatch; − Database, LegacySchema, SchemaToo*
    lib.rs                   # CAMBIA: sin db/sync; + rules, schema
    models/                  # CAMBIA: + LedgerStatus, LedgerSnapshot, BackupInfo
    services/
      account_service.rs     # CAMBIA: valida una sola emergencia
      entry_service.rs       # CAMBIA: valida sobregiro/crédito antes de push; valida edición; split_preview
      budget_service.rs      # NUEVO (R2)
      concept_service.rs     # NUEVO (R2)
      backup_service.rs      # NUEVO: create, is_due, run_auto_if_due, render SQL (R9, R10)
      report_service.rs      # sin cambios de lógica
      setup_service.rs       # sin cambios de lógica
    storage/
      mod.rs                 # CAMBIA: trait sin métodos de espejo; + status, export_snapshot; connect() (R11)
      remote.rs              # CAMBIA: paginación, filtro de período, RPC nuevos, sin pull_changes (R7, R8)
      ledger.rs              # sin cambios (matemática pura)
      memory.rs              # NUEVO: MemoryBackend, solo con test-support (R3)
      sqlite.rs              # SE ELIMINA
    db.rs                    # SE ELIMINA
    sync/                    # SE ELIMINA
  tests/scenarios.rs         # CAMBIA: fresh_db() → MemoryBackend::seeded()
  Cargo.toml                 # − rusqlite; + feature test-support; self dev-dependency

cli/src/
  main.rs                    # CAMBIA: respaldo automático después de un comando exitoso (R10)
  commands/db.rs             # CAMBIA: − status, reset, remote sync, remote migrate; + backup; status nuevo
  commands/helpers.rs        # CAMBIA: backend() → storage::connect()
  commands/income.rs         # CAMBIA: usa split_preview del dominio
  commands/budget.rs, concept.rs, add.rs  # CAMBIAN: usan budget_service / concept_service
cli/Cargo.toml               # − rusqlite

gui/src-tauri/src/
  state.rs                   # CAMBIA: backend Option, construido al primer uso (R11)
  lib.rs                     # CAMBIA: sin expect al arrancar; comandos nuevos registrados
  error.rs                   # CAMBIA: kinds not_configured, schema_mismatch
  commands/sync.rs           # CAMBIA: ledger_status, connection_info, remote_login con url/key
  commands/backup.rs         # NUEVO: backup_create, backup_auto
  commands/entries.rs        # CAMBIA: + income_split_preview
  commands/budgets.rs, concepts.rs  # CAMBIAN: usan servicios
gui/src-tauri/Cargo.toml     # − rusqlite
gui/src/
  App.tsx                    # CAMBIA: gate de conexión antes del wizard; backup_auto al montar
  routes/Connect.tsx         # NUEVO
  routes/Settings.tsx        # CAMBIA: sin espejo; + tarjeta Respaldo
  routes/Register.tsx        # CAMBIA: aviso de reparto desde income_split_preview
  hooks/useSync.ts           # CAMBIA: sondea ledger_status
  api/sync.ts, api/backup.ts # CAMBIAN / NUEVO
  bindings/                  # se regeneran

supabase/
  sql/0001_setup.sql         # git mv desde migrations/0001_initial.sql, sin cambios
  sql/0002_schema_version.sql  # NUEVO (R6)
  tests/verify.sql           # NUEVO (FR-021)
  README.md                  # NUEVO (FR-020)
scripts/migrar_a_supabase.sh # SE ELIMINA; setup_inicial.sh y presupuesto.sh: solo comentarios
README.md, AGENTS.md         # SE REESCRIBEN las partes de SQLite, espejo y modo local (FR-022)
.specify/memory/constitution.md  # enmienda 2.0.0 (FR-023)
```

**Structure Decision**: Se conserva el workspace de tres crates. El cambio principal ocurre dentro de `money_core`:
entran `rules.rs`, tres servicios nuevos y `memory.rs`, y salen `db.rs`, `sync/` y `sqlite.rs`. Los handlers solo se
adaptan a la API nueva. Los archivos SQL salen de `supabase/migrations/` hacia `supabase/sql/`, para que no los tome
el CLI de Supabase.

### Orden de implementación sugerido (para `/speckit.tasks`)

1. **Base sin romper nada**: `rules.rs` + `MemoryBackend`. Las pruebas pasan a usar `MemoryBackend` con SQLite todavía
   presente, y deben quedar en verde antes de borrar nada.
2. **Reglas al dominio**: servicios que validan antes de escribir; se quitan las reglas de `sqlite.rs`/`remote.rs`.
3. **SQL**: `git mv` de `0001`, `0002`, `verify.sql`, `schema.rs`; validar en el proyecto de prueba (quickstart §2).
4. **Almacén remoto**: `status`, `export_snapshot`, paginación, filtro de período, `connect()`, auth con usuario y
   llavero por proyecto.
5. **Borrar SQLite**: `db.rs`, `sync/`, `sqlite.rs`, comandos locales, `rusqlite`, `scripts/migrar_a_supabase.sh`.
6. **Respaldo**: `backup_service`, `db backup`, hook del CLI, comandos Tauri, GUI.
7. **GUI**: estado perezoso, pantalla Conexión, Ajustes, `useSync`.
8. **Documentación y gobierno**: `supabase/README.md`, README, AGENTS.md, enmienda de la constitución.
9. **Validación y puesta en producción**: quickstart §1–§8.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Portón: pruebas con `MONEY_TRACKER_DB` | Sin SQLite no hay base desechable | SQLite en pruebas probaría una copia de las reglas (Q3) |
| Principio II: "modo WAL" | CLI y GUI comparten el libro en Supabase | El texto describiría algo que ya no existe |
| Principio V: rechazo de bases legacy | Sin base local no hay esquema legacy | Su sucesor es la revisión de `schema_version` (R5) |

Las tres se resuelven con la enmienda 2.0.0 (research R14) dentro de esta misma feature, antes del merge.

# Quickstart — Validación end-to-end (Supabase como fuente)

**Branch**: `001-supabase-backend` | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Guía de validación runnable que demuestra que el feature funciona de punta a punta. Detalles de
implementación → `tasks.md` (Phase 2). Contratos y modelo → [contracts/](contracts/) y
[data-model.md](data-model.md). No es un reemplazo de la suite de tests; es la verificación manual/guía
de humo.

## Prerrequisitos

- Proyecto de Supabase creado (plan free OK) con URL y **key publicable** (`sb_publishable_*`).
- Usuario único creado (signups deshabilitados) con email+password.
- Rust toolchain del workspace + `supabase` CLI (para `db push`).
- Aplicadas las migraciones: `supabase link --project-ref <ref> && supabase db push`.

## Preparación

```sh
export MONEY_TRACKER_SUPABASE_URL="https://<ref>.supabase.co"
export MONEY_TRACKER_SUPABASE_KEY="sb_publishable_..."   # env gana sobre config
export MONEY_TRACKER_DB="/tmp/mt-mirror.db"               # espejo desechable
```

## 1. Humo: login y primer ciclo completo (User Story 1, P1)

```sh
cargo run -p money-tracker -- db remote login --email tu@correo.com --password ***** --yes
cargo run -p money-tracker -- db remote status                    # sesión OK, revision 0
cargo run -p money-tracker -- account add debito --kind spending
cargo run -p money-tracker -- account add "Fondo de emergencia" --kind emergency
cargo run -p money-tracker -- config set emergency_pct 10
cargo run -p money-tracker -- setup --account "Fondo de emergencia"=35000 --account debito=18000 -D 2026-08-01
cargo run -p money-tracker -- income 20000 Nomina
cargo run -p money-tracker -- add 350 Alimentos
cargo run -p money-tracker -- report -p 2026-08 --detail
```

**Esperado**: ingreso reparte 10 % al fondo de emergencia automáticamente (`2000`); `report` muestra
devengado y salida real coherentes; `db remote status` muestra `revision` avanzada y **paridad
espejo/remoto OK** (el estándar: `sqlite3 /tmp/mt-mirror.db "select count(*) from entries"` ==
remoto).

## 2. Sin sesión / sin red (FR-008)

Quitar la sesión (`db remote logout`) o cortar la red; intentar `add 10 Prueba`.

**Esperado**: error claro; `sqlite3 /tmp/mt-mirror.db "select count(*) from entries"` no cambió
(el espejo no se muta sin éxito remoto).

## 3. Migración (User Story 3 / FR-007)

```sh
# base local legada con data real (N temor usa MONEY_TRACKER_DB para una base android)
MONEY_TRACKER_DB="/tmp/mt-current.db" cargo run -p money-tracker -- <operaciones previas>
MONEY_TRACKER_DB="/tmp/mt-mirror.db" cargo run -p money-tracker -- db remote migrate \
    --source /tmp/mt-current.db
MONEY_TRACKER_DB="/tmp/mt-mirror.db" cargo run -p money-tracker -- report -p <periodo> --detail
```

**Esperado**: la data se traslada 100 % (reporte por período idéntico antes/después, SC-003). Sobre
una base legacy (con `transactions`/`buckets`) falla con error accionable (FR-007).

## 4. Propagación entre instalaciones (User Story 2 / SC-002)

- Instalación A: `cd gui && pnpm tauri dev` (mismo URL/key; otro espejo local en otra ruta).
- Instalación B (CLI): registrar `transfer -a 500 --from debito --to "Fondo de emergencia"`.

**Esperado**: en A (GUI) el movimiento aparece en la lista y en el reporte en < 1 min, sin reload
manual (poll ~30 s → `bumpRevision()`).

## 5. Recuperación desde el espejo (SC-005)

Con el espejo al día, interrumpir/simular pérdida del remoto (revocar proyecto o bloqueo de URL):
leer el espejo con `sqlite3` y comparar que `account_balances` reproduce los saldos del último
`db remote status` exitoso. El comando `db remote status` también reporta integridad del espejo
(`PRAGMA integrity_check`).

## 6. Regresión de la suite

```sh
cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets
cd gui && npx tsc --noEmit
```

**Esperado**: los 44 tests de `money_core` pasan sobre `SqliteBackend` (in-memory) y
`scenarios.rs` corre también contra el Supabase de desarrollo si `MONEY_TRACKER_SUPABASE_URL` está
presente (los invariantes contables con ambos backends — Principio IV). Regenerar bindings ts-rs solo
si cambian los modelos exportados (`cargo test -p money_core --features ts-rs`).

## Nota

El proceso de "datos de prueba" usa `MONEY_TRACKER_DB` (espejo) para no tocar una DB real ni un
Supabase con data personal; los tests unitarios usan SQLite in-memory y una instancia de desarrollo
de Supabase cuando esté disponible.
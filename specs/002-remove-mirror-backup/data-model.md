# Data Model: Supabase como única base de datos, con respaldos y esquema versionado

**Branch**: `002-remove-mirror-backup` | **Spec**: [spec.md](spec.md) | **Research**: [research.md](research.md)

Las entidades contables (cuentas, movimientos, presupuestos, conceptos, configuración) **no cambian**:
mismas columnas, mismos CHECKs y misma forma que en `001-supabase-backend/data-model.md`. Aquí solo
se describe lo nuevo, lo que cambia y lo que se elimina.

## En Supabase

### `schema_version` (nueva, la crea `0002`)

| Columna | Tipo | Regla |
|---|---|---|
| `id` | `smallint` | PK, `check (id = 1)`: una sola fila |
| `version` | `integer` | `not null`; `0002` la deja en `2`; cada archivo `NNNN` la fija en `NNNN` |
| `applied_at` | `timestamptz` | `not null default now()`; se actualiza con cada archivo |

- RLS activado; política de `select` para `authenticated`. Nadie la escribe desde la API: solo los
  archivos de esquema, desde el SQL Editor.
- Transición de estados: `(no existe)` → `2` → `3` → …, siempre de uno en uno (FR-014).

### `sync_state` (cambia)

- Misma estructura. La política pasa de `for all using (true)` a `select` para `authenticated`.
- `revision` sube con cada `insert`/`update` (`touch_row`, ya existe) y con cada `delete`
  (`bump_revision_after_delete`, nuevo; reemplaza al trigger de `tombstones`).

### Eliminados por `0002`

- Tabla `tombstones`, función `tombstone_after_delete()` y sus cinco triggers `trg_*_tomb`.
- Función `pull_changes(bigint)`.

### RPC

| Función | Devuelve | Uso |
|---|---|---|
| `apply_entries(jsonb)` | filas creadas | Escribir movimientos; se corrige en `0002` (research R6) |
| `ledger_status()` | `{ revision, schema_version }` | Revisión de versión, sondeo de la GUI, `db remote status` |
| `export_ledger()` | libro completo + metadatos | Respaldo consistente (una sola foto) |

## En `money_core` (Rust)

### `LedgerStatus` (nuevo)

`{ revision: i64, schema_version: i64 }`. Lo devuelve `LedgerBackend::status()`. Se exporta a la GUI
con ts-rs (`#[ts(type = "number")]` en los `i64`).

### `LedgerSnapshot` (nuevo, reemplaza a `LedgerDelta`)

`{ revision, schema_version, exported_at: String, concepts: Vec<Concept>, accounts: Vec<Account>,
entries: Vec<Entry>, budgets: Vec<Budget>, config: Vec<Config> }`. Lo devuelve
`LedgerBackend::export_snapshot()`. `accounts` incluye las archivadas.

### `BackupInfo` (nuevo)

`{ path: String, created_at: String (RFC 3339, hora local), revision: i64, schema_version: i64, entries:
i64, automatic: bool }`. Es lo que ven el CLI y la GUI al terminar un respaldo. Se exporta con ts-rs; la
ruta y la fecha van como texto para no depender del soporte de `chrono` en ts-rs.

### `LastBackupRecord` (nuevo, archivo local)

`~/.money-tracker/last-backup.toml`:

```toml
at = "2026-09-26T21:04:11-06:00"   # RFC 3339, hora local con desfase
path = "/Users/…/.money-tracker/backups/money-tracker-20260926-210411.sql"
revision = 129
schema_version = 2
```

- Solo se escribe después de un respaldo exitoso, sea manual o automático.
- Si falta o no se puede leer, cuenta como "nunca hubo respaldo". Es local a cada máquina.
- `is_due(now, record) = record.is_none() || now - record.at > 7 días`.

### `AppError` (cambia)

- Se agregan `NotConfigured(String)`, con instrucciones de configuración, y `SchemaMismatch { found:
  i64, expected: i64 }`.
- Se eliminan `Database(rusqlite::Error)`, `LegacySchema`, `SchemaTooNew` y `SchemaTooOld`.
- `ApiError` de la GUI mapea las nuevas variantes a los kinds `not_configured` y `schema_mismatch`, y
  el frontend las usa para decidir si muestra la pantalla de conexión.

### `Settings` / `config.toml` (cambia)

```toml
supabase_url = "https://<ref>.supabase.co"
supabase_publishable_key = "sb_publishable_…"
token_storage = "file"        # nuevo: "keychain" (por defecto) | "file"
```

`Settings` agrega `token_storage: TokenStorage` (`Keychain` | `File`), con `Keychain` si falta o si
el valor es desconocido. `save_settings` lo conserva al reescribir el archivo en `db remote login`.

### Sesión de auth (cambia)

`TokenResponse` agrega `user: { id: Uuid-string, email: String }`. `SupabaseAuth` expone
`session_user() -> Option<SessionUser>`, que se llena en `login` y en `refresh`. Con eso se prellena el
email del restaurador en el respaldo y se muestra en `db remote status`.

## Validaciones del dominio (`rules.rs`)

| Regla | Entrada | Error |
|---|---|---|
| Sobregiro | cuenta origen `target`/`emergency`, `monto > saldo` | `Invalid("Insufficient balance…")` |
| Límite de crédito | cuenta origen `credit` con límite, `max(-saldo,0) + monto > límite` | `Invalid("Exceeds credit limit…")` |
| Una emergencia | crear `emergency` con otra activa | `Invalid(...)` |
| Presupuesto positivo | `limit <= 0` | `Invalid("Limit must be positive")` |
| Tipo de concepto | fuera de `expense`/`income`/`both` | `Invalid(...)` |
| Forma al editar | lado de cuenta incorrecto, auto-transferencia, concepto de más o de menos | `Invalid(...)` |
| Reparto de emergencia | destino líquido + fondo activo + no `--no-emergency` | aplica o no (no es error) |

Los textos de error se conservan iguales a los actuales, para que los mensajes del CLI no cambien.

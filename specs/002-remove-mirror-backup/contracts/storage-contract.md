# Contract: `LedgerBackend` después de 002

**Branch**: `002-remove-mirror-backup` | **Spec**: [spec.md](../spec.md) | **Data model**: [data-model.md](../data-model.md)

Contrato interno de `money_core`. Reemplaza a `001-supabase-backend/contracts/storage-contract.md`.
Hay dos implementaciones: `SupabaseBackend` (producción) y `MemoryBackend` (solo en pruebas). El
backend **solo guarda y lee**: no contiene reglas contables (research R1).

## Trait

```rust
pub trait LedgerBackend: Send + Sync {
    // Lecturas crudas (paginadas en Supabase; research R7)
    fn raw_accounts(&self, include_archived: bool) -> Result<Vec<Account>>;
    fn entries(&self, f: &EntryFilter) -> Result<Vec<Entry>>;
    fn get_entry(&self, id: i64) -> Result<Entry>;

    // Escrituras (sin validar reglas contables; eso ya lo hizo el servicio)
    fn insert_account(&self, new: &NewAccount) -> Result<i64>;
    fn set_archived(&self, id: i64) -> Result<()>;
    fn push_entries(&self, entries: &[NewEntry]) -> Result<Vec<Entry>>;   // lote atómico
    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry>;
    fn delete_entry(&self, id: i64) -> Result<()>;

    fn get_config(&self, key: &str) -> Result<Option<String>>;
    fn set_config(&self, key: &str, value: &str) -> Result<()>;
    fn list_config(&self) -> Result<Vec<Config>>;
    fn list_concepts(&self, type_filter: Option<&str>) -> Result<Vec<Concept>>;
    fn add_concept(&self, name: &str, concept_type: &str) -> Result<()>;
    fn set_budget(&self, concept: &str, limit: f64, period: &str) -> Result<()>;
    fn list_budgets(&self, period: Option<&str>) -> Result<Vec<Budget>>;
    fn delete_budget(&self, concept: &str, period: &str) -> Result<()>;

    // Nuevos
    fn status(&self) -> Result<LedgerStatus>;             // RPC ledger_status()
    fn export_snapshot(&self) -> Result<LedgerSnapshot>;  // RPC export_ledger(), una sola foto
    fn session_email(&self) -> Option<String>;            // para prellenar el respaldo

    // Derivadas con implementación por defecto (sin cambios): accounts_with_balances,
    // list_accounts, get_account, find_account_by_name, require_account_by_name,
    // emergency_account, balance_as_of, is_seeded, archive_account
}
```

**Se eliminan**: `remote_revision` (lo reemplaza `status`), `pull_changes_since`,
`apply_remote_snapshot`, `sync_cursor`, `reset_mirror_cursor`, `poll_sync`, `take_sync_warning` y el
tipo `LedgerDelta`.

## Garantías que ambas implementaciones deben cumplir

- `push_entries` es atómico: o se guardan todos los movimientos del lote, o ninguno.
- Orden estable: cuentas por `kind, name`; movimientos por `date desc, id desc`; conceptos por `name`;
  presupuestos por `period, concept`; configuración por `key`.
- `get_entry`, `update_entry` y `delete_entry` sobre un id inexistente devuelven
  `AppError::NotFound`.
- Nombres únicos: una cuenta o un concepto repetido da `AppError::Invalid`.
- Las listas nunca se truncan en silencio, sin importar cuántas filas haya.

`SupabaseBackend` además mantiene las validaciones del esquema como segunda defensa (research R6).
`MemoryBackend` no las replica.

## Conexión

```rust
pub fn connect(settings: &Settings) -> Result<Box<dyn LedgerBackend>>;
```

1. Sin `supabase_url` o sin `supabase_publishable_key` → `AppError::NotConfigured`, con el comando
   exacto para configurar.
2. Construye `SupabaseBackend` y llama a `status()`. Sin sesión → `AppError::InvalidGrant`, que se
   presenta como "inicia sesión".
3. `status.schema_version != schema::EXPECTED_SCHEMA_VERSION` → `AppError::SchemaMismatch { found,
   expected }`.

## `MemoryBackend` (solo pruebas)

- Está disponible con `#[cfg(test)]` o con la feature `test-support`.
- `MemoryBackend::new()` arranca vacío. `MemoryBackend::seeded()` crea los 10 conceptos base y
  `emergency_pct = 10`.
- `status()` devuelve `{ revision: n, schema_version: EXPECTED_SCHEMA_VERSION }`, donde `n` sube con
  cada escritura. `export_snapshot()` devuelve todo el estado. `session_email()` devuelve
  `Some("test@example.com")`.

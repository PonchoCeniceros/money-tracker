# Contract: `LedgerBackend` (storage layer de money_core)

**Branch**: `001-supabase-backend` | **Spec**: [spec.md](../spec.md) | **Data model**: [data-model.md](../data-model.md)

Contrato interno del crate `money_core`. Define el punto único en el que los servicios se apoyan
para leer/escribir el libro mayor. Dos implementaciones: `SqliteBackend` (rusqlite) y
`SupabaseBackend` (PostgREST), más el decorador `MirroringBackend`.

## Traito (formato de firma)

```rust
pub trait LedgerBackend: Send + Sync {
    type Error: Into<AppError>;

    // Cuentas
    fn insert_account(&self, new: &NewAccount) -> Result<i64>;
    fn list_accounts(&self, include_archived: bool) -> Result<Vec<AccountBalance>>;
    fn get_account(&self, id: i64) -> Result<AccountBalance>;
    fn find_account_by_name(&self, name: &str) -> Result<Option<AccountBalance>>;
    fn emergency_account(&self) -> Result<Option<AccountBalance>>;
    fn archive_account(&self, id: i64, force: bool) -> Result<()>;

    // Entradas (único escritor del libro)
    fn insert_entry(&self, e: &NewEntry) -> Result<Entry>;          // devuelve la fila con id (y nombres)
    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry>;
    fn delete_entry(&self, id: i64) -> Result<()>;
    fn get_entry(&self, id: i64) -> Result<Entry>;
    fn list_entries(&self, f: &EntryFilter) -> Result<Vec<Entry>>;   // join con nombres de cuentas

    // Split atómico (ingreso + transferencia de emergencia)
    fn apply_income_with_split(&self, income: &NewEntry, split: Option<(i64, f64)>)
        -> Result<IncomeResult>;

    // Config
    fn get_config(&self, key: &str) -> Result<Option<String>>;
    fn set_config(&self, key: &str, value: &str) -> Result<()>;

    // Snapshots para derivación en memoria (ledger.rs)
    fn accounts_snapshot(&self) -> Result<Vec<AccountBalance>>;
    fn entries_snapshot(&self, f: &EntryFilter) -> Result<Vec<Entry>>;

    // Sync / migración
    fn remote_revision(&self) -> Result<(i64, i64)>;                  // (revision, ts watermark)
    fn pull_changes_since(&self, watermark: i64) -> Result<LedgerDelta>;
    fn apply_remote_snapshot(&self, rows: &LedgerDelta) -> Result<()>; // poblar día 1 / restaurar
}
```

## Garantías

- **Integridad**: toda mutación pasa por el esquema (CHECKs + índice único parcial) del backend; un
  intento inválido devuelve `AppError::Invalid` (35000/23505/23514 mapeadas a mensajes accionables).
- **Atomicidad**: `apply_income_with_split` es una sola transacción remota (RPC `apply_entries`) o
  local (transacción SQLite); nunca dos POSTs sin garantía.
- **Consistencia del espejo**: quienes usan `SupabaseBackend` a través de `MirroringBackend` reciben
  el "éxito" solo después de que el remoto confirmó Y el espejo fue re-sincronizado para esa mutación.
- **Read-modify-write**: overdraft/cupo de crédito dentro de `apply_income_with_split`; reconciliación
  calcula el delta desde `entries_snapshot` y lo escribe como una entrada única (última escritura
  ganadora).

## Criterios de aceptación (verificación)

1. Transactional: un fallo a mitad de `apply_income_with_split` deja el remoto y el espejo idénticos
   (auditable comparando counts/ids después de un fallo inyectado).
2. Poridad de resultados: con el mismo juego de datos, `list_accounts`, `list_entries(period)` y el
   reporte derivado en memoria producen salidas idénticas en `SqliteBackend` y `SupabaseBackend`.
3. Los 44 tests actuales de `money_core` pasan sin modificar su aserción cuando corren sobre
   `SqliteBackend` (in-memory) (Principio IV).
# Contract: CLI y GUI después de 002

**Branch**: `002-remove-mirror-backup` | **Spec**: [spec.md](../spec.md) | **Research**: [research.md](../research.md) R2, R10–R12

## CLI

### Comandos que cambian

| Comando | Antes | Después |
|---|---|---|
| `db status` | Estado del archivo SQLite | **Se elimina** |
| `db reset` | Aparta o borra el archivo SQLite | **Se elimina** |
| `db remote sync` | Sincroniza el espejo | **Se elimina** |
| `db remote migrate` | Sube la base local | **Se elimina** |
| `db remote status` | Modo, sesión, revisión, espejo | Conexión, sesión (email), revisión, versión de esquema, último respaldo |
| `db remote login` | Guarda url/key + sesión | Igual |
| `db remote logout` | Olvida la sesión | Igual |
| `db backup [-o RUTA]` | — | **Nuevo**: respaldo manual (ver [backup-contract](backup-contract.md)) |

Todos los demás comandos (`add`, `income`, `transfer`, `bucket`, `account`, `entry`, `concept`,
`budget`, `report`, `config`, `setup`) conservan su sintaxis y sus mensajes.

### Salida de `db remote status`

```
Conexión:        https://<ref>.supabase.co
Sesión:          activa (giovanny@example.com) · guardada en archivo
Revisión:        131
Esquema:         versión 2 (la app espera 2)
Último respaldo: 2026-09-26 21:04 · ~/.money-tracker/backups/money-tracker-20260926-210411.sql
```

### Dónde se guarda la sesión (research R17)

- `config.toml`: `token_storage = "keychain"` (por defecto) o `"file"`. No hay bandera de CLI; se
  cambia editando el archivo y se aplica en el siguiente `db remote login`.
- Con `"file"`, ningún comando lee, escribe ni borra el llavero.
- La GUI respeta la misma clave y muestra en Ajustes dónde está guardada la sesión.

### Errores de arranque (todos con código de salida 1)

| Caso | Mensaje (resumen) |
|---|---|
| Sin configuración | `Supabase no está configurado. Corre: money-tracker db remote login --url … --key …` |
| Sin sesión o sesión vencida | `No hay sesión activa. Corre: money-tracker db remote login` |
| Esquema atrasado | `El esquema está en la versión 1 y esta app espera la 2. Aplica supabase/sql/0002_… en el SQL Editor.` |
| App atrasada | `El esquema de Supabase está en la versión 3 y esta app espera la 2. Actualiza la app (README, sección 1.6).` |
| Sin conexión | Igual que hoy (001 FR-008) |

### Reglas de handler

- `income` obtiene del dominio si aplica el reparto y de cuánto (`rules::emergency_split` vía
  `entry_service`). Ya no lee `emergency_pct` ni decide por su cuenta (`income.rs:107-127`).
- `budget` y `concept` llaman a `budget_service` y `concept_service`, no al backend.

## GUI

### Comandos Tauri

| Comando | Cambio |
|---|---|
| `sync_status`, `sync_poll` | **Se eliminan** |
| `ledger_status` | **Nuevo** → `LedgerStatus`, o error `not_configured` / `auth_needed` / `schema_mismatch` |
| `remote_login` | Acepta `{ url?, key?, email, password }`; guarda la config y reconstruye el backend |
| `remote_logout` | Igual; además descarta el backend en memoria |
| `connection_info` | **Nuevo** → `{ url?, email?, configured, logged_in, last_backup? }` para Ajustes |
| `backup_create` | **Nuevo** `{ dest? }` → `BackupInfo` |
| `backup_auto` | **Nuevo** → `BackupInfo \| null` (`null` si no tocaba) |
| `income_split_preview` | **Nuevo** `{ to_account_id, amount }` → `{ pct, amount } \| null` |

Todos los demás comandos obtienen el backend con un helper que lo construye al primer uso. Si no se
puede construir, devuelven el error correspondiente en lugar de entrar en pánico.

### Pantallas

- **Conexión** (`routes/Connect.tsx`, nueva): URL, key publicable, email y contraseña. Se muestra en
  lugar de todo lo demás si `ledger_status` falla con `not_configured`, `auth_needed` o
  `schema_mismatch`. En el último caso muestra el mensaje de versión y no el formulario.
- **Orden de los gates en `App.tsx`**: conexión → SetupWizard (0 cuentas) → pestañas.
- **Ajustes**:
  - Sale del SyncCard todo lo del espejo: "Espejo local", "Sincronizar ahora" y el texto de "Modo
    local".
  - Se muestran la conexión, la sesión, la revisión y la versión de esquema.
  - Tarjeta nueva **Respaldo**: botón "Respaldar ahora", fecha y ruta del último respaldo, campo
    opcional de ruta.
- **Registrar → Ingreso**: el aviso de reparto usa `income_split_preview`, en lugar de deducirlo en
  el componente (`Register.tsx:309-313`).
- **Al montar**: `backup_auto()` sin bloquear; el resultado sale como aviso breve.
- **`useSync`**: sondea `ledger_status` cada 30 s y hace `bumpRevision()` solo si cambia `revision`.

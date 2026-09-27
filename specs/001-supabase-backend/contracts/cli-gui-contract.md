# Contract: Superficie de CLI y GUI

**Branch**: `001-supabase-backend` | **Spec**: [spec.md](../spec.md) | **Plan**: [../plan.md](../plan.md)

Cambios en la interfaz de usuario (CLI y GUI). Todo lo demás (add, income, transfer, bucket, account,
entry, concept, budget, report, setup, config) mantiene sus comandos y flags actuales: solo cambia el
backend donde leen/escriben. Manter los handlers delgados (Principio II): ninguna lógica nueva aquí.

## 1. CLI — subcomandos nuevos bajo `db`

| Comando | Flags | Comportamiento |
|---|---|---|
| `db remote status` | — | Muestra: url, estado de sesión (login/fecha de expiración), `revision` remota, último watermark del espejo, totales por tabla, y paridad espejo vs remoto |
| `db remote login` | `--email`, `--password` (opcional; dialoguer si faltan), `--yes` | Crea/renueva sesión (refresh token a keyring); indica expiración |
| `db remote logout` | `--yes` | Revoca sesión en Supabase y borra el refresh token del keyring |
| `db remote migrate` | `--source <ruta>` (default `MONEY_TRACKER_DB`/archivo actual), `--force` | Migración explícita de un solo paso (P3, FR-007): valida esquema (rechaza legacy con error accionable), autentica, comprueba remoto (si tiene data distinta, exige `--force`), copia concepts→config→budgets→accounts→entries con remapeo de ids en memoria, y pobla desde cero el espejo |

Empresa: la migración es idempotente y de un solo paso; no hay "modo mixto" posterior (la base alojada
es la fuente y el archivo previo queda como respaldo de archivo sin renombrar).

## 2. CLI — config/env adicionales

- `MONEY_TRACKER_SUPABASE_URL`, `MONEY_TRACKER_SUPABASE_KEY` (env) / `~/.money-tracker/config.toml`
  (file). Sin sesión → los comandos de escritura fallan con error claro de login; los de lectura
  igual (siempre en línea, FR-008).
- `MONEY_TRACKER_DB` = ruta del espejo (misma variable, nueva semántica; usada por tests).

## 3. GUI — cambios de superficie

- **Settings**: sección nueva "Sincronización" con login/logout (email+password), estado de sesión,
  `last_revision`, y aviso cuando el poll está caído (sin red: banner claro, sin escritura).
- **SetupWizard**: hoy siembra una DB local vacía; pasará a operar contra el remoto (login primero).
- **Sincronización en vivo**: `useSync` — poll ~30 s; ante revisión nueva → `bumpRevision()` para
  refetch global (los hooks `useApi` ya refactoran por revisión). Banner de "sincronizando" no
  bloqueante; los demás hooks no cambian.
- **Id misma**: `Entry.id`/`Account.id` siguen `number` — sin cambios de bindings ts-rs.

## 4. Criterios de aceptación

1. `db remote login` + `report` (sin flags previos) funciona de punta a punta contra el remoto.
2. `db remote migrate` en una base local poblada deja el remoto con la misma data y el reporte por
   período idéntico antes/después (SC-003).
3. `db remote migrate` sobre esquema legacy falla con error accionable indicando la ruta (FR-007).
4. En la GUI, un movimiento registrado desde otra instalación aparece en < 1 min y dispara refetch
   sin recargar la ventana (SC-002).
5. Sin conexión o sin sesión: tanto CLI como GUI rechazan escritura con mensaje claro y no modifican
   el espejo (FR-008, SC-004).
# Research: Supabase como única base de datos, con respaldos y esquema versionado

**Branch**: `002-remove-mirror-backup` | **Date**: 2026-09-26 | **Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

Formato por decisión: **Decision** / **Rationale** / **Alternatives considered**. Las referencias
`archivo:línea` son del estado del repo al 2026-09-26 (commit `5c51146`).

## R1. Dónde viven las reglas contables

- **Decision**: Un módulo de dominio puro, `money_core/src/rules.rs`, con funciones sin I/O:
  - `check_source(account: &AccountBalance, amount) -> Result<()>`: sobregiro en `target`/`emergency`
    y límite de crédito, con deuda = `max(-saldo, 0)`. Se porta tal cual `check_source_locked`
    (`storage/sqlite.rs:198-230`).
  - `check_new_emergency(existing: Option<&AccountBalance>)`: una sola cuenta de emergencia activa.
  - `validate_budget_limit`, `validate_concept_type`.
  - `validate_entry_update(existing: &Entry, upd: &EntryUpdate)`: forma al editar. Hoy está duplicada
    en `sqlite.rs:366-421` y `remote.rs:378-444`.
  - `emergency_split(to: &AccountBalance, emergency: Option<&AccountBalance>, pct, amount) ->
    Option<f64>`: si aplica el reparto y de cuánto. Lo usan `entry_service` y, para la pregunta de
    confirmación, el CLI y la GUI.

  Los servicios las llaman **antes** de escribir. En un lote (ingreso + reparto) se valida cada
  movimiento contra los saldos ya ajustados por los anteriores del mismo lote.
- **Rationale**: Una sola copia de cada regla (FR-017), probable sin base de datos (FR-019). Es el
  principio I de la constitución llevado hasta el final: hoy la regla más importante (sobregiro) vive
  en dos backends distintos, y la de Supabase tiene un bug (R6).
- **Alternatives considered**: (a) Dejar las reglas en los backends y probar el falso con una copia:
  rechazado, porque mantiene la duplicación que causó el bug. (b) Traits de validación inyectables:
  sobra para cinco funciones puras (principio V).
- **Nota sobre atomicidad**: validar en Rust antes de escribir no es atómico frente a dos dispositivos
  escribiendo a la vez. Supabase conserva su validación con bloqueo de fila (R6) como defensa. Con un
  solo usuario, el caso es teórico.
- **Edición de movimientos**: `update_entry` hoy no revalida sobregiro (`entry_service.rs:128`), para no
  impedir correcciones históricas cuyo saldo posterior ya cambió. Se conserva: solo se valida la forma.

## R2. Servicios faltantes (principio II)

- **Decision**: Agregar `budget_service` (`set`, `list`, `remove`) y `concept_service` (`list`, `add`),
  porque ahí van dos reglas de R1. Los handlers dejan de llamar `be.set_budget`/`be.add_concept`
  directo (`cli budget.rs:128,144`, `concept.rs:35,60`, `add.rs:57`, `income.rs:57`; GUI `budgets.rs`,
  `concepts.rs`).
- **Rationale**: La regla necesita un lugar en el dominio antes de la escritura, y el principio II pide
  un servicio por comando.
- **Alternatives considered**: Un `config_service` también. Se deja fuera: `config get/set` no tiene
  reglas, y agregarlo es limpieza fuera del alcance. Queda anotado como desviación previa.

## R3. Almacén falso para pruebas

- **Decision**: `money_core/src/storage/memory.rs` con `MemoryBackend`, compilado solo con
  `#[cfg(any(test, feature = "test-support"))]`. `money_core` se agrega a sí mismo como
  dev-dependency con `features = ["test-support"]`, para que `tests/scenarios.rs` lo vea. Todo el
  estado va dentro de un `Mutex`, porque el trait exige `Send + Sync` (`storage/mod.rs:52`).
  Comportamiento de almacén que debe imitar, porque las pruebas actuales dependen de él:
  - ids crecientes desde 1; `NotFound` en ids inexistentes;
  - nombres únicos de cuenta y de concepto; presupuesto único por (concepto, período);
  - el mismo orden que hoy: cuentas por `kind, name`; movimientos por `date desc, id desc`; conceptos,
    presupuestos y configuración como en `sqlite.rs:522-646`;
  - lotes atómicos: si un movimiento del lote falla, no se guarda ninguno;
  - `MemoryBackend::seeded()` crea los mismos 10 conceptos y `emergency_pct = 10` que sembraba
    `db.rs:9-20,266`, para que las pruebas existentes no cambien.

  Las reglas contables **no** van en el falso; viven en `rules.rs`.
- **Rationale**: Las cinco funciones auxiliares que crean el almacén (`setup()` en `account_service`,
  `entry_service` y `report_service`, `setup_db()`, `fresh_db()`) hacen que el cambio sea de una línea
  por archivo. Las aserciones no cambian (principio IV).
- **Alternatives considered**: Compilarlo siempre como parte pública de la librería: rechazado por
  FR-019. Un crate aparte: sobra.

## R4. Pruebas que se eliminan o se reescriben

Por principio IV, cada una con su razón, repetida en el mensaje del commit:

| Prueba (archivo) | Destino | Razón |
|---|---|---|
| `schema_creates_cleanly` (db.rs) | Eliminar | El esquema SQLite deja de existir |
| `malformed_date_rejected` (db.rs) | Eliminar | Ya la cubre `period::validate_date` |
| `only_one_active_emergency_account` (db.rs) | Reescribir en `rules.rs` | La regla pasa al dominio |
| `target_account_allows_open_ended_bucket` (db.rs) | Reescribir en `models/account.rs` | Regla del modelo |
| `spending_account_rejects_target_amount` (db.rs) | Reescribir en `models/account.rs` | Regla del modelo |
| `accounts_and_entries_roundtrip` (sqlite.rs) | Eliminar | Prueba del almacén SQLite |
| `push_entries_is_atomic_on_overdraft` (sqlite.rs) | Reescribir en `entry_service` | Atomicidad del lote en el servicio |
| `apply_remote_snapshot_*` (sqlite.rs, 3 pruebas) | Eliminar | El espejo deja de existir |

Siguen sin cambios: `settings.rs` (2), `period.rs` (6), `ledger.rs` (3) y las 33 de servicios y
escenarios (solo cambia cómo se crea el almacén).

## R5. Versión de esquema

- **Decision**:
  - Tabla `public.schema_version (id = 1, version int, applied_at timestamptz)` con RLS de solo
    lectura para `authenticated`. La crea `0002` y la deja en `2`.
  - Cada archivo `NNNN_*.sql` abre `begin`, verifica que la versión sea `NNNN - 1` o aborta con
    `raise exception`, hace su cambio, fija la versión en `NNNN` y cierra con `commit`. `0002` es el caso
    especial: verifica que exista `public.entries` (señal de `0001`) y que **no** exista
    `schema_version`.
  - La app conoce `money_core::schema::EXPECTED_SCHEMA_VERSION`. Al conectarse lee la versión con el
    RPC `ledger_status()` (R8), y si no coincide falla con `AppError::SchemaMismatch { found, expected
    }`. El mensaje dice qué archivo aplicar (base atrás) o que se actualice el binario (base adelante).
  - Candado en git: una prueba de `money_core` lee `supabase/sql/` y verifica que el número del último
    archivo sea `EXPECTED_SCHEMA_VERSION`.
- **Rationale**: Es el flujo manual que ya usa el usuario (SQL Editor), con las protecciones dentro del
  propio SQL (FR-013 a FR-015). Git versiona los archivos; `schema_version` dice cuáles corrieron.
- **Alternatives considered**: Migraciones del CLI de Supabase (`supabase db push`): rechazadas en la
  clarificación (más herramientas y la contraseña de la base cada vez). Un hash de cada archivo: sobra.
- **Archivos**: `supabase/migrations/0001_initial.sql` → `git mv` a `supabase/sql/0001_setup.sql`, sin
  tocar el contenido. La carpeta `supabase/migrations/` desaparece, para que `supabase db push` no la
  tome por error.

## R6. Correcciones del esquema en `0002` (FR-016)

- **Decision**:
  1. **Límite de crédito** (`0001:292`): `least(v_balance, 0) + v_amt` → `greatest(-v_balance, 0) +
     v_amt`. Verificado en vivo: el proyecto corre la versión con el error.
  2. **Bloqueo**: antes de calcular el saldo, `perform 1 from accounts where id = v_from and user_id =
     auth.uid() for update`, para serializar escrituras concurrentes sobre la misma cuenta.
  3. **Cuenta destino**: si `v_to` no es nulo, debe existir con `user_id = auth.uid()`; si no, error.
  4. **`sync_state`**: se reemplaza la política `for all using (true)` (`0001:441-443`) por una de
     `select` para `authenticated`. `touch_row` es `security definer` y sigue pudiendo escribir.
     Verificado en vivo: sin sesión, `GET /rest/v1/sync_state` hoy devuelve la revisión.
  5. **Funciones**: `revoke execute ... from public, anon` en todas las funciones de `public`, y `grant`
     explícito a `authenticated` solo en los RPC que usa la app.
  6. **Objetos del espejo**: se borran `pull_changes`, la tabla `tombstones`, sus triggers y
     `tombstone_after_delete`. **Efecto colateral**: ese trigger era lo único que subía `revision` al
     borrar, así que se reemplaza por `bump_revision_after_delete()`, un trigger `after delete` en las
     cinco tablas que solo sube la revisión. Sin él, la GUI no vería borrados hechos en otra máquina.
  7. **Comentarios**: el encabezado deja de describir el esquema como réplica del espejo SQLite.

  Se conservan las columnas `rev` y `updated_at`: `touch_row` las mantiene, no estorban, y quitarlas es
  churn sin beneficio.
- **Alternatives considered**: Corregir `0001` en su lugar: prohibido por FR-013.

## R7. Consultas de Supabase: paginación y filtro por período

- **Decision**:
  1. **Paginación**: ninguna consulta de lista en `remote.rs` pagina. Por encima del tope de filas de
     PostgREST (1000 por defecto en Supabase) los resultados se truncarían sin aviso, y los saldos y
     reportes descargan **todos** los movimientos. Se agrega un ciclo por páginas de 1000 (encabezado
     `Range` o `limit`/`offset` con orden estable) en `entries`, `raw_accounts`, `list_concepts`,
     `list_budgets` y `list_config`. Hoy hay 107 movimientos; SC-004 pide 1,100.
  2. **Filtro por período** (`remote.rs:185`): usa `or=(date.gte.lo,date.lt.hi)`, que acepta cualquier
     fecha. Verificado: `entry list -p 2026-08` devuelve los 107 movimientos, 41 de ellos de
     septiembre. Se cambia a `and=(...)`, o a dos parámetros `date`, cuidando no chocar con el `or=` del
     filtro por cuenta.
- **Rationale**: Son errores de corrección del único almacén que queda. Los reportes no los muestran
  porque filtran en memoria (`ledger.rs:49`), pero `entry list -p` y la vista Movimientos sí.

## R8. RPC nuevos

- **Decision**:
  - `ledger_status() returns jsonb` → `{ revision, schema_version }`. Se usa en la revisión de versión
    al conectar, en el sondeo de la GUI cada ~30 s y en `db remote status`. Reemplaza el `GET
    sync_state` de `remote.rs:579`.
  - `export_ledger() returns jsonb` → `{ revision, schema_version, exported_at, concepts, accounts
    (incluidas las archivadas), entries, budgets, config }`, todo en **una** llamada, así que todo sale
    de la misma foto de la base (el mismo argumento de `pull_changes`, `0001:364-368`). Un `jsonb` es un
    solo valor, así que no le aplica el tope de filas.

  Los dos son `security invoker` (RLS filtra por usuario), `stable`, con `grant` solo a
  `authenticated`.
- **Alternatives considered**: Varios `GET` para el respaldo: rechazado, porque no darían una foto
  consistente (FR-009) y sí toparían con el límite de filas.

## R9. Formato del respaldo

- **Decision**: Un archivo `.sql` generado por `backup_service`, con este orden:
  1. Encabezado comentado: fecha, revisión, versión de esquema, email del usuario de origen, e
     instrucciones de restauración.
  2. `begin;`
  3. Bloque de guardas (`do $$ ... $$`): la versión de esquema debe ser la del respaldo; `accounts`,
     `entries`, `concepts`, `budgets` y `config` deben estar vacías; el email del restaurador debe
     existir exactamente una vez en `auth.users`.
  4. `create temp table _restore_user on commit drop as select id from auth.users where email =
     '<email>'`. El email viene **prellenado** con el usuario de la sesión que hizo el respaldo, así
     que restaurar con el mismo email no requiere editar nada. Es el único dato a completar (FR-008).
  5. `insert` con ids explícitos y `user_id` del restaurador, en orden de dependencias: `concepts` →
     `accounts` → `entries` → `budgets` → `config`.
  6. `select setval(pg_get_serial_sequence('public.<tabla>', 'id'), max(id))` por tabla con secuencia,
     para que la app pueda seguir insertando.
  7. `commit;`

  Detalles:
  - Los literales de texto se escapan duplicando `'` (con `standard_conforming_strings = on`, que es el
    valor por defecto). Los números se escriben con la representación exacta de `f64`, y las fechas
    como `'YYYY-MM-DD'`.
  - Los triggers `touch_row` corren durante la restauración y suben `revision`. Eso es correcto: el
    proyecto nuevo empieza su propia historia de revisiones.
- **Rationale**: Se ejecuta en el mismo lugar donde se aplica el esquema (el SQL Editor), sin
  herramientas nuevas. `begin`/`commit` y las guardas garantizan "todo o nada" y "solo sobre un
  proyecto vacío".
- **Alternatives considered**:
  - JSON más un script de importación: más piezas.
  - `pg_dump`: necesita la contraseña de la base y Postgres instalado, y trae el esquema mezclado con
    los datos.
  - Un comando `restore` en la app: rechazado en el spec (principio V).
- **Dato necesario**: `auth.rs` hoy descarta el objeto `user` de la respuesta de GoTrue
  (`auth.rs:17-21`). Se agrega `user: { id, email }` a `TokenResponse` y se guarda en la sesión en
  memoria. El refresh que ya hace cada comando lo trae, así que no cuesta una llamada extra.

## R10. Respaldo automático perezoso

- **Decision**:
  - **Registro**: `~/.money-tracker/last-backup.toml`, con `at` (RFC 3339), `path`, `revision` y
    `schema_version`. Si falta o no se puede leer, cuenta como "nunca hubo respaldo".
  - **Plazo**: se hace un respaldo si `now - at > 7 días`. La fecha se lee de un archivo local, así que
    la revisión no toca la red; solo se conecta a Supabase cuando toca respaldar.
  - **Archivos**: van a `~/.money-tracker/backups/money-tracker-YYYYMMDD-HHMMSS.sql`, con sufijo `-2`,
    `-3`… si el nombre ya existe. El contenido se arma completo en memoria y luego se escribe con
    `OpenOptions::create_new` y permisos `0600`, así que nunca sobrescribe. Si la escritura falla, se
    borra el archivo. El registro se actualiza solo después de escribir con éxito.
  - **API de dominio**: `backup_service::{create(be, dest), is_due(now, record), run_auto_if_due(now,
    connect)}`. `connect` es una closure que construye el backend solo si hace falta.
  - **CLI**: en `main.rs`, después del `match`, solo si `result.is_ok()` y el comando no es
    `db backup`, `db remote login` ni `db remote logout`. Un respaldo automático fallido imprime una
    advertencia en stderr y no cambia el código de salida.
  - **GUI**: el frontend llama al comando `backup_auto` al montar la app, sin bloquear el render. El
    resultado sale como aviso breve (hecho o advertencia).
- **Rationale**: Es la regla de Q1 tal como se aclaró, sin costo en los comandos normales.
- **Alternatives considered**: Revisar la fecha del último respaldo contra Supabase: costaría una
  llamada por comando. Un hilo de fondo en el CLI: el proceso termina antes que el hilo.

## R11. Arranque sin configuración, conexión y GUI

- **Decision**:
  - `storage::connect(&Settings) -> Result<Box<dyn LedgerBackend>>` reemplaza a `production_backend`.
    Sin URL o sin key devuelve `AppError::NotConfigured`, con instrucciones (`db remote login --url
    --key`). Luego llama a `ledger_status()` y compara la versión (R5).
  - **GUI, estado**: `AppState.backend` pasa a `Mutex<Option<Box<dyn LedgerBackend>>>`. Se construye
    al primer uso y se reconstruye tras `remote_login`. Hoy `lib.rs:9` hace `expect` y la app entra en
    pánico antes de abrir la ventana; eso se quita.
  - **GUI, pantalla de conexión**: `routes/Connect.tsx` (URL, key publicable, email, contraseña). Se
    muestra cuando `ledger_status` responde `not_configured`, `auth_needed` o `schema_mismatch`, antes
    del gate del SetupWizard. Hoy la GUI no pide URL ni key; solo existe el login en Ajustes.
  - **`remote_login`** acepta `url` y `key` opcionales, igual que el CLI: los guarda en `config.toml` y
    reconstruye el backend.
- **Rationale**: Supabase pasa a ser requisito, así que la primera pantalla tiene que poder
  configurarlo (principio II: el CLI ya puede).

## R12. Sondeo de cambios en la GUI

- **Decision**: `useSync` llama cada 30 s a `ledger_status` y hace `bumpRevision()` solo cuando
  `revision` cambia. Se conserva `useRefetchOnFocus`.
- **Rationale**: Es lo mismo que hoy, sin espejo. Con R6.6, los borrados también mueven la revisión.

## R13. Limpieza de dependencias y archivos

- **Decision**:
  - Quitar `rusqlite` de `money_core` (bundled), de `cli/Cargo.toml:12` y de
    `gui/src-tauri/Cargo.toml:26` (los dos últimos no lo usan).
  - Borrar `db.rs`, `sync/`, `storage/sqlite.rs`, `settings::mirror_path`, la variable
    `MONEY_TRACKER_DB`, los errores `AppError::{Database, LegacySchema, SchemaTooNew, SchemaTooOld}`
    (reemplazados por `SchemaMismatch` y `NotConfigured`), y `scripts/migrar_a_supabase.sh`.
  - `setup_inicial.sh` y `presupuesto.sh` se quedan; solo se actualizan sus comentarios, que
    recomiendan `MONEY_TRACKER_DB`.
  - `~/.money-tracker/data.db` y los respaldos SQLite viejos del home no se tocan; la documentación
    dice que se pueden borrar.
- **Verificación**: `cargo tree -p money_core | grep -i sqlite` debe quedar vacío.

## R14. Constitución (FR-023)

- **Decision**: Enmienda **1.0.1 → 2.0.0** (MAJOR, porque redefine partes de principios):
  - Principio II: se quita "compartir el mismo libro mayor (modo WAL)"; queda "compartir el mismo
    libro mayor alojado".
  - Principio V: "rechazar bases con esquema previo al rediseño" se declara **deprecado** (ya no existe
    base local). Su sucesor es la revisión de `schema_version`.
  - Portones: las pruebas usan el almacén en memoria, y la verificación manual usa un proyecto de
    Supabase de prueba en lugar de `MONEY_TRACKER_DB`. Se agregan dos reglas: un archivo de esquema
    aplicado no se edita, y cada archivo nuevo sube `EXPECTED_SCHEMA_VERSION` y pasa `verify.sql` en
    el proyecto de prueba antes de aplicarse en producción.

## R15. Sesiones separadas por proyecto

- **Decision**: La entrada del llavero pasa de `supabase_refresh_token` (fija, `auth.rs:12-13`) a
  `supabase_refresh_token:<project-ref>`, donde `<project-ref>` es el subdominio de `supabase_url`. El
  archivo de respaldo del token ya vive bajo `config_dir()`, así que se separa solo con
  `MONEY_TRACKER_CONFIG`. No se migra la entrada vieja: después de actualizar hay que iniciar sesión
  una vez, y la entrada vieja se borra en `logout`.
- **Rationale**: El simulacro de restauración (SC-003, SC-005) usa un segundo proyecto. Con la entrada
  fija, iniciar sesión en el proyecto de prueba pisaría la sesión de producción.
- **Alternatives considered**: Copiar el token viejo a la entrada nueva de forma automática: es una
  capa de compatibilidad para ahorrar un solo login.

## R16. Puesta en producción: esquema en la versión 1 y binario que espera la 2

- **Problema**: El binario nuevo se niega a conectarse mientras el esquema esté en la versión 1, y
  `export_ledger` no existe hasta `0002`. Por eso el primer respaldo con la app solo es posible
  **después** de aplicar `0002` a producción.
- **Decision**:
  1. Validar `0001` + `0002` + `verify.sql` en el proyecto de prueba.
  2. Antes de aplicar `0002` a producción, sacar una **copia de seguridad previa** con una consulta de
     solo lectura en el SQL Editor, que junta las cinco tablas en un solo JSON. Corre como `postgres`,
     así que no la filtra RLS. El usuario guarda el resultado en un archivo.
  3. Aplicar `0002` a producción. Es transaccional y no toca filas contables: solo funciones,
     políticas y triggers, y borra la tabla `tombstones`, que solo contenía datos del espejo.
  4. Instalar el binario nuevo y hacer de inmediato `db backup`.

  El quickstart describe estos pasos en orden.
- **Compatibilidad hacia atrás**: Un binario viejo contra el esquema 2 sigue escribiendo, porque
  `apply_entries` conserva su firma. Solo falla su refresco del espejo (`pull_changes` ya no existe), y
  lo reporta como advertencia (`sync/mod.rs:36-43`). No corrompe datos.

## R17. Sesión en archivo en lugar del llavero (FR-024)

- **Causa del aviso de macOS**: el binario tiene firma `adhoc` (verificado con `codesign -dv`), así que
  su identidad cambia con cada compilación, y el CLI, `cargo run` y la GUI son tres binarios distintos.
  Además, cada comando del CLI **lee y reescribe** el token en el llavero, porque Supabase entrega un
  refresh token nuevo en cada renovación (`auth.rs:128-142`). Por eso "Permitir siempre" nunca dura.
- **Decision**:
  - Nueva clave en `config.toml`: `token_storage = "keychain" | "file"`. Por defecto `keychain`; si
    falta o tiene un valor desconocido, también se usa `keychain`.
  - Con `file`: `auth.rs` usa solo `~/.money-tracker/refresh_token` (`0600`, el archivo de respaldo
    que ya existe, `auth.rs:144-160`) y **no llama a `keyring` en ninguna ruta**, ni para leer, ni para
    escribir, ni para borrar, porque cualquier acceso al llavero dispara el aviso.
  - `logout` borra el archivo; en modo `keychain`, la entrada del llavero.
  - Cambiar de modo no migra el token: se inicia sesión una vez con el modo nuevo.
  - `db remote status` y la GUI muestran dónde está guardada la sesión.
- **Rationale**: Es lo que eligió el usuario. Protege lo mismo que `config.toml`, y funciona igual en
  procesos sin sesión gráfica (MCP o API a futuro).
- **Alternatives considered**: Firmar los binarios con un certificado propio: conserva el llavero pero
  agrega un paso a cada compilación. Marcar la entrada como "todas las apps" en Keychain Access: se
  pierde con cada login y protege lo mismo que el archivo.
- **Relación con R15**: con `file`, la sesión ya queda separada por proyecto gracias a
  `MONEY_TRACKER_CONFIG`. La entrada del llavero por proyecto solo aplica en modo `keychain`.

# Arquitectura

Cómo está hecho money-tracker por dentro, para quien vaya a tocar el código. Qué es y cómo se usa:
[`README.md`](../README.md). Cómo instalarlo: [`INSTALACION.md`](INSTALACION.md). Convenciones y comandos de
desarrollo: [`AGENTS.md`](../AGENTS.md).

## Índice

1. [Visión general](#1-visión-general)
2. [money_core: el modelo](#2-money_core-el-modelo)
3. [Almacenamiento y Supabase](#3-almacenamiento-y-supabase)
4. [La GUI por dentro](#4-la-gui-por-dentro)
5. [Respaldos por dentro](#5-respaldos-por-dentro)
6. [Pruebas](#6-pruebas)
7. [Decisiones y referencias](#7-decisiones-y-referencias)

---

## 1. Visión general

Workspace de Rust con tres crates y un frontend:

```
money_core/        librería: modelos, reglas contables, servicios y el almacén de Supabase
cli/               binario `money-tracker`: clap + prompts de dialoguer; manejador delgado sobre money_core
gui/src-tauri/     backend Tauri v2 (crate `gui`): un comando por llamada a un servicio de money_core
gui/src/           frontend React + TypeScript (CSS Modules, sin router ni librería de queries)
setup/             lo que se corre una vez: esquema de Supabase (sql/), verify.sql (tests/), scripts de carga y guía
```

Tres ideas sostienen todo:

- **Modelo primero**: toda regla de negocio vive en `money_core`, que no imprime, no pregunta, no parsea argumentos y
  no depende de nada del CLI ni de la GUI (`cargo tree -p money_core` nunca debe mostrar `clap`, `dialoguer`, `tabled`
  ni `tauri`).
- **Manejadores delgados**: el CLI y la GUI solo adaptan entrada, salida y errores; ninguno repite reglas.
- **Un solo almacén**: Supabase. No hay base de datos local, ni modo local, ni espejo (spec `002-remove-mirror-backup`).

Estos principios son obligatorios: están en la constitución del proyecto (`.specify/memory/constitution.md`).

---

## 2. money_core: el modelo

### El modelo de datos

Todo movimiento de dinero es una **entrada** (`entry`) entre **cuentas**, o a través del borde del sistema. Dos tablas
cargan con todo el modelo (en Supabase, por usuario y con RLS):

```
accounts(id, user_id, name UNIQUE por usuario, kind CHECK IN ('spending','emergency','target','credit'),
         target_amount, credit_limit, liquid, archived)

entries(id, user_id, date, kind CHECK IN ('income','expense','transfer','opening'),
        amount CHECK(amount > 0),           -- siempre positivo; kind da la dirección
        from_account_id, to_account_id,     -- exactamente una de estas formas:
        concept, subconcept, description)   --   income/opening: solo to · expense: solo from · transfer: ambos
```

Más `concepts`, `budgets` y `config` (las claves como `emergency_pct`), también por usuario.

**Los saldos se derivan, nunca se almacenan**: `storage::ledger::derive_balances` suma los movimientos de cada cuenta
(y la vista `account_balances` hace lo mismo en SQL). No existe ningún `UPDATE ... saldo` en el código, así que el
saldo no puede desincronizarse del historial, y un saldo inicial sigue ahí el mes siguiente.

### Las reglas contables

Viven una sola vez, en `money_core/src/rules.rs`, como funciones puras (sin I/O), y los servicios las aplican
**antes** de escribir:

| Regla | Función |
|---|---|
| Un bucket (`target`/`emergency`) no puede quedar en negativo | `check_source` |
| Un cargo a la tarjeta no puede pasar el disponible (límite − `max(-saldo, 0)`) | `check_source` |
| Una sola cuenta de emergencia activa | `check_new_emergency` |
| `target_amount` solo en `target`, `credit_limit` solo en `credit`, ambos positivos | `validate_new_account` |
| Presupuesto positivo; tipo de concepto `expense`/`income`/`both` | `validate_budget_limit`, `validate_concept_type` |
| Al editar, solo se mueve el lado de cuenta que aplica al tipo | `merge_entry_update` |
| Cuánto de un ingreso va al fondo de emergencia | `emergency_split` |

`entry_service::push_checked` es la única puerta por la que los movimientos llegan al almacén: valida cada movimiento
de un lote contra los saldos ya ajustados por los anteriores del mismo lote, y luego escribe el lote de forma atómica.
Los manejadores preguntan al dominio en lugar de deducir reglas; por ejemplo, el CLI y la GUI usan
`entry_service::emergency_split_preview` para saber si un ingreso aportará al fondo.

Supabase repite las reglas más importantes como **segunda defensa**: CHECKs, el índice único parcial de la cuenta de
emergencia y `apply_entries`, que bloquea la cuenta origen durante la validación para que dos escrituras simultáneas
desde dos dispositivos se validen en serie.

### Servicios

```
services/
  account_service.rs   crear (valida), listar, archivar, cuadrar
  entry_service.rs     único escritor de movimientos: ingreso (con reparto), gasto, transferencia, saldo inicial,
                       edición y vista previa del reparto
  budget_service.rs    presupuestos (informativos)
  concept_service.rs   conceptos
  report_service.rs    reporte mensual, patrimonio neto
  setup_service.rs     saldos iniciales
  backup_service.rs    respaldos (sección 5)
```

### Estructura de `money_core/src`

```
rules.rs        reglas contables puras
schema.rs       EXPECTED_SCHEMA_VERSION + check_schema; una prueba la compara con setup/sql/
period.rs       Period ("YYYY-MM") — única fuente de verdad para fechas
auth.rs         login y renovación con Supabase Auth; sesión en llavero (por proyecto) o en archivo
settings.rs     config.toml, variables de entorno y rutas de ~/.money-tracker
storage/        trait LedgerBackend y connect(), SupabaseBackend, MemoryBackend (solo pruebas), ledger.rs (saldos)
models/         cuentas, movimientos, presupuestos, conceptos, config, estado del libro, respaldo
services/       ver arriba
```

---

## 3. Almacenamiento y Supabase

`LedgerBackend` (`storage/mod.rs`) solo **guarda y lee**; no contiene reglas contables. Hay dos implementaciones:

- **`SupabaseBackend`** (`storage/remote.rs`): habla con la API REST de Supabase (PostgREST) con la publishable key y
  el token de la sesión. Lee las tablas y vistas con paginación de 1000 filas (PostgREST trunca en silencio más allá
  de eso), escribe movimientos con el RPC `apply_entries` y usa `ledger_status()` y `export_ledger()`.
- **`MemoryBackend`** (`storage/memory.rs`): almacén en memoria para las pruebas; solo existe con `cfg(test)` o la
  feature `test-support`.

`storage::connect(&Settings)` es la única forma de obtener un backend:

1. Sin URL o sin key → `AppError::NotConfigured`, con instrucciones y sin tocar la red.
2. Llama a `ledger_status()`, lo que además prueba que la sesión sirve.
3. Compara la versión del esquema con `EXPECTED_SCHEMA_VERSION` → `AppError::SchemaMismatch` si no coincide, con un
   mensaje que dice qué archivo aplicar (base atrasada) o que hay que actualizar la app (app atrasada).

**Sesión** (`auth.rs`): el access token vive solo en memoria y se renueva solo con el refresh token. El refresh token
se guarda según `token_storage` en `config.toml`: en el llavero del sistema (una entrada por proyecto,
`supabase_refresh_token:<ref>`) o en `~/.money-tracker/refresh_token` (`0600`). En modo archivo ningún camino del
código toca el llavero. Supabase rota el refresh token en cada renovación, y el CLI y la GUI comparten el mismo
almacenamiento, así que no se pisan.

**Versión de esquema**: cada cambio al esquema es un archivo numerado en `setup/sql/` que verifica la versión
anterior y sube `schema_version`, dentro de una transacción. Un archivo aplicado nunca se edita. Proceso completo en
[`setup/README.md`](../setup/README.md), sección 3.

---

## 4. La GUI por dentro

**Backend (`gui/src-tauri/src/`)**: un archivo por grupo de comandos en `commands/`, cada comando una llamada a un
servicio de `money_core`.

- Todos los comandos son `async`, y su trabajo corre en el pool de hilos bloqueantes (`state::blocking`). Un comando
  síncrono de Tauri corre en el hilo principal, y cada consulta a Supabase congelaría la ventana.
- `AppState` guarda la conexión (`Arc`), creada al primer uso con `connect()`. El candado solo se toma para copiar el
  `Arc` (o, una vez, mientras conecta), así que las consultas del frontend corren en paralelo.
- Un backend nunca se destruye en un hilo async: su cliente HTTP bloqueante lleva un runtime interno que entra en
  pánico si se destruye ahí. Los comandos mueven su copia a `blocking`, y `reset()` destruye la vieja en un hilo
  normal.
- `ApiError` traduce `AppError` para el frontend. Su `kind` (`not_configured`, `auth`/`auth_needed`,
  `schema_mismatch`) decide si se muestra la pantalla Conexión.

**Frontend (`gui/src/`)**: sin router ni librería de queries, porque el IPC es local.

- `hooks/useApi.ts`: carga al montar, y un contador de "revisión" global que, al subir tras cada cambio, hace que todo
  componente vuelva a pedir sus datos.
- `hooks/useSync.ts`: cada ~30 s pregunta `ledger_status` a Supabase y sube la revisión solo si cambió. Supabase sube
  su contador en cada alta, cambio y borrado (triggers `touch_row` y `bump_revision_after_delete`).
- `App.tsx`, en orden: pantalla Conexión (si `ledger_status` falla por configuración, sesión o esquema) → SetupWizard
  (sin cuentas) → pestañas. `api/client.ts` emite un evento si cualquier llamada falla por sesión, y la app vuelve
  sola a Conexión.
- `bindings/`: tipos generados desde Rust con ts-rs (`cargo test -p money_core --features ts-rs`); no se editan a mano.

---

## 5. Respaldos por dentro

`backup_service` (`money_core/src/services/backup_service.rs`):

- `export_snapshot()` llama al RPC `export_ledger()`: todo el libro en **una** llamada, así sale de una sola foto de la
  base aunque otro dispositivo esté escribiendo, y un `jsonb` no está sujeto al tope de filas.
- `render_sql` lo convierte en un script que se restaura en el SQL Editor. El script verifica la versión de esquema y
  que el proyecto esté vacío, asigna cada fila al usuario del email indicado (prellenado con el de la sesión, editable
  en la línea `RESTAURAR COMO`), inserta con ids explícitos en orden de dependencias y ajusta las secuencias, todo
  entre `begin` y `commit`. La salida es determinista (filas por id), y una prueba la compara con un texto esperado.
- El archivo se escribe con `create_new` y permisos `0600`: nunca sobrescribe, y si falla no deja un archivo parcial.
  Después se actualiza `~/.money-tracker/last-backup.toml`.
- **Automático**: `run_auto_if_due` (CLI, después de cada comando exitoso salvo `db backup` y `db remote login/logout`)
  y `run_auto_with` (GUI, al abrir) solo leen `last-backup.toml` y se conectan si el último respaldo tiene más de 7
  días. Una falla es un aviso y nunca cambia el resultado del comando.

---

## 6. Pruebas

- `cargo test --workspace` corre **sin red ni base de datos**, sobre `MemoryBackend`. `MemoryBackend` solo imita lo
  que garantiza un almacén (ids, unicidad, llaves foráneas, lotes atómicos), nunca reglas contables: así las pruebas
  de servicios prueban las reglas de `rules.rs`.
- `money_core/tests/scenarios.rs`: escenarios de caja negra por la API pública.
- `schema.rs`: una prueba verifica que `EXPECTED_SCHEMA_VERSION` coincide con el último archivo de `setup/sql/`.
- `setup/tests/verify.sql`: comprueba en un Supabase (idealmente local) que el esquema rechaza lo que debe; termina
  en `ROLLBACK`.
- Para probar a mano, un Supabase local con Docker y `MONEY_TRACKER_CONFIG` apuntando a un `config.toml` aparte:
  [`setup/README.md`](../setup/README.md), sección 5.

---

## 7. Decisiones y referencias

- **Especificaciones** (qué se decidió y por qué): `specs/001-supabase-backend/` (Supabase como fuente) y
  `specs/002-remove-mirror-backup/` (Supabase como único almacén, reglas al dominio, respaldos y esquema versionado),
  cada una con `spec.md`, `plan.md`, `research.md` y `tasks.md`.
- **Constitución**: `.specify/memory/constitution.md`.
- **Esquema**: [`setup/README.md`](../setup/README.md).
- **Guía de desarrollo**: [`AGENTS.md`](../AGENTS.md).

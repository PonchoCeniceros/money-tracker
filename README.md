# money-tracker

Control de finanzas personales desde la terminal y/o una app de escritorio nativa. Cuentas (efectivo,
débito, vales, tarjeta de crédito), buckets de ahorro (fondo de emergencia + metas), presupuestos
informativos, y un reporte mensual que distingue "cuánto gasté" de "cuánto salió de mi bolsillo".

Tus datos viven en **tu propio proyecto de Supabase** (tiene plan gratuito): el CLI y la GUI leen y escriben
directo ahí, desde cualquier máquina. No hay ninguna base de datos local que se pueda desincronizar. Para no
depender solo de Supabase, la app hace **respaldos**: a mano cuando quieras y solos cada 7 días (sección 5).

Reemplaza un dashboard de Excel que se había vuelto engorroso de mantener. El Excel
(`Dashboard_Financiero.xlsx`/`.ods`) queda como **referencia histórica de solo consulta**; no se importa.

---

## Índice

1. [Instalación y ejecución](#1-instalación-y-ejecución)
2. [Core — cómo funciona](#2-core--cómo-funciona)
3. [CLI — set de instrucciones](#3-cli--set-de-instrucciones)
4. [GUI — vistas y funcionalidades](#4-gui--vistas-y-funcionalidades)
5. [Respaldo y restauración](#5-respaldo-y-restauración)
6. [Ejemplos](#6-ejemplos)
7. [Apéndice: problemas comunes](#apéndice-problemas-comunes)

---

## 1. Instalación y ejecución

### 1.1 Prerrequisitos

```sh
# Rust toolchain: compila core, CLI y el backend de la GUI
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Node.js + pnpm: solo para la GUI (no hace falta para el CLI)
corepack enable   # o: npm i -g pnpm
```

Y una cuenta en [supabase.com](https://supabase.com) (el plan gratuito alcanza).

### 1.2 Crear tu proyecto de Supabase (una sola vez)

1. En Supabase, **New Project**. Anota la **Project URL** (`https://<ref>.supabase.co`) y la **publishable key**
   (Project Settings → API, la `sb_publishable_…`). Nunca uses la `service_role` en la app.
2. En el **SQL Editor** del proyecto, pega y corre en orden cada archivo de `supabase/sql/`: primero
   `0001_setup.sql`, luego `0002_schema_version.sql`, y así hasta el último.
3. En **Authentication → Users**, crea tu usuario (email y contraseña).

Detalle, qué crea cada archivo y cómo verificarlo: [`supabase/README.md`](supabase/README.md).

### 1.3 Clonar y compilar

```sh
git clone <repo-url> money-tracker
cd money-tracker
cargo build --workspace   # compila money_core + cli + gui/src-tauri
cargo test --workspace    # deben pasar todas (no usan red ni base de datos)
```

### 1.4 Archivos de configuración

`money_core` es una librería (no un binario): el CLI y la GUI la envuelven, y comparten estos archivos, todos en
`~/.money-tracker/`:

| Archivo | Qué guarda | Quién lo escribe |
|---|---|---|
| `config.toml` | URL y publishable key del proyecto; dónde va la sesión | `db remote login` o la pantalla Conexión |
| `refresh_token` | La sesión, si `token_storage = "file"` (permisos `0600`) | El login y cada renovación |
| `backups/` | Los respaldos `.sql` | `db backup` y el respaldo automático |
| `last-backup.toml` | Fecha y ruta del último respaldo | Cada respaldo exitoso |

```toml
# ~/.money-tracker/config.toml
supabase_url = "https://<ref>.supabase.co"
supabase_publishable_key = "sb_publishable_…"
token_storage = "file"   # opcional: "keychain" (por defecto) o "file"
```

- **`token_storage`**: con `"keychain"` la sesión va al llavero del sistema, una entrada por proyecto. Con `"file"`
  va a `refresh_token` y la app nunca toca el llavero: macOS deja de pedirte la contraseña de la laptop en cada uso
  (ver el [apéndice](#apéndice-problemas-comunes)).
- **`MONEY_TRACKER_CONFIG=/ruta/config.toml`** apunta la app a otro `config.toml` y mueve con él los demás archivos
  de la tabla. Sirve para probar contra un Supabase local sin tocar tu configuración real.
- **`MONEY_TRACKER_SUPABASE_URL`** y **`MONEY_TRACKER_SUPABASE_KEY`** ganan sobre lo que diga `config.toml`.
- Las **reglas del libro contable** (`emergency_pct`, `default_account`, …) no viven en un archivo sino en
  Supabase (tabla `config`); se cambian con `config set` o en **Ajustes**. Ver la
  [tabla de claves](#claves-de-configuración).

### 1.5 CLI

**Desde el proyecto** (recompila si hiciste cambios):

```sh
cargo run -p money-tracker -- db remote login --url https://<ref>.supabase.co --key sb_publishable_…
cargo run -p money-tracker -- report
```

**Instalado en la máquina**: compila en modo release y copia el binario a tu `PATH`:

```sh
cargo build --release -p money-tracker
sudo cp "$(git rev-parse --show-toplevel)/target/release/money-tracker" /usr/local/bin/
money-tracker db remote login --url https://<ref>.supabase.co --key sb_publishable_…
money-tracker db remote status
```

El binario siempre queda en `target/` de la **raíz** del repo, aunque compiles desde `gui/` u otra subcarpeta (así
funcionan los workspaces de Cargo). Por eso el `cp` usa `git rev-parse --show-toplevel`: funciona desde cualquier
carpeta del proyecto.

El login pide tu email y contraseña de Supabase y guarda la URL, la key y la sesión; después ya no hay que repetirlo.
`db remote status` debe mostrar la sesión activa y `Esquema: versión N (la app espera N)`.

### 1.6 GUI

**Desde el proyecto (modo desarrollo)**:

```sh
cd gui
pnpm install
pnpm tauri dev
```

**Instalado en la máquina**, con un instalador nativo:

```sh
cd gui
pnpm install
pnpm tauri build   # el instalador queda en gui/src-tauri/target/release/bundle/<plataforma>/
```

(`.dmg`/`.app` en macOS, `.msi`/`.exe` en Windows, `.deb`/`.AppImage` en Linux; por ahora la app se llama `gui`, por
el `productName` de `gui/src-tauri/tauri.conf.json`).

Si todavía no configuraste Supabase, la GUI abre en la pantalla **Conexión**: ahí pegas la URL y la key e inicias
sesión. Usa los mismos archivos que el CLI, así que basta con conectarse en uno de los dos. CLI y GUI pueden correr al
mismo tiempo.

**Solo si vas a modificar el código de la GUI**: si cambias o agregas un modelo de `money_core` que usa el frontend,
regenera los tipos de TypeScript:

```sh
cargo test -p money_core --features ts-rs   # reescribe gui/src/bindings/*.ts
```

### 1.7 Actualizar o reinstalar (instalación limpia)

`cargo build` solo actualiza `target/`. **Nunca toca** lo que ya copiaste a `/usr/local/bin` ni la GUI que instalaste:
si haces `git pull`, cambias de rama o vuelves al proyecto después de un tiempo, lo instalado sigue siendo la versión
vieja. Síntoma típico: un comando que el README documenta "no existe" (`error: unrecognized subcommand`).

```sh
cd ~/Projects/money-tracker
git status                              # confirma en qué rama estás: eso es lo que vas a instalar
git pull

# ¿el esquema cambió? revisa si hay archivos nuevos en supabase/sql/: haz una copia previa (abajo) y
# aplícalos en el SQL Editor ANTES de instalar la app nueva (supabase/README.md, sección 3)

# CLI
cargo build --release -p money-tracker
sudo cp "$(git rev-parse --show-toplevel)/target/release/money-tracker" /usr/local/bin/

# GUI (solo si la instalaste como app)
cd gui && pnpm install && pnpm tauri build && cd ..

# verificar
money-tracker db remote status          # sesión activa y "Esquema: versión N (la app espera N)"
```

Reinstalar **no toca tus datos**: viven en Supabase, y la configuración en `~/.money-tracker/`, fuera del repo.

Si la app y el esquema no coinciden, la app se niega a trabajar y dice qué hacer: "aplica `supabase/sql/000N_…`"
(esquema atrasado) o "actualiza la app" (app atrasada).

**Copia previa antes de aplicar un archivo de esquema.** Cada archivo corre completo o no aplica nada, pero aun así
conviene tener tus datos a mano. En el SQL Editor, corre esta consulta de solo lectura, dale **Export → Copy as JSON**
y guárdala con `pbpaste > ~/money-tracker-copia-previa-AAAA-MM-DD.json`:

```sql
select jsonb_build_object(
  'concepts', (select jsonb_agg(c) from public.concepts c),
  'accounts', (select jsonb_agg(a) from public.accounts a),
  'entries',  (select jsonb_agg(e) from public.entries  e),
  'budgets',  (select jsonb_agg(b) from public.budgets  b),
  'config',   (select jsonb_agg(k) from public.config   k));
```

(Si la app ya está al día, `money-tracker db backup` también sirve, y además se puede restaurar.)

**Si vienes de una versión anterior a la 002** (la que usaba una base local `data.db` y un espejo):

1. Haz la copia previa de arriba.
2. Aplica `supabase/sql/0002_schema_version.sql` en el SQL Editor; `select * from public.schema_version;` → `2`.
3. Agrega `token_storage = "file"` a `~/.money-tracker/config.toml` (opcional, ver [apéndice](#apéndice-problemas-comunes)).
4. Instala el CLI nuevo (arriba) e inicia sesión **una vez más** con `money-tracker db remote login`: la sesión ahora se
   guarda por proyecto, así que la anterior ya no se usa.
5. `money-tracker db remote status` → sesión activa y esquema 2 de 2; luego `money-tracker db backup`.
6. Reinstala o vuelve a correr la GUI; usa la misma sesión que el CLI.
7. Cuando quieras, borra `~/.money-tracker/data.db` (y `data.db-wal`/`-shm`): la app ya no los usa.

---

## 2. Core — cómo funciona

`money_core` es el modelo puro: modelos, reglas contables y servicios. No imprime nada, no pregunta nada, no parsea
argumentos; el CLI y la GUI son dos "manejadores" delgados sobre la misma librería (`cargo tree -p money_core` nunca
debe mostrar `clap`/`dialoguer`/`tabled`/`tauri`).

### El modelo de datos

Todo movimiento de dinero es una **entrada** (`entry`) entre **cuentas**, o a través del borde del sistema. Dos tablas
cargan con todo el modelo:

```
accounts(id, name UNIQUE, kind CHECK IN ('spending','emergency','target','credit'),
         target_amount, credit_limit, liquid, archived)

entries(id, date, kind CHECK IN ('income','expense','transfer','opening'),
        amount CHECK(amount > 0),           -- siempre positivo; kind da la dirección
        from_account_id, to_account_id,     -- exactamente una de estas formas:
        concept, subconcept, description)   --   income/opening: solo to · expense: solo from · transfer: ambos
```

**Tipos de entrada**:

| Tipo | Significado |
|---|---|
| `income` | Entra dinero al sistema (nómina, vales, etc.) |
| `expense` | Sale dinero del sistema (un gasto) |
| `transfer` | Se mueve entre dos cuentas — **no es gasto ni ingreso** |
| `opening` | Saldo inicial cargado con `setup` — no cuenta como ingreso |

**Tipos de cuenta** (`kind`):

- **`spending`**: efectivo, débito, vales. Puede marcarse `--restricted` (no líquida): el aporte automático al fondo
  de emergencia nunca se dispara sobre ella (ej. vales de despensa, que no se pueden mover a ahorro).
- **`emergency`**: el fondo de emergencia. Solo puede haber **una** activa. Recibe automáticamente un
  `emergency_pct`% (10% por defecto) de cada `income` que caiga en una cuenta líquida.
- **`target`**: un bucket de ahorro, con o sin meta (`target_amount` es opcional, para un bucket abierto tipo
  "Patrimonio").
- **`credit`**: una tarjeta de crédito. Su saldo va en negativo = deuda. Pagarla es una `transfer`, nunca un `expense`
  nuevo (evita contarlo dos veces).

**Los saldos se derivan, nunca se almacenan**: el saldo de una cuenta es la suma de sus movimientos. No existe
ningún `UPDATE ... saldo` en el código, así que el saldo no puede desincronizarse del historial.

### Las reglas contables

Viven una sola vez, en `money_core/src/rules.rs`, y los servicios las aplican **antes** de escribir:

- un bucket (`target`/`emergency`) no puede quedar en negativo;
- un cargo a la tarjeta no puede pasar el disponible (límite − deuda);
- solo una cuenta de emergencia activa; `target_amount` solo en `target`; `credit_limit` solo en `credit`;
- presupuesto positivo; tipo de concepto válido; al editar, solo se mueve el lado de cuenta que aplica al tipo;
- el reparto al fondo de emergencia de cada ingreso.

Supabase repite las más importantes en su esquema como **segunda defensa** (ver
[`supabase/README.md`](supabase/README.md)).

### Los dos números del reporte

Como un gasto se puede pagar desde una cuenta de gasto, una tarjeta de crédito o directo de un bucket de ahorro,
"cuánto gasté este mes" tiene dos respuestas honestas y distintas; el `report` muestra ambas:

- **Gasto del mes (devengado)**: todo lo que consumiste este mes, sin importar la fuente. Contra esto compara el
  presupuesto.
- **Salida real de efectivo**: lo que realmente salió de tus cuentas de gasto, incluyendo pagos de tarjeta hechos ese
  mes (que no financian nada nuevo, solo liquidan un cargo de un mes anterior).

`report --detail` desglosa el devengado en pagado-con-flujo / financiado-con-ahorro / a-crédito.

### Versión de esquema

Cada cambio al esquema de Supabase es un archivo numerado en `supabase/sql/` que se verifica a sí mismo y sube
`schema_version`. La app conoce la versión que espera (`money_core::schema::EXPECTED_SCHEMA_VERSION`) y la compara al
conectar. Proceso completo: [`supabase/README.md`](supabase/README.md), sección 3.

### Arquitectura interna (por si vas a tocar el código)

```
money_core/src/
  rules.rs        # reglas contables puras (sin I/O)
  schema.rs       # EXPECTED_SCHEMA_VERSION + prueba contra supabase/sql/
  period.rs       # Period ("YYYY-MM") — única fuente de verdad para fechas
  auth.rs         # login/renovación con Supabase Auth; sesión en llavero o archivo
  settings.rs     # config.toml y rutas de ~/.money-tracker
  storage/        # trait LedgerBackend + SupabaseBackend + MemoryBackend (solo pruebas) + ledger.rs (saldos)
  models/         # cuentas, movimientos, presupuestos, conceptos, config, estado del libro, respaldo
  services/       # account, entry (único escritor de movimientos), budget, concept, report, setup, backup
tests/scenarios.rs  # pruebas de caja negra por la API pública, sobre MemoryBackend
```

`cli/` (clap + dialoguer) y `gui/src-tauri/` (comandos Tauri) son manejadores delgados sobre esos `services/`; ninguno
repite reglas.

---

## 3. CLI — set de instrucciones

Todos los comandos que registran dinero resuelven uno de tres modos según qué banderas pases
(`cli/src/commands/helpers.rs::PromptMode`):

- **`Wizard`**: sin argumentos, o con `-i`/`--interactive`: pregunta todo, incluyendo opcionales.
- **`Fill`**: algunos argumentos: solo pregunta por los campos requeridos que falten.
- **`Strict`**: con `--yes`: nunca pregunta, falla si falta algo requerido. Es el modo para scripts.

### Referencia de comandos

| Comando | Subcomando | Qué hace |
|---|---|---|
| `add` | — | Registrar un gasto |
| `income` | — | Registrar un ingreso (aparta `emergency_pct`% al fondo de emergencia) |
| `transfer` | — | Mover dinero entre dos cuentas (pago de tarjeta, retiro de cajero…) |
| `bucket` | `deposit` | Depositar a un bucket de ahorro |
| | `withdraw` | Retirar de un bucket (**no es un gasto**) |
| `account` | `add` | Crear una cuenta |
| | `list` | Listar cuentas con su saldo (`--all` incluye archivadas) |
| | `archive` | Archivar (rechaza si el saldo no es cero, salvo `--force`) |
| | `reconcile` | Cuadrar el sobre de efectivo contra lo que contaste |
| `entry` | `list` | Listar movimientos, con filtros |
| | `edit` | Corregir un movimiento (monto, concepto, cuenta, fecha…) |
| | `rm` | Borrar un movimiento por id |
| `concept` | `list` / `add` | Gestionar conceptos |
| `budget` | `set` / `show` / `rm` | Presupuesto mensual (informativo, nunca bloquea) |
| `report` | — | Reporte del mes, ver [los dos números](#los-dos-números-del-reporte) |
| `config` | `list` / `get` / `set` | Reglas del libro contable ([claves](#claves-de-configuración)) |
| `setup` | — | Cargar saldos iniciales en un libro nuevo |
| `db` | `backup` | Respaldo del libro contable ([sección 5](#5-respaldo-y-restauración)) |
| | `remote login` | Conectar con tu proyecto de Supabase |
| | `remote logout` | Olvidar la sesión (conserva URL y key) |
| | `remote status` | Conexión, sesión, revisión, versión de esquema y último respaldo |

Sintaxis (`[...]` es opcional; cualquier comando acepta `--help`):

```
add <MONTO> <CONCEPTO> [--from CUENTA] [-s SUBCONCEPTO] [-d DESCRIPCION] [-D FECHA] [--new-concept]
income <MONTO> <CONCEPTO> [--to CUENTA] [-d DESCRIPCION] [-D FECHA] [--no-emergency] [--new-concept]
transfer -a MONTO --from CUENTA --to CUENTA [-d DESCRIPCION] [-D FECHA]
bucket deposit  -b BUCKET -a MONTO [--from CUENTA] [-D FECHA]
bucket withdraw -b BUCKET -a MONTO [--to CUENTA] [-D FECHA]
account add <NOMBRE> --kind <spending|emergency|target|credit> [--target N] [--limit N] [--restricted]
account list [--all]
account archive <NOMBRE> [--force]
account reconcile <NOMBRE> --actual N [-c CONCEPTO] [-D FECHA]
entry list [-p PERIODO] [-c CONCEPTO] [--account NOMBRE] [--kind K] [-n LIMITE]
entry edit <ID> [-a MONTO] [-c CONCEPTO] [-s SUBCONCEPTO] [-d DESCRIPCION] [-D FECHA] [--from CUENTA] [--to CUENTA]
entry rm <ID>
concept list
concept add <NOMBRE> [-t TIPO]
budget set -c CONCEPTO -l LIMITE [-p PERIODO]
budget show [-p PERIODO]
budget rm -c CONCEPTO [-p PERIODO]
report [-p PERIODO] [--detail]
config list
config get <CLAVE>
config set <CLAVE> <VALOR>
setup [--account NOMBRE=MONTO ...] [-D FECHA] [--force]
db backup [-o CARPETA_O_ARCHIVO]
db remote login [EMAIL] [--url URL] [--key KEY]
db remote logout
db remote status
```

`-D FECHA` es `YYYY-MM-DD` y `-p PERIODO` es `YYYY-MM`. Sin `-p`, `report` usa el mes actual y `entry list` muestra
todos los movimientos. `add`, `income` y `transfer` también aceptan `-i` (wizard completo) y `--yes` (nunca pregunta).

Después de cada comando exitoso, si el último respaldo tiene más de 7 días, el CLI hace uno solo y lo avisa en una
línea ([sección 5](#5-respaldo-y-restauración)).

### Claves de configuración

`config set <clave> <valor>` (o **Ajustes** en la GUI). Viven en Supabase, así que son las mismas en todas tus
máquinas:

| Clave | Uso | Default |
|---|---|---|
| `emergency_pct` | % de cada `income` líquido que se aparta al fondo de emergencia | `10` |
| `default_account` | Cuenta usada por `add --from` si se omite | — |
| `income_account` | Cuenta usada por `income --to` si se omite | — |
| `cash_concept` | Concepto usado por `account reconcile` para el sobrante/faltante | — |
| `baseline_monthly_expense` | Gasto mensual de referencia para "Meses de colchón" mientras no hay meses registrados | — |

---

## 4. GUI — vistas y funcionalidades

La GUI (Tauri v2 + React) es un **segundo manejador** sobre el mismo `money_core`: los mismos datos, la misma lógica,
paridad de operaciones con el CLI.

![Dashboard de money-tracker](docs/screenshot-dashboard.png)

Cada pestaña de la barra superior es una vista (`gui/src/routes/`):

| Vista | Qué muestra / permite | Equivalente en CLI |
|---|---|---|
| **Dashboard** | Reporte del mes, gastos por concepto, saldos, patrimonio neto | `report` |
| **Registrar** | Gasto, ingreso (con aviso de cuánto va al fondo) y transferencia | `add`, `income`, `transfer` |
| **Cuentas** | Crear y listar cuentas, depositar/retirar de buckets, cuadrar efectivo | `account *`, `bucket *` |
| **Movimientos** | Listar con filtros, editar o borrar un movimiento | `entry list/edit/rm` |
| **Presupuestos** | Crear/ver presupuestos, con donut de % consumido | `budget *` |
| **Ajustes** | Reglas del libro, conceptos, conexión (sesión, revisión, esquema) y respaldo | `config *`, `concept *`, `db *` |

Y dos pantallas que reemplazan a las pestañas cuando hace falta:

| Pantalla | Cuándo aparece | Equivalente en CLI |
|---|---|---|
| **Conexión** | Sin Supabase configurado, sin sesión, o con un esquema de otra versión | `db remote login` |
| **SetupWizard** | Conectado, pero todavía sin ninguna cuenta: crear cuentas y saldos iniciales | `account add` + `setup` |

**Cómo se mantiene al día**: no hay router ni librería de queries. `hooks/useApi.ts` mantiene un contador de
"revisión" global que sube después de cada cambio, y todo componente que lee datos los vuelve a pedir cuando cambia.
`hooks/useSync.ts` pregunta a Supabase cada ~30 s su revisión (`ledger_status`) y recarga solo si cambió: así ves lo
que registraste desde otra máquina, incluidos los borrados. También recarga al volver a la ventana y con el botón
**Actualizar**.

Al abrir, si el último respaldo tiene más de 7 días, la GUI hace uno sola y lo avisa arriba.

El backend Tauri (`gui/src-tauri/src/commands/`) tiene un archivo por grupo de comandos, cada uno una llamada a un
servicio de `money_core`. `AppState` guarda la conexión a Supabase, que se abre al primer uso; `ApiError` traduce los
errores para el frontend (`not_configured`, `auth_needed`, `schema_mismatch`… deciden si se muestra Conexión).

---

## 5. Respaldo y restauración

Un respaldo es un archivo `.sql` con **todo** tu libro contable (cuentas, incluidas las archivadas, movimientos,
presupuestos, conceptos y configuración), tomado en un solo instante.

**Hacer un respaldo**:

```sh
money-tracker db backup                    # → ~/.money-tracker/backups/money-tracker-AAAAMMDD-HHMMSS.sql
money-tracker db backup -o ~/Dropbox/mt/   # otra carpeta (o un archivo nuevo)
```

O **Ajustes → Respaldo → Respaldar ahora** en la GUI. Nunca sobrescribe un archivo existente y se crea legible solo
por ti (`0600`). Guárdalo también fuera de la laptop (Dropbox, iCloud, un USB): el respaldo protege contra perder el
proyecto de Supabase, no contra perder la laptop.

**Automático**: si el último respaldo tiene más de 7 días, se hace uno solo al terminar un comando del CLI o al abrir
la GUI. Si falla (sin internet, por ejemplo) solo avisa, y lo reintenta la próxima vez. `db remote status` y
**Ajustes** muestran la fecha del último.

**Restaurar** (si pierdes tu proyecto de Supabase):

1. Crea un proyecto nuevo y aplica todos los archivos de `supabase/sql/` (sección 1.2).
2. Crea tu usuario en **Authentication → Users**. Si usas otro email que el del respaldo, cámbialo en la línea
   marcada con `RESTAURAR COMO`.
3. Pega el respaldo completo en el **SQL Editor** y dale Run.
4. Conecta la app al proyecto nuevo: `money-tracker db remote login --url … --key …`.

El respaldo corre completo o no aplica nada, y se niega a correr si el proyecto no está vacío o es de otra versión de
esquema. Detalle: [`supabase/README.md`](supabase/README.md), sección 4.

---

## 6. Ejemplos

```sh
# Crear las cuentas (una sola vez)
money-tracker account add efectivo --kind spending
money-tracker account add debito --kind spending
money-tracker account add vales --kind spending --restricted
money-tracker account add "Fondo de emergencia" --kind emergency
money-tracker account add Vacaciones --kind target --target 50000
money-tracker account add tdc --kind credit --limit 30000
money-tracker config set default_account debito

# Cargar saldos iniciales
money-tracker setup --account "Fondo de emergencia"=35000 --account debito=18000 -D 2026-08-01

# Registrar un gasto (usa default_account si se omite --from)
money-tracker add 350 Alimentos

# Gasto pagado con tarjeta
money-tracker add 1800 Discrecional --from tdc

# Pagar la tarjeta el mes siguiente
money-tracker transfer -a 1800 --from debito --to tdc

# Registrar un ingreso (aporta automáticamente al fondo de emergencia)
money-tracker income 24000 Nomina

# Vales de despensa: no dispara aporte a emergencia (cuenta restringida)
money-tracker income 2400 "Vales de Despensa" --to vales

# El sobre de efectivo: retirar y, a fin de mes, cuadrar lo que quedó
money-tracker transfer -a 1000 --from debito --to efectivo
money-tracker account reconcile efectivo --actual 150

# Depositar / retirar de un bucket de ahorro
money-tracker bucket deposit -b Vacaciones -a 2000 --from debito
money-tracker bucket withdraw -b "Fondo de emergencia" -a 4200 --to debito

# Gastar directo de un bucket, sin retirar primero
money-tracker add 4200 Servicios --from "Fondo de emergencia"

# Presupuesto (informativo)
money-tracker budget set -c Alimentos -l 2500 -p 2026-08
money-tracker budget show -p 2026-08

# Ver el reporte del mes
money-tracker report -p 2026-08 --detail

# Ver todas las cuentas
money-tracker account list
```

### Escenario completo de un mes

Flujo típico, en las propias palabras con las que se pensó el diseño:

> 1. yo recibo mi ingreso, se separa su porcentaje al fondo de emergencia
> 2. si tengo algún bucket, le destino parte de mi ingreso a discreción
> 3. voy a ir al cajero a hacer un retiro de efectivo para mi sobre
> 4. uso la tarjeta de débito y/o la de vales para los gastos
> 5. por lo general, los gastos en efectivo serán discrecionales, pero puede que no siempre — quizá en
>    la gasolinera no acepten tarjeta, o salió alguna reparación imprevista (ej. se ponchó una llanta)
> 6. si la junté lana para lo que tenía destinado cierto bucket, hago un retiro del bucket y lo gasto
>    (juntaba para unos tenis y voy y los compro; juntaba para la reparación del carro y retiro en
>    efectivo para pagar)
> 7. si surge una emergencia aplico el punto anterior pero para el fondo de emergencia
> 8. de mi remanente hago una "aportación voluntaria" a mi fondo de emergencia para "reponerlo"

Comando por comando:

**1. Ingreso con reparto automático al fondo de emergencia**

```sh
money-tracker income 20000 Nomina
```

Si `income_account` (default `debito`) es líquida y existe un `Fondo de emergencia`, la app aparta
sola el `emergency_pct`% — no hay paso manual.

**2. Destinar parte del ingreso a un bucket**

```sh
money-tracker account add Tenis --kind target --target 2000   # una sola vez
money-tracker bucket deposit -b Tenis -a 500 --from debito
```

Es una transferencia: no mueve "Gasto del mes".

**3. Retiro de cajero para el sobre de efectivo**

```sh
money-tracker transfer -a 1000 --from debito --to efectivo
```

**4. Gastos con débito/vales — se registran al momento**

```sh
money-tracker add 350 Alimentos --from debito
money-tracker add 250 Alimentos --from vales
```

**5. Gasto en efectivo que sí puedes identificar (no siempre es discrecional)**

Si sabes en el momento que fue gasolina o una reparación, regístralo con su concepto real — no esperes
al cuadre de fin de mes:

```sh
money-tracker add 450 Extraordinario --from efectivo -d "llanta ponchada"
```

Solo lo que de verdad no puedas rastrear cae en `account reconcile efectivo --actual N` al cierre del
mes, y ese sobrante cae por default en `Discrecional` (tu `cash_concept`).

**6. Bucket completo → retiro y gasto**

Compra directa (tenis), sin pasar por efectivo:

```sh
money-tracker add 2000 Discrecional --from Tenis -d "tenis nuevos"
```

Reparación que necesitas pagar en efectivo — retiro y gasto son dos hechos distintos:

```sh
money-tracker bucket withdraw -b "Fondo Auto" -a 2500 --to efectivo
money-tracker add 2500 Extraordinario --from efectivo -d "pago reparación del carro"
```

**7. Emergencia real — mismo patrón que el 6, con el fondo de emergencia**

```sh
money-tracker bucket withdraw -b "Fondo de emergencia" -a 3000 --to debito
money-tracker add 3000 Extraordinario --from debito -d "gasto médico imprevisto"
```

**8. Aportación voluntaria para reponer el fondo**

```sh
money-tracker bucket deposit -b "Fondo de emergencia" -a 1500 --from debito
```

Es un depósito manual, no pasa por `income` — no vuelve a dispararse ningún % automático sobre dinero
que ya era tuyo.

### Salida del reporte

```
╔══════════════════════════════════════╗
║     REPORTE MENSUAL   2026-08        ║
╚══════════════════════════════════════╝

   Gasto del mes (devengado): $6650.00
    pagado con flujo del mes  4850.00
       financiado con ahorro  0.00
                   a crédito  1800.00
     Salida real de efectivo: $4850.00

             Ingreso del mes: $24000.00
      Flujo neto (devengado): $17350.00
            Aportes a ahorro: $2400.00

Gastos por concepto:
+--------------+----------+---------+------+----+
| Concepto     | Gastado  | Presup. | %    | #  |
+--------------+----------+---------+------+----+
| Discrecional | $1800.00 | —       | —    | 1  |
+--------------+----------+---------+------+----+
| Alimentos    | $350.00  | $2500   | 14%  | 1  |
+--------------+----------+---------+------+----+

Cuentas:
  Fondo de emergencia       $37400.00
  Vacaciones                $2000.00 / $50000.00 (4%)
  tdc                       $0.00 (deuda $1800.00 · disponible $28200.00)
  debito                    $15250.00
  efectivo                  $150.00

 Efectivo disponible: $15400.00
              Ahorro: $39400.00
    Deuda de tarjeta: $1800.00
     Patrimonio neto: $53000.00
```

---

## Apéndice: problemas comunes

**"unrecognized subcommand" o un comando que el README documenta y "no existe"**: tu binario instalado es anterior al
código. Reinstala ([1.7](#17-actualizar-o-reinstalar-instalación-limpia)). Para comparar:
`money-tracker db --help` (lo instalado) contra `cargo run -p money-tracker -- db --help` (el código actual).

**"El esquema de Supabase está en la versión X y esta app espera la Y"**: si dice "Aplica supabase/sql/…", pega ese
archivo en el SQL Editor. Si dice "Actualiza la app", reinstala desde la versión más reciente del repo.

**"Supabase no está configurado"**, **"No hay sesión activa"** o **"Session expired"**: `money-tracker db remote login
--url … --key …` (la primera vez) o `money-tracker db remote login` (si ya tienes URL y key). La sesión se renueva
sola; solo expira si cambias tu contraseña, cierras sesión en todos lados o la revocas en Supabase. En la GUI, si la
sesión expira con la app abierta, vuelve sola a la pantalla Conexión.

**macOS pide la contraseña de la laptop en cada uso**: pasa porque el binario se firma distinto en cada compilación y
el llavero no lo reconoce. Agrega `token_storage = "file"` a `~/.money-tracker/config.toml` e inicia sesión una vez
más: la sesión pasa a un archivo legible solo por ti y la app no vuelve a tocar el llavero.

**Archivos viejos en `~/.money-tracker/`**: `data.db` (y `data.db-wal`/`-shm`) son de la versión anterior, que usaba
una base SQLite local. La app ya no los lee ni los modifica; puedes borrarlos cuando quieras.

**Probar algo sin tocar tus datos reales**: levanta un Supabase local con Docker (`supabase start`) y apunta la app a
un `config.toml` aparte con `MONEY_TRACKER_CONFIG`. Pasos: [`supabase/README.md`](supabase/README.md), sección 5.

# Instalación y puesta en marcha

Todo lo necesario para dejar money-tracker funcionando, desde cero o al volver después de un tiempo. Qué es y cómo se
usa: [`README.md`](../README.md). Cómo está hecho por dentro: [`ARQUITECTURA.md`](ARQUITECTURA.md).

## Índice

1. [Prerrequisitos](#1-prerrequisitos)
2. [Crear tu proyecto de Supabase](#2-crear-tu-proyecto-de-supabase-una-sola-vez)
3. [Clonar y compilar](#3-clonar-y-compilar)
4. [Archivos de configuración](#4-archivos-de-configuración)
5. [CLI](#5-cli)
6. [GUI](#6-gui)
7. [Actualizar o reinstalar](#7-actualizar-o-reinstalar-instalación-limpia)
8. [Restaurar un respaldo](#8-restaurar-un-respaldo)
9. [Problemas comunes](#9-problemas-comunes)

## 1. Prerrequisitos

```sh
# Rust toolchain: compila core, CLI y el backend de la GUI
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Node.js + pnpm: solo para la GUI (no hace falta para el CLI)
corepack enable   # o: npm i -g pnpm
```

Y una cuenta en [supabase.com](https://supabase.com) (el plan gratuito alcanza).

---

## 2. Crear tu proyecto de Supabase (una sola vez)

1. En Supabase, **New Project**. Anota la **Project URL** (`https://<ref>.supabase.co`) y la **publishable key**
   (Project Settings → API, la `sb_publishable_…`). Nunca uses la `service_role` en la app.
2. En el **SQL Editor** del proyecto, pega y corre en orden cada archivo de `setup/sql/`: primero
   `0001_setup.sql`, luego `0002_schema_version.sql`, y así hasta el último.
3. En **Authentication → Users**, crea tu usuario (email y contraseña).

Detalle, qué crea cada archivo y cómo verificarlo: [`setup/README.md`](../setup/README.md).

---

## 3. Clonar y compilar

```sh
git clone <repo-url> money-tracker
cd money-tracker
cargo build --workspace   # compila money_core + cli + gui/src-tauri
cargo test --workspace    # deben pasar todas (no usan red ni base de datos)
```

---

## 4. Archivos de configuración

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
  (ver [problemas comunes](#9-problemas-comunes)).
- **`MONEY_TRACKER_CONFIG=/ruta/config.toml`** apunta la app a otro `config.toml` y mueve con él los demás archivos
  de la tabla. Sirve para probar contra un Supabase local sin tocar tu configuración real.
- **`MONEY_TRACKER_SUPABASE_URL`** y **`MONEY_TRACKER_SUPABASE_KEY`** ganan sobre lo que diga `config.toml`.
- Las **reglas del libro contable** (`emergency_pct`, `default_account`, …) no viven en un archivo sino en
  Supabase (tabla `config`); se cambian con `config set` o en **Ajustes**. Ver la
  [tabla de claves](../README.md#claves-de-configuración).

---

## 5. CLI

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

---

## 6. GUI

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

---

## 7. Actualizar o reinstalar (instalación limpia)

`cargo build` solo actualiza `target/`. **Nunca toca** lo que ya copiaste a `/usr/local/bin` ni la GUI que instalaste:
si haces `git pull`, cambias de rama o vuelves al proyecto después de un tiempo, lo instalado sigue siendo la versión
vieja. Síntoma típico: un comando que el README documenta "no existe" (`error: unrecognized subcommand`).

```sh
cd ~/Projects/money-tracker
git status                              # confirma en qué rama estás: eso es lo que vas a instalar
git pull

# ¿el esquema cambió? revisa si hay archivos nuevos en setup/sql/: haz una copia previa (abajo) y
# aplícalos en el SQL Editor ANTES de instalar la app nueva (setup/README.md, sección 3)

# CLI
cargo build --release -p money-tracker
sudo cp "$(git rev-parse --show-toplevel)/target/release/money-tracker" /usr/local/bin/

# GUI (solo si la instalaste como app)
cd gui && pnpm install && pnpm tauri build && cd ..

# verificar
money-tracker db remote status          # sesión activa y "Esquema: versión N (la app espera N)"
```

Reinstalar **no toca tus datos**: viven en Supabase, y la configuración en `~/.money-tracker/`, fuera del repo.

Si la app y el esquema no coinciden, la app se niega a trabajar y dice qué hacer: "aplica `setup/sql/000N_…`"
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
2. Aplica `setup/sql/0002_schema_version.sql` en el SQL Editor; `select * from public.schema_version;` → `2`.
3. Agrega `token_storage = "file"` a `~/.money-tracker/config.toml` (opcional, ver [problemas comunes](#9-problemas-comunes)).
4. Instala el CLI nuevo (arriba) e inicia sesión **una vez más** con `money-tracker db remote login`: la sesión ahora se
   guarda por proyecto, así que la anterior ya no se usa.
5. `money-tracker db remote status` → sesión activa y esquema 2 de 2; luego `money-tracker db backup`.
6. Reinstala o vuelve a correr la GUI; usa la misma sesión que el CLI.
7. Cuando quieras, borra `~/.money-tracker/data.db` (y `data.db-wal`/`-shm`): la app ya no los usa.

---

## 8. Restaurar un respaldo

Si pierdes tu proyecto de Supabase, tu libro se reconstruye desde el último respaldo (`~/.money-tracker/backups/`, o
donde lo hayas guardado; ver [respaldos en el README](../README.md#3-respaldos)):

1. Crea un proyecto nuevo y aplica todos los archivos de `setup/sql/` ([sección 2](#2-crear-tu-proyecto-de-supabase-una-sola-vez)).
   La versión de esquema debe ser la misma que dice el encabezado del respaldo.
2. Crea tu usuario en **Authentication → Users**. Si usas otro email que el del respaldo, cámbialo en la línea
   marcada con `RESTAURAR COMO`.
3. Pega el respaldo completo en el **SQL Editor** y dale Run.
4. Conecta la app al proyecto nuevo: `money-tracker db remote login --url … --key …`.

El respaldo corre completo o no aplica nada, y se niega a correr si el proyecto no está vacío o es de otra versión de
esquema. Ajusta las secuencias, así que la app puede seguir registrando de inmediato. Detalle:
[`setup/README.md`](../setup/README.md), sección 4.

---

## 9. Problemas comunes

**"unrecognized subcommand" o un comando que el README documenta y "no existe"**: tu binario instalado es anterior al
código. Reinstala ([sección 7](#7-actualizar-o-reinstalar-instalación-limpia)). Para comparar:
`money-tracker db --help` (lo instalado) contra `cargo run -p money-tracker -- db --help` (el código actual).

**"El esquema de Supabase está en la versión X y esta app espera la Y"**: si dice "Aplica setup/sql/…", pega ese
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
un `config.toml` aparte con `MONEY_TRACKER_CONFIG`. Pasos: [`setup/README.md`](../setup/README.md), sección 5.

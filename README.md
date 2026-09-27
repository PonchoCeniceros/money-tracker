<h1 align="left">
  <img src="https://github.com/PonchoCeniceros/money-tracker/blob/main/docs/icon.png" width="90" align="absmiddle">
  &nbsp;
  money-tracker
</h1>

Control de finanzas personales desde la terminal y/o una app de escritorio nativa. Cuentas (efectivo,
débito, vales, tarjeta de crédito), buckets de ahorro (fondo de emergencia + metas), presupuestos
informativos, y un reporte mensual que distingue "cuánto gasté" de "cuánto salió de mi bolsillo".

Tus datos viven en **tu propio proyecto de Supabase** (tiene plan gratuito): el CLI y la GUI leen y escriben
directo ahí, desde cualquier máquina. No hay ninguna base de datos local que se pueda desincronizar. Para no
depender solo de Supabase, la app hace **respaldos**: a mano cuando quieras y solos cada 7 días.

Reemplaza un dashboard de Excel que se había vuelto engorroso de mantener. El Excel
(`Dashboard_Financiero.xlsx`/`.ods`) queda como **referencia histórica de solo consulta**; no se importa.

## Documentación

| Documento | Para qué |
|---|---|
| Este README | Qué es, conceptos, uso del CLI y la GUI, respaldos y ejemplos |
| [`docs/INSTALACION.md`](docs/INSTALACION.md) | Instalar, configurar Supabase, actualizar, restaurar y problemas comunes |
| [`docs/ARQUITECTURA.md`](docs/ARQUITECTURA.md) | Cómo está hecho por dentro, para quien vaya a tocar el código |
| [`setup/README.md`](setup/README.md) | El esquema de Supabase: aplicarlo, cambiarlo y verificarlo |
| [`AGENTS.md`](AGENTS.md) | Guía de desarrollo (convenciones y comandos) |

¿Primera vez, o vuelves después de un tiempo? Empieza por [`docs/INSTALACION.md`](docs/INSTALACION.md).

---

## Índice

1. [Conceptos](#1-conceptos)
2. [CLI — set de instrucciones](#2-cli--set-de-instrucciones)
3. [Respaldos](#3-respaldos)
4. [GUI — vistas y funcionalidades](#4-gui--vistas-y-funcionalidades)
5. [Ejemplos](#5-ejemplos)

---

## 1. Conceptos

Todo movimiento de dinero es un **movimiento** (`entry`) entre **cuentas**, o a través del borde del sistema:

| Tipo de movimiento | Significado |
|---|---|
| `income` | Entra dinero al sistema (nómina, vales, etc.) |
| `expense` | Sale dinero del sistema (un gasto) |
| `transfer` | Se mueve entre dos cuentas — **no es gasto ni ingreso** |
| `opening` | Saldo inicial cargado con `setup` — no cuenta como ingreso |

Tipos de cuenta (`--kind` en `account add`):

- **`spending`**: efectivo, débito, vales. Con `--restricted` queda como no líquida: el aporte automático al fondo de
  emergencia nunca se dispara sobre ella (ej. vales de despensa, que no se pueden mover a ahorro).
- **`emergency`**: el fondo de emergencia. Solo puede haber **una** activa. Recibe automáticamente un `emergency_pct`%
  (10% por defecto) de cada `income` que caiga en una cuenta líquida.
- **`target`**: un bucket de ahorro, con o sin meta (`--target` es opcional, para un bucket abierto tipo "Patrimonio").
- **`credit`**: una tarjeta de crédito. Su saldo va en negativo = deuda. Pagarla es una `transfer`, nunca un gasto
  nuevo (evita contarlo dos veces).

Los **saldos no se guardan**: son la suma de los movimientos de cada cuenta, así que nunca se desincronizan y un saldo
inicial sigue ahí el mes siguiente.

La app **rechaza** lo que rompería el libro: sacar de un bucket más de lo que tiene, pasar el disponible de la tarjeta
(límite − deuda), una segunda cuenta de emergencia activa, montos de cero o negativos, o un presupuesto que no sea
positivo. Los presupuestos son **informativos**: nunca bloquean un gasto.

### Los dos números del reporte

Como un gasto se puede pagar desde una cuenta de gasto, una tarjeta de crédito o directo de un bucket de ahorro,
"cuánto gasté este mes" tiene dos respuestas honestas y distintas; el `report` muestra ambas:

- **Gasto del mes (devengado)**: todo lo que consumiste este mes, sin importar la fuente. Contra esto compara el
  presupuesto.
- **Salida real de efectivo**: lo que realmente salió de tus cuentas de gasto, incluyendo pagos de tarjeta hechos ese
  mes (que no financian nada nuevo, solo liquidan un cargo de un mes anterior).

`report --detail` desglosa el devengado en pagado-con-flujo / financiado-con-ahorro / a-crédito.

---

## 2. CLI — set de instrucciones

¿Todavía no lo tienes instalado o conectado? [`docs/INSTALACION.md`](docs/INSTALACION.md).

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
| `db` | `backup` | Respaldo del libro contable ([respaldos](#3-respaldos)) |
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
línea ([respaldos](#3-respaldos)).

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

## 3. Respaldos

Un respaldo es un archivo `.sql` con **todo** tu libro contable (cuentas, incluidas las archivadas, movimientos,
presupuestos, conceptos y configuración), tomado en un solo instante.

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

**Restaurar** (si pierdes tu proyecto de Supabase): proyecto nuevo, archivos de esquema y el respaldo pegado en el SQL
Editor. Pasos: [`docs/INSTALACION.md`, sección 8](docs/INSTALACION.md#8-restaurar-un-respaldo).

---

## 4. GUI — vistas y funcionalidades

La GUI (Tauri v2 + React) hace lo mismo que el CLI, sobre los mismos datos. Cómo abrirla o instalarla:
[`docs/INSTALACION.md`, sección 6](docs/INSTALACION.md#6-gui).

![Dashboard de money-tracker](docs/screenshot-dashboard.png)

Cada pestaña de la barra superior es una vista:

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
| **Conexión** | Sin Supabase configurado, sin sesión (o si expira con la app abierta), o con otro esquema | `db remote login` |
| **SetupWizard** | Conectado, pero todavía sin ninguna cuenta: crear cuentas y saldos iniciales | `account add` + `setup` |

La GUI se actualiza sola: cada ~30 s revisa si hubo cambios en Supabase (por ejemplo, algo que registraste desde el
CLI o desde otra máquina, incluidos los borrados) y recarga solo si los hubo. También recarga al volver a la ventana y
con el botón **Actualizar**. Al abrir, si el último respaldo tiene más de 7 días, hace uno sola y lo avisa arriba.

---

## 5. Ejemplos

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
╭──────────────┬──────────┬──────────┬─────┬───╮
│ Concepto     │  Gastado │  Presup. │   % │ # │
├──────────────┼──────────┼──────────┼─────┼───┤
│ Discrecional │ $1800.00 │        — │   — │ 1 │
│ Alimentos    │  $350.00 │ $2500.00 │ 14% │ 1 │
╰──────────────┴──────────┴──────────┴─────┴───╯

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

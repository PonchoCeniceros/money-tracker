# money-tracker

Control de finanzas personales desde la terminal. Ingresos, gastos, buckets de ahorro, presupuestos e importacion desde ODS de Google Sheets.

## Uso

```
money-tracker <comando> [opciones]
```

### Compilar e instalar

```sh
cargo build --release
cp target/release/money-tracker /usr/local/bin/
```

O directamente con cargo: `cargo run -- <comando> [opciones]`

### Comandos

| Comando | Subcomando | Descripcion | Banderas |
|---------|-----------|-------------|----------|
| `add` | — | Registrar un gasto | `-a` amount, `-c` concept, `-s` subconcept, `-t` tipo, `-d` description, `-m` month, `-y` year |
| `income` | — | Registrar un ingreso (opcionalmente asigna al fondo de emergencia) | `-a` amount, `-c` concept, `-d` description, `-m` month, `-y` year |
| `bucket` | `create` | Crear un bucket de ahorro | `name` (positional), `-t` target |
| | `list` | Listar buckets con saldos | — |
| | `deposit` | Depositar a un bucket | `-b` bucket, `-a` amount |
| | `withdraw` | Retirar de un bucket | `-b` bucket, `-a` amount |
| `concept` | `list` | Listar todos los conceptos | — |
| | `add` | Agregar un concepto nuevo | `name` (positional), `-t` type (expense/income/both) |
| `budget` | `show` | Mostrar presupuestos del mes | `-m` month, `-y` year |
| | `set` | Establecer presupuesto mensual | `-c` concept, `-l` limit, `-m` month, `-y` year |
| `report` | — | Mostrar reporte mensual de ingresos/gastos/flujo | `-m` month, `-y` year |
| `config` | `list` | Listar toda la configuracion | — |
| | `get` | Obtener un valor de config | `key` |
| | `set` | Establecer un valor de config | `key` `value` |
| `import` | — | Importar transacciones desde un archivo .ods | `path` (positional) |
| `init-balances` | — | Cargar saldos iniciales de flujo y buckets en DB nueva | `-f` flujo |

Todos los comandos aceptan banderas para uso no interactivo. Sin banderas, se muestra un prompt interactivo via dialoguer.

### Ejemplos

```sh
# Registrar un gasto
money-tracker add -a 350 -c Alimentos -t Credito

# Registrar ingreso con asignacion automatica a fondo de emergencia
money-tracker income -a 5000 -c Nomina

# Mostrar reporte de Junio 2026
money-tracker report -m 6 -y 2026

# Listar conceptos disponibles
money-tracker concept list

# Agregar un concepto nuevo
money-tracker concept add "Suscripciones" -t expense

# Crear un bucket con meta
money-tracker bucket create "Vacaciones" -t 50000

# Listar buckets
money-tracker bucket list

# Depositar a un bucket
money-tracker bucket deposit -b "Vacaciones" -a 2000

# Establecer presupuesto mensual
money-tracker budget set -c Alimentos -l 2500 -m 6 -y 2026

# Ver presupuestos del mes
money-tracker budget show -m 6 -y 2026

# Importar desde ODS
money-tracker import Dashboard_Financiero.ods

# Inicializar DB nueva con saldos
money-tracker init-balances -f 50000

# Ver configuracion
money-tracker config list
money-tracker config get emergency_pct
money-tracker config set emergency_pct 15
```

### Salida del reporte

```
╔══════════════════════════════════════╗
║     REPORTE MENSUAL   6/2026         ║
╚══════════════════════════════════════╝

          Total Ingresos: $24711.08
           Total Gastos: $9486.84
           Flujo Neto: $15224.24
    Aportaciones Buckets: $0.00
     Retiros de Buckets: $0.00
  Flujo (disponible): $15224.24

Gastos por Concepto:
+--------------+----------+--------+---+----+
| Concepto     | Gastado  | Presup. | % | #  |
+--------------+----------+--------+---+----+
| Alimentos    | $3308.84 | —       | — | 13 |
+--------------+----------+--------+---+----+
| Discrecional | $3265.20 | —       | — | 38 |
+--------------+----------+--------+---+----+
| Servicios    | $1279.80 | —       | — | 8  |
+--------------+----------+--------+---+----+
| Transporte   | $1150.00 | —       | — | 4  |
+--------------+----------+--------+---+----+
| Sandbox Inv  | $483.00  | —       | — | 3  |
+--------------+----------+--------+---+----+

Buckets:
  Sin buckets aun.

Tasa fondo de emergencia: 10% del ingreso
```

## Importacion ODS

Importa transacciones desde un archivo `.ods` exportado de Google Sheets. El parser escanea cada hoja buscando grupos de celdas (monto, fecha, concepto) y maneja:

- **Gastos regulares** — montos negativos se almacenan tal cual
- **Ingresos** — Nomina, Extraordinario, Despensa, Saldo inicial se almacenan como positivos
- **Movimientos de fondos** — Voluntario, Patronal, Ahorro Patronal se marcan como `tipo = Fondo` (excluidos de totales de ingresos/gastos)
- **Subconcepto/tipo/descripcion** — se capturan de celdas adyacentes cuando existen

## Estructura del proyecto

```
Cargo.toml          # workspace root (money_core + cli)
money_core/         # libreria: modelos, servicios, SQLite
  src/
    db.rs           # conexion, migraciones, seed
    models/         # Transaction, Bucket, Concept, Budget, Config
    services/       # transaction, bucket, report, import, init
cli/                # binario: clap + dialoguer
  src/
    main.rs         # entrypoint, comandos
    commands/       # add, income, bucket, budget, concept, config, import, init_balances, report
Dashboard_Financiero.ods  # datos fuente para importacion ODS
```

## Base de datos

- **Ubicacion**: `~/.money-tracker/data.db`
- Se crea automaticamente al ejecutar cualquier comando
- Los datos importados persisten ahi, no en el codigo
- Para resetear: `rm -f ~/.money-tracker/data.db`

## Arquitectura

Workspace con dos crates:

- **`money_core`** — libreria con SQLite (via rusqlite), modelos, servicios (transacciones, buckets, presupuestos, reportes, importacion)
- **`cli`** — binario con comandos clap + prompts interactivos dialoguer

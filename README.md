# money-tracker

Control de finanzas personales desde la terminal. Cuentas (efectivo, débito, vales, tarjeta de
crédito), buckets de ahorro (fondo de emergencia + metas), presupuestos informativos.

Reemplaza un dashboard de Excel que se había vuelto engorroso de mantener. El Excel
(`Dashboard_Financiero.xlsx`) queda como **referencia histórica de solo consulta** — ya no se importa
nada de él; una base de datos nueva arranca vacía y se carga con `setup`.

## Uso

```
money-tracker <comando> [opciones]
```

### Compilar e instalar

```sh
cargo build --release
sudo cp target/release/money-tracker /usr/local/bin/
```

O directamente con cargo: `cargo run -- <comando> [opciones]`

## El modelo

Todo movimiento de dinero es una **entrada** entre **cuentas**, o a través del borde del sistema:

| Tipo de entrada | Significado |
|---|---|
| `income` | Entra dinero al sistema (nómina, vales, etc.) |
| `expense` | Sale dinero del sistema (un gasto) |
| `transfer` | Se mueve entre dos cuentas — **no es gasto ni ingreso** |
| `opening` | Saldo inicial cargado con `setup` — no cuenta como ingreso |

Las cuentas tienen un **tipo** (`kind`):

- **`spending`** — efectivo, débito, vales. Los vales pueden marcarse `--restricted`: el aporte
  automático al fondo de emergencia nunca se dispara sobre una cuenta restringida.
- **`emergency`** — el fondo de emergencia. Solo puede haber uno activo. Recibe un % fijo
  (`emergency_pct`, default 10%) de cada ingreso que caiga en una cuenta líquida.
- **`target`** — un bucket de ahorro con meta (Vacaciones, etc). Los que quieras.
- **`credit`** — una tarjeta de crédito. Su saldo va en negativo = deuda. Pagarla es una
  transferencia, no un gasto nuevo.

Los saldos de cuenta **se derivan** de la suma de entradas, nunca se almacenan — no pueden
desincronizarse, y arrastran de un mes a otro (por eso `setup` funciona: el saldo cargado sigue ahí
el mes siguiente).

### Los dos números del reporte

Con tarjeta de crédito o gastos pagados desde un bucket, "cuánto gasté este mes" tiene dos respuestas
honestas y distintas:

- **Gasto del mes (devengado)** — lo que consumiste, sin importar cómo lo pagaste. Contra esto compara
  el presupuesto.
- **Salida real de efectivo** — lo que realmente salió de tus cuentas de gasto, incluyendo pagos de
  tarjeta hechos ese mes (que no financian nada nuevo, solo liquidan un cargo de un mes anterior).

### Comandos

| Comando | Subcomando | Descripción |
|---------|-----------|-------------|
| `add` | — | Registrar un gasto: `add <MONTO> <CONCEPTO> [--from CUENTA]` |
| `income` | — | Registrar un ingreso: `income <MONTO> <CONCEPTO> [--to CUENTA] [--no-emergency]` |
| `transfer` | — | Mover dinero entre dos cuentas cualquiera (pago de tarjeta, retiro de cajero, ...) |
| `bucket` | `deposit` | Depositar a un bucket de ahorro |
| | `withdraw` | Retirar de un bucket (**no es un gasto** — avisa si ya lo gastaste) |
| `account` | `add` | Crear una cuenta: `--kind <spending\|emergency\|target\|credit>` |
| | `list` | Listar cuentas con saldo derivado |
| | `archive` | Archivar una cuenta (rechaza si el saldo no es cero, salvo `--force`) |
| | `reconcile` | Cuadrar el sobre de efectivo contra lo que realmente tenés |
| `entry` | `list` | Listar movimientos, filtrable por período/concepto/cuenta/tipo |
| | `rm` | Borrar un movimiento por id |
| `concept` | `list` / `add` | Gestionar conceptos |
| `budget` | `set` / `show` / `rm` | Presupuesto mensual — **solo informativo**, nunca bloquea |
| `report` | — | Reporte del mes: gasto devengado, salida de efectivo, saldos, presupuesto vs real |
| `config` | `list` / `get` / `set` | Configuración (`emergency_pct`, `default_account`, `income_account`, `cash_concept`) |
| `setup` | — | Cargar saldos iniciales en una base de datos nueva |
| `db` | `status` / `reset` | Inspeccionar o reiniciar el archivo de base de datos |

Todos los comandos que registran dinero aceptan banderas para uso no interactivo, o preguntan por
dialoguer si faltan datos. `-i/--interactive` fuerza el wizard completo; `--yes` nunca pregunta y
falla si falta algo requerido.

### Ejemplos

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

## Estructura del proyecto

```
Cargo.toml          # workspace root (money_core + cli)
money_core/         # libreria: modelos, servicios, SQLite — sin dependencias de UI
  src/
    db.rs           # esquema, migraciones, PRAGMA user_version, detección de esquema legacy
    period.rs       # Period ("YYYY-MM") y utilidades de fecha
    models/         # AccountKind/NewAccount/AccountBalance, EntryKind/NewEntry/Entry, Budget, Concept, Config
    services/       # account_service, entry_service, report_service, setup_service
  tests/scenarios.rs  # tests de integración vía la API pública
cli/                # binario: clap + dialoguer
  src/
    main.rs
    commands/       # add, income, transfer, bucket, account, entry, concept, budget, report, config, setup, db
Dashboard_Financiero.xlsx / .ods  # dashboard legado, solo consulta — ya no se importa
```

## Base de datos

- **Ubicación**: `~/.money-tracker/data.db` (o `MONEY_TRACKER_DB` para apuntar a otra ruta, útil para
  pruebas)
- Se crea automáticamente al ejecutar cualquier comando
- Una base de datos con el esquema anterior (`transactions`/`buckets`) se rechaza con un mensaje
  accionable — no hay migración automática. Ver `money-tracker db status` y `db reset --backup`.

## Arquitectura

Workspace con dos crates:

- **`money_core`** — el modelo: SQLite (rusqlite), cuentas, entradas, presupuestos, reportes. No
  imprime, no pregunta, no parsea argumentos — un consumidor (CLI, GUI) lo envuelve.
- **`cli`** — binario con comandos clap + prompts interactivos dialoguer. Un manejador delgado sobre
  `money_core`.

## GUI

Segundo manejador sobre el mismo `money_core`, con paridad completa de operaciones (dashboard,
registrar gasto/ingreso/transferencia, cuentas y buckets, cuadre de efectivo, presupuestos,
movimientos, ajustes, y el wizard de arranque si la base está vacía).

```sh
cd gui
pnpm install
pnpm tauri dev       # ventana nativa, contra ~/.money-tracker/data.db
```

CLI y GUI pueden correr al mismo tiempo contra el mismo archivo — la conexión abre en modo WAL.
Los tipos compartidos (`AccountBalance`, `Entry`, `MonthlyReport`, etc.) se generan desde
`money_core` con `ts-rs`:

```sh
cargo test -p money_core --features ts-rs   # regenera gui/src/bindings/*.ts
```

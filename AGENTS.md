# AGENTS.md — money-tracker

## Project structure

Rust workspace with two crates:
- `money_core/` — library: SQLite (rusqlite bundled, no external deps), models, services
- `cli/` — binary: clap derive commands + dialoguer interactive prompts

## Build, run & test

```sh
cargo build           # compila todo el workspace
cargo run -- <comando>  # ejecuta el binario (ej: cargo run -- report -m 6 -y 2026)
cargo test            # 4 unit tests en money_core
```

No lint/typecheck config beyond `cargo check`.

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

## Database

- Ubicacion: `~/.money-tracker/data.db`
- Se crea automaticamente al ejecutar cualquier comando
- Migraciones via `CREATE TABLE IF NOT EXISTS` + seed de conceptos/config en tablas vacias — sin sistema de versiones
- Borrar el archivo para resetear: `rm -f ~/.money-tracker/data.db`
- Los datos importados persisten en la DB, no en el codigo

## Key domain rules

- `tipo = "Fondo"` transactions (Voluntario, Patronal, Ahorro Patronal) are fund movements excluded from income/expense totals and report queries
- `flujo = total_income - total_expense - bucket_contributions + bucket_withdrawals`
- Emergency fund: `emergency_pct` config key (default 10%), auto-allocated on `income` command
- "Saldo inicial" is a seeded income concept, used by `init-balances`

## CLI commands

All accept flags (non-interactive) or omit flags (dialoguer prompts):
- `add`, `income`, `bucket`, `concept`, `budget`, `report`, `config`, `import`, `init-balances`

Examples:
```sh
money-tracker add -a 350 -c Alimentos -t Credito
money-tracker income -a 5000 -c Nomina
money-tracker report -m 6 -y 2026
money-tracker import Dashboard_Financiero.ods
money-tracker init-balances -f 50000
```

## ODS import

- Parses Google Sheets `.ods` export: scans for (amount, date, concept) cell groups per sheet
- Fund concepts (Voluntario, Patronal, Ahorro Patronal) → `tipo = "Fondo"`
- Raw-number cells (parser picking wrong column) are silently skipped

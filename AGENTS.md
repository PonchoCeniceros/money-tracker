# AGENTS.md — money-tracker

## Project structure

Rust workspace with three crates plus a frontend package:
- `money_core/` — library: models, accounting rules, services, and the Supabase store. Pure model —
  no printing, no prompting, no argument parsing, no presentation formatting. Depends on nothing
  CLI/GUI-specific (`cargo tree -p money_core` must never show `clap`/`dialoguer`/`tabled`/`tauri`).
- `cli/` — binary: clap derive commands + dialoguer interactive prompts. A thin handler over
  `money_core`.
- `gui/src-tauri/` (crate name `gui`) — Tauri v2 backend: thin `#[tauri::command]` wrappers, one per
  `money_core` service call, plus `AppState` (the Supabase backend, connected lazily on first use) and
  `ApiError` (`AppError` isn't `Serialize`, so every command returns `ApiError` instead). `gui/src/` —
  React + TS frontend, CSS Modules, no router/query library (IPC is local, so a global revision counter in
  `hooks/useApi.ts` triggers refetch after every mutation).
- `setup/` — everything you run once: the Supabase schema as numbered SQL files applied by hand in the SQL
  Editor (`setup/sql/`, schema files ONLY), `setup/tests/verify.sql`, the optional data-loading scripts
  (`setup/scripts/`) and a guide (`setup/README.md`).

**Supabase is the only store.** There is no local database, no local mode and no mirror (spec
`002-remove-mirror-backup`). Without `supabase_url` + key the app fails with `AppError::NotConfigured`.

## Build, run & test

```sh
cargo build --workspace          # compiles money_core + cli + gui/src-tauri
cargo test --workspace           # 82 tests, all offline: they run on storage::memory::MemoryBackend
cargo clippy --workspace --all-targets
cd gui && pnpm install && pnpm tauri dev   # runs the GUI as a native window
npx tsc --noEmit                            # type-check the frontend (run from gui/)
```

**Never run the app against the default config** (`~/.money-tracker/config.toml` points at the user's
production Supabase). A project hook (`.claude/hooks/guard-production.py`) blocks `money-tracker`,
`cargo run`, `tauri dev` and production `curl`s unless `MONEY_TRACKER_CONFIG` points elsewhere. For manual
verification use a local Supabase (`supabase start`, see `setup/README.md` §5) and
`MONEY_TRACKER_CONFIG=/tmp/mt-local/config.toml` with `token_storage = "file"`.

Configuration (`money_core::settings`, all under `config_dir()` = `~/.money-tracker/` or the parent of
`MONEY_TRACKER_CONFIG`):
- `config.toml`: `supabase_url`, `supabase_publishable_key`, `token_storage` (`"keychain"` default |
  `"file"`). `MONEY_TRACKER_SUPABASE_URL` / `MONEY_TRACKER_SUPABASE_KEY` override the file.
- `refresh_token` (0600): the session when `token_storage = "file"` (or when the keychain is unavailable).
  In keychain mode the entry is `money-tracker` / `supabase_refresh_token:<project-ref>`, one per project.
  In file mode no code path calls `keyring` (any access makes macOS prompt).
- `backups/` and `last-backup.toml`: backups and the record of the last one.

`storage::connect(&Settings)` is the only way to get a backend: it checks url/key, calls the
`ledger_status()` RPC (which also proves the session) and compares the schema version.

### Regenerating GUI type bindings

Models consumed by the GUI derive `ts_rs::TS` behind an opt-in `ts-rs` feature on `money_core`
(off by default so the CLI-only build doesn't pull in the codegen macro):

```sh
cargo test -p money_core --features ts-rs   # writes gui/src/bindings/*.ts
```

Two things to know if you add a new exported field: `ts-rs`'s `export_to` path is relative to
`<crate>/bindings/`, not the crate root — hence the `"../../gui/src/bindings/"` (not `"../gui/..."`)
in every `#[ts(export_to = ...)]` attribute. And `i64`/`Option<i64>` fields need an explicit
`#[ts(type = "number")]` / `#[ts(type = "number | null")]` override — ts-rs's default `i64 -> bigint`
mapping doesn't match what actually arrives over Tauri's `serde_json`-based IPC (a plain JS `number`).

## Project layout

```
Cargo.toml            # workspace root (money_core + cli + gui/src-tauri)
money_core/
  src/
    rules.rs            # accounting rules, pure: overdraft, credit limit, one emergency account,
                        # account shape, budget > 0, concept type, entry-update shape, emergency split
    schema.rs           # EXPECTED_SCHEMA_VERSION + check_schema; a test pins it to setup/sql/
    period.rs           # Period ("YYYY-MM") + today()/validate_date() — single source of truth for dates
    settings.rs         # Settings (config.toml + env), TokenStorage, config_dir/backups_dir/last_backup_path
    auth.rs             # SupabaseAuth (GoTrue): login/refresh, token in keychain (per project) or file
    storage/            # LedgerBackend trait + connect() (mod.rs), SupabaseBackend (remote.rs),
                        # MemoryBackend (memory.rs, cfg(test) or feature test-support), ledger.rs (balances)
    models/             # AccountKind, NewAccount, AccountBalance, EntryKind, NewEntry, Entry,
                        # ConceptSummary, Budget, Concept, Config, LedgerStatus, LedgerSnapshot, BackupInfo
    services/
      account_service.rs   # create (validates via rules)/list/archive/reconcile accounts
      entry_service.rs     # sole writer of entries: push_checked() validates every batch before writing;
                           # income/expense/transfer/opening + emergency split + split preview + update
      budget_service.rs    # set/list/remove budgets
      concept_service.rs   # list/add concepts
      report_service.rs    # monthly_report, net_worth, full_status
      setup_service.rs     # opening-balance seeding
      backup_service.rs    # SQL backup (render_sql), write, last-backup record, lazy 7-day auto backup
  tests/scenarios.rs   # black-box tests through the public API only, on MemoryBackend
cli/
  src/
    main.rs             # dispatch + auto backup after a successful command
    commands/           # add, income, transfer, bucket, account, entry, concept, budget, report,
                        # config, setup, db (backup, remote login|logout|status), helpers
gui/
  src/                 # React + TS frontend
    bindings/          # generated by ts-rs — do not hand-edit
    api/                # typed wrappers over Tauri's invoke(), one module per command group
    hooks/useApi.ts     # fetch-on-mount + global revision counter for refetch-after-mutation
    hooks/useSync.ts    # polls ledger_status every ~30 s; bumps revision only when the ledger revision moved
    routes/             # Connect, Dashboard, Register, Accounts, Movements, Budgets, Settings, SetupWizard
  src-tauri/
    src/
      state.rs          # AppState: Mutex<Option<Box<dyn LedgerBackend>>>, backend() connects lazily, reset()
      error.rs          # ApiError, From<AppError> (kinds not_configured / auth_needed / schema_mismatch gate Connect)
      commands/         # one file per command group; sync.rs = ledger_status/connection_info/login/logout,
                        # backup.rs = backup_create/backup_auto
setup/
  sql/0001_setup.sql, 0002_schema_version.sql   # applied in order in the SQL Editor; never edit an applied one
  tests/verify.sql                               # 14 checks of what the schema must reject; ends in ROLLBACK
  scripts/setup_inicial.sh, presupuesto.sh       # optional: load your own data through the CLI
  README.md
README.md            # what it is, concepts, CLI/GUI usage, backups, examples (user-facing)
docs/INSTALACION.md  # install, Supabase setup, config files, update/reinstall, restore, troubleshooting
docs/ARQUITECTURA.md # internals: crates, rules, storage, GUI internals, backups, tests
Dashboard_Financiero.xlsx / .ods   # LEGACY dashboard, read-only reference — no longer imported
```

## Data model

Everything is a movement between accounts, or across the system boundary. Two tables carry the whole
model (in Supabase, per user under RLS):

```
accounts(id, user_id, name UNIQUE per user, kind CHECK IN ('spending','emergency','target','credit'),
         target_amount, credit_limit, liquid, archived)

entries(id, user_id, date, kind CHECK IN ('income','expense','transfer','opening'),
        amount CHECK(amount > 0),           -- always positive; kind gives direction
        from_account_id, to_account_id,     -- exactly one of these shapes, enforced by CHECK:
        concept, subconcept, description)   --   income/opening: to only · expense: from only · transfer: both
```

Account balances are **derived**, never stored: `storage::ledger::derive_balances` sums entries per
account (and the `account_balances` view does the same in SQL). There is no stored balance anywhere, so
balances cannot desync from the ledger, and a seeded balance carries forward across months.

**Rules live in `money_core/src/rules.rs`** and the services apply them before any write; `MemoryBackend`
enforces only what a store guarantees (ids, uniqueness, foreign keys, atomic batches), never accounting
rules. Supabase repeats the key ones as a second line of defense (CHECKs, the partial unique index for one
active emergency account, and `apply_entries`, which locks the source account and checks overdraft,
credit limit, shape and ownership):
- At most one active `emergency` account.
- Only `target` accounts may have a `target_amount` — but it's optional even there, for an open-ended
  accumulation bucket (e.g. "Patrimonio"). `AccountBalance::progress_pct()` returns `None` when it's
  unset, and every caller (CLI, GUI) renders that as "—".
- `kind='transfer'` is the only path that can move money between two accounts without touching
  income/expense totals — this is what lets a credit-card payment, a bucket withdrawal, or an ATM
  cash-envelope withdrawal happen without being double-counted as spending.
- `kind='opening'` is shape-identical to `income` but structurally excluded from income totals — this
  is what a seeded starting balance uses, so `setup` never inflates the month it runs in.
- `accounts.liquid` gates the automatic emergency-fund split on `income`: an income landing in a
  non-liquid account (e.g. "vales de despensa") never triggers the split. Handlers ask
  `entry_service::emergency_split_preview` instead of restating this rule.

## Reporting

Because an expense can be paid from a spending account, a credit card, or straight out of a savings
bucket, "how much did I spend this month" has two different honest answers, both shown by `report`:

- **Gasto del mes (devengado)** — everything consumed this month, regardless of funding source. This
  is what budgets compare against.
- **Salida real de efectivo** — what actually left spending accounts, including any card payments made
  this month (which fund nothing new, they just settle a prior month's charge).

`report --detail` breaks the devengado figure down into paid-with-flow / funded-from-savings /
on-credit, and always shows savings contributions/withdrawals and card payments as separate lines
when nonzero.

## CLI commands

All commands take flags for scripted/non-interactive use, or fall back to dialoguer prompts. Every
command that registers money resolves one of three modes (`cli/src/commands/helpers.rs::PromptMode`):
`Wizard` (no args, or `-i`) prompts for everything including optionals; `Fill` (some args) prompts only
for missing required fields; `Strict` (`--yes`) never prompts, errors instead.

- `add <AMOUNT> <CONCEPT> [--from ACCOUNT]` — register an expense
- `income <AMOUNT> <CONCEPT> [--to ACCOUNT] [--no-emergency]` — register an income; auto-splits
  `emergency_pct`% into the emergency account unless the destination is non-liquid or `--no-emergency`
- `transfer -a AMOUNT --from ACCOUNT --to ACCOUNT` — move money between any two accounts
- `bucket deposit -b BUCKET -a AMOUNT [--from ACCOUNT]` / `bucket withdraw -b BUCKET -a AMOUNT [--to ACCOUNT]`
  — sugar over `transfer` for savings buckets; `withdraw` explicitly prints that it is NOT an expense
- `account add <NAME> --kind <spending|emergency|target|credit> [--target N] [--limit N] [--restricted]`
  / `account list [--all]` / `account archive <NAME> [--force]` — buckets ARE accounts
- `account reconcile <NAME> --actual N [-c CONCEPT]` — writes the adjusting expense/income needed to
  bring a derived balance to what was physically counted
- `entry list [-p PERIOD] [-c CONCEPT] [--account NAME] [--kind K]` / `entry edit <ID> …` / `entry rm <ID>`
- `budget set -c CONCEPT -l LIMIT [-p PERIOD]` / `budget show [-p PERIOD]` / `budget rm -c CONCEPT` —
  informative only; never blocks an over-budget expense
- `report [-p PERIOD] [--detail]`
- `setup [--account NAME=AMOUNT ...] [-D DATE]` — opening balances as `kind='opening'` entries
- `db backup [-o DIR_OR_FILE]` — whole ledger as a restorable SQL file (never overwrites, 0600)
- `db remote login [EMAIL] [--url URL --key KEY]` / `db remote logout` / `db remote status`
- `concept list|add`, `config get|set|list`

After every successful command except `db backup` and `db remote login|logout`, `main.rs` runs the lazy
auto backup: if `last-backup.toml` is missing or older than 7 days, it backs up and prints one line to
stderr; a failure is a warning and never changes the exit code. The GUI does the same on launch.

Config keys (stored in Supabase, table `config`): `emergency_pct` (default 10), `default_account`
(fallback for `add --from`), `income_account` (fallback for `income --to`), `cash_concept` (default
concept for `account reconcile`), `baseline_monthly_expense` (GUI "Meses de colchón" until real months
exist).

## Schema versioning

Schema changes are numbered files in `setup/sql/` (`NNNN_name.sql`), applied by hand in the Supabase
SQL Editor (the Supabase CLI migration system is **not** used). Rules:
- An applied file is never edited; every change is a new file with the next number.
- Each file (from 0003) starts with a guard that aborts unless `public.schema_version` is the previous
  number, and ends by setting it to its own number, all in one transaction (template in
  `setup/README.md` §3).
- Bump `money_core::schema::EXPECTED_SCHEMA_VERSION`; a test fails if it doesn't match the highest file.
- New functions: `revoke execute ... from public, anon` and `grant` only to `authenticated`.
- Run `setup/tests/verify.sql` on a local Supabase before applying to production; apply the schema
  file to production before installing the new app.

At connect time the app compares versions and fails with `AppError::SchemaMismatch`, whose message says
which file to apply (database behind) or to update the app (app behind). A project without `0002`
(no `ledger_status()` RPC, PostgREST `PGRST202`) counts as version 1.

## Backups

`backup_service::render_sql` turns `export_ledger()` (one RPC, one consistent snapshot) into a SQL
script that restores into an empty project at the same schema version: it checks the version and that
the project is empty, maps every row to the user found by email in `auth.users` (prefilled with the
session's email, editable at the `RESTAURAR COMO` line), inserts with explicit ids in dependency order and
fixes the sequences, all between `begin`/`commit`. Output is deterministic (rows by id); a golden test
pins it.

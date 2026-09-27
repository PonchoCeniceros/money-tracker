# Contract: archivos de esquema de Supabase y RPC

**Branch**: `002-remove-mirror-backup` | **Spec**: [spec.md](../spec.md) | **Research**: [research.md](../research.md) R5, R6, R8

## Ubicación y nombres

```
supabase/
  README.md                 # guía del esquema (FR-020)
  sql/
    0001_setup.sql          # = el antiguo migrations/0001_initial.sql, sin cambios de contenido
    0002_schema_version.sql # versión de esquema + correcciones de FR-016
  tests/
    verify.sql              # verificación en un proyecto de prueba (FR-021)
```

- Un archivo aplicado **no se edita nunca**. Cada cambio es un `NNNN_descripcion.sql` nuevo, con
  `NNNN` consecutivo.
- `money_core::schema::EXPECTED_SCHEMA_VERSION` es igual al `NNNN` más alto. Una prueba de
  `cargo test` lo verifica.

## Estructura obligatoria de cada archivo `NNNN` (desde `0003`)

```sql
begin;
do $$ begin
  if (select version from public.schema_version where id = 1) is distinct from NNNN - 1 then
    raise exception 'NNNN: el esquema está en la versión %, se esperaba %',
      (select version from public.schema_version where id = 1), NNNN - 1;
  end if;
end $$;

-- ... cambios ...

update public.schema_version set version = NNNN, applied_at = now() where id = 1;
commit;
```

`0002` usa otra guarda: aborta si `to_regclass('public.entries') is null` (falta `0001`) o si
`to_regclass('public.schema_version') is not null` (ya se aplicó). Al final inserta la fila con
`version = 2`.

## Contenido de `0002` (FR-016)

1. `schema_version`: tabla, RLS y política de `select` para `authenticated`.
2. `sync_state`: se borra `sync_state_policy` y se crea una de `select` para `authenticated`.
3. `apply_entries`, con la misma firma (`returns table(entry_id bigint, …)`):
   - bloqueo `for update` de la cuenta origen antes de calcular el saldo;
   - cuenta destino obligatoriamente del usuario;
   - fórmula de crédito `greatest(-v_balance, 0) + v_amt > v_credit_lim`.
4. `bump_revision_after_delete()` y los triggers `after delete` en `concepts`, `accounts`, `entries`,
   `budgets` y `config`.
5. Se borran los triggers `trg_*_tomb`, la función `tombstone_after_delete`, la tabla `tombstones` y
   la función `pull_changes`.
6. RPC nuevos `ledger_status()` y `export_ledger()` (abajo).
7. `revoke execute on all functions in schema public from public, anon`, y luego `grant execute` a
   `authenticated` solo en `apply_entries`, `ledger_status` y `export_ledger`.
8. Comentarios que describen el esquema actual, sin referencias al espejo SQLite.

## RPC

### `ledger_status() returns jsonb`

```json
{ "revision": 129, "schema_version": 2 }
```

Es `security invoker` y `stable`. Lee `sync_state` y `schema_version`.

### `export_ledger() returns jsonb`

```json
{
  "revision": 131, "schema_version": 2, "exported_at": "2026-09-26T21:04:11.123-06:00",
  "concepts": [{ "id": 1, "name": "Alimentos", "concept_type": "expense" }],
  "accounts": [{ "id": 1, "name": "tdc", "kind": "credit", "target_amount": null,
                 "credit_limit": 3000, "liquid": true, "archived": false }],
  "entries":  [{ "id": 1, "date": "2026-08-01", "kind": "opening", "amount": 40000,
                 "from_account_id": null, "to_account_id": 2, "concept": null,
                 "subconcept": null, "description": null }],
  "budgets":  [{ "id": 1, "concept": "Alimentos", "monthly_limit": 2500, "period": "2026-08" }],
  "config":   [{ "key": "emergency_pct", "value": "10" }]
}
```

- Es `security invoker` y `stable`. Una sola llamada da una sola foto de la base (research R8).
- Incluye las cuentas archivadas. Los arreglos vacíos son `[]`, nunca `null`.

## `supabase/tests/verify.sql` (FR-021)

- Todo corre dentro de `begin; … rollback;`, así que no deja rastro.
- Crea datos mínimos de prueba e intenta cada operación prohibida dentro de un bloque que **espera**
  la excepción. Si la operación pasa, el script lanza `raise exception 'FALLÓ: …'`.
- Casos mínimos:
  - cargo sobre el límite de crédito;
  - retiro mayor al saldo de un bucket;
  - segunda cuenta de emergencia activa;
  - monto `<= 0`;
  - forma inválida por tipo;
  - auto-transferencia;
  - `target_amount` en una cuenta que no es `target`;
  - cuenta destino de otro usuario.
- Termina con `raise notice 'verify.sql: N/N rechazos confirmados'`.
- Los casos sin sesión (RLS, `revoke`) no se pueden simular desde el SQL Editor. Se verifican con las
  dos llamadas `curl` documentadas en el quickstart.

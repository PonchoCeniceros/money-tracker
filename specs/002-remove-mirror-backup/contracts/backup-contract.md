# Contract: archivo de respaldo y respaldo automático

**Branch**: `002-remove-mirror-backup` | **Spec**: [spec.md](../spec.md) | **Research**: [research.md](../research.md) R9, R10

## Ubicación y nombre

- Destino por defecto: `~/.money-tracker/backups/money-tracker-YYYYMMDD-HHMMSS.sql` (hora local). Si el
  nombre ya existe se agrega `-2`, `-3`…
- Destino explícito: `db backup -o RUTA` (CLI) o el campo de ruta de la GUI. Si `RUTA` es una carpeta,
  el archivo se crea adentro con el nombre por defecto. Si es un archivo que ya existe, es un error:
  nunca se sobrescribe.
- Permisos `0600`. Se crea con `create_new`; si la escritura falla, se borra el archivo.

## Estructura del archivo

```sql
-- money-tracker · respaldo del libro contable
-- creado:          2026-09-26T21:04:11-06:00
-- revisión:        131
-- versión esquema: 2
-- usuario origen:  giovanny@example.com
--
-- Restaurar (proyecto de Supabase nuevo y vacío):
--   1. Aplica supabase/sql/0001_setup.sql … 0002_*.sql en el SQL Editor, en orden.
--   2. Crea tu usuario en Authentication → Users (puede ser el mismo email).
--   3. Si el email es otro, cámbialo en la línea marcada con «RESTAURAR COMO».
--   4. Pega este archivo completo en el SQL Editor y dale Run.

begin;

do $$
declare n int;
begin
  if (select version from public.schema_version where id = 1) is distinct from 2 then
    raise exception 'Este respaldo es de la versión de esquema 2 y el proyecto está en %', …;
  end if;
  if exists (select 1 from public.accounts) or exists (select 1 from public.entries)
     or exists (select 1 from public.concepts) or exists (select 1 from public.budgets)
     or exists (select 1 from public.config) then
    raise exception 'El proyecto no está vacío: solo se restaura sobre un proyecto nuevo';
  end if;
end $$;

create temp table _restore_user on commit drop as
  select id from auth.users where email = 'giovanny@example.com';  -- RESTAURAR COMO
do $$ begin
  if (select count(*) from _restore_user) <> 1 then
    raise exception 'No existe exactamente un usuario con ese email en auth.users';
  end if;
end $$;

insert into public.concepts (id, user_id, name, concept_type) values
  (1, (select id from _restore_user), 'Alimentos', 'expense'), …;
insert into public.accounts (id, user_id, name, kind, target_amount, credit_limit, liquid, archived) values …;
insert into public.entries (id, user_id, date, kind, amount, from_account_id, to_account_id,
                            concept, subconcept, description) values …;
insert into public.budgets (id, user_id, concept, monthly_limit, period) values …;
insert into public.config (user_id, key, value) values …;

select setval(pg_get_serial_sequence('public.concepts', 'id'), coalesce(max(id), 1)) from public.concepts;
select setval(pg_get_serial_sequence('public.accounts', 'id'), coalesce(max(id), 1)) from public.accounts;
select setval(pg_get_serial_sequence('public.entries',  'id'), coalesce(max(id), 1)) from public.entries;
select setval(pg_get_serial_sequence('public.budgets',  'id'), coalesce(max(id), 1)) from public.budgets;

commit;
```

Reglas de generación:

- Un `insert` por tabla con varias filas en `values`. Las tablas vacías no generan `insert`.
- Texto: `'...'` con cada `'` duplicado. `null` literal cuando no hay valor. Números con la
  representación exacta del `f64`. Booleanos como `true`/`false`.
- Orden de las filas: por `id` ascendente, para que el archivo sea determinista y comparable con
  `diff`.

## Respaldo automático

| Momento | Condición para respaldar | Si falla |
|---|---|---|
| Al terminar un comando del CLI | Comando exitoso y no excluido, Supabase configurado, `is_due` | Aviso en stderr; mismo código de salida |
| Al abrir la GUI | Supabase configurado y conectado, `is_due` | Aviso; la GUI sigue normal |

- Lista de exclusión del CLI: `db backup`, `db remote login`, `db remote logout`. Además, clap sale
  antes con `--help` o un error de argumentos, así que ahí nunca llega a revisarse.
- `is_due` solo lee `~/.money-tracker/last-backup.toml`, así que no toca la red. Solo se conecta a
  Supabase cuando toca respaldar.
- Mensajes:
  - Éxito: `Respaldo automático: <ruta> (131 movimientos)`.
  - Falla: `Aviso: no se pudo hacer el respaldo automático (<causa>). Se reintentará la próxima vez.`
- El registro se actualiza después de **cualquier** respaldo exitoso, manual o automático.

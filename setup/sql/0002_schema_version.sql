-- 0002_schema_version.sql — versión de esquema + correcciones de seguridad y validación.
--
-- Spec: specs/002-remove-mirror-backup (FR-014, FR-016). Contrato: contracts/schema-contract.md.
--
-- Cómo aplicarlo: pega el archivo COMPLETO en el SQL Editor de Supabase y dale Run, después de
-- 0001_setup.sql. Todo corre en una sola transacción: si algo falla, no cambia nada. Si ya se
-- aplicó, se detiene al inicio sin cambios.
--
-- Qué hace:
--   1. Crea public.schema_version (esta base queda en la versión 2).
--   2. sync_state: solo lectura y solo con sesión (antes cualquiera con la key publicable podía
--      leerlo y modificarlo).
--   3. apply_entries: corrige la fórmula del límite de crédito, bloquea la cuenta origen durante la
--      validación y exige que la cuenta destino sea del usuario. Misma firma que en 0001.
--   4. Los borrados vuelven a subir la revisión con un trigger propio (antes lo hacía el trigger del
--      espejo, que se elimina en el punto 5).
--   5. Elimina lo que solo servía al espejo local: pull_changes, la tabla tombstones y sus triggers.
--   6. RPC nuevos: ledger_status() y export_ledger().
--   7. Quita el permiso de ejecutar funciones sin sesión; solo 'authenticated' ejecuta los RPC.
--
-- Las reglas contables viven en money_core (money_core/src/rules.rs). Lo de aquí es la segunda
-- defensa: protege el libro aunque un cliente se equivoque o dos dispositivos escriban a la vez.

begin;

-- ---------------------------------------------------------------------------
-- Guarda: 0001 aplicado y 0002 no aplicado todavía.
-- ---------------------------------------------------------------------------
do $$
begin
  if to_regclass('public.entries') is null then
    raise exception '0002: falta 0001_setup.sql (no existe public.entries). Aplícalo primero.';
  end if;
  if to_regclass('public.schema_version') is not null then
    raise exception '0002: ya estaba aplicado (public.schema_version ya existe). No se cambió nada.';
  end if;
end $$;

-- ---------------------------------------------------------------------------
-- 1. schema_version: una sola fila; la escriben solo los archivos de esquema.
-- ---------------------------------------------------------------------------
create table public.schema_version (
  id         smallint primary key default 1 check (id = 1),
  version    integer not null,
  applied_at timestamptz not null default now()
);

alter table public.schema_version enable row level security;

create policy schema_version_select on public.schema_version
  for select to authenticated using (true);

comment on table public.schema_version is
  'Versión del esquema aplicada (una fila). Cada supabase/sql/NNNN_*.sql la verifica y la sube. '
  'La app la compara con money_core::schema::EXPECTED_SCHEMA_VERSION al conectar.';

-- ---------------------------------------------------------------------------
-- 2. sync_state: solo lectura, solo con sesión. touch_row() y
--    bump_revision_after_delete() son security definer y siguen escribiendo.
-- ---------------------------------------------------------------------------
drop policy if exists sync_state_policy on public.sync_state;

create policy sync_state_select on public.sync_state
  for select to authenticated using (true);

comment on table public.sync_state is
  'Contador global de revisión: sube con cada insert, update y delete del libro. '
  'La GUI lo sondea (ledger_status) para saber cuándo recargar.';

-- ---------------------------------------------------------------------------
-- 3. apply_entries: el único escritor de movimientos. Misma firma que 0001.
-- ---------------------------------------------------------------------------
create or replace function public.apply_entries(p_entries jsonb)
returns table (
  entry_id        bigint,
  date            date,
  kind            text,
  amount          numeric,
  from_account_id bigint,
  to_account_id   bigint,
  from_account    text,
  to_account      text,
  concept         text,
  subconcept      text,
  description     text
)
language plpgsql
security definer
set search_path = public
as $$
declare
  v_entry      jsonb;
  v_kind       text;
  v_from       bigint;
  v_to         bigint;
  v_amt        numeric;
  v_src_kind   text;
  v_balance    numeric;
  v_credit_lim numeric;
  v_entry_id   bigint;
  v_ids        bigint[] := '{}';
begin
  if p_entries is null or jsonb_array_length(p_entries) = 0 then
    raise exception 'apply_entries: no entries given' using errcode = '23400';
  end if;

  for v_entry in select * from jsonb_array_elements(p_entries) loop
    v_kind := v_entry ->> 'kind';
    v_from := (v_entry ->> 'from_account_id')::bigint;
    v_to   := (v_entry ->> 'to_account_id')::bigint;
    v_amt  := (v_entry ->> 'amount')::numeric;

    if v_kind not in ('income','expense','transfer','opening') then
      raise exception 'apply_entries: unknown kind %', v_kind using errcode = '23514';
    end if;
    if (v_kind in ('income','opening') and (v_from is not null or v_to is null))
       or (v_kind = 'expense' and (v_from is null or v_to is not null))
       or (v_kind = 'transfer' and (v_from is null or v_to is null or v_from = v_to)) then
      raise exception 'apply_entries: invalid account shape for kind %', v_kind
        using errcode = '23514';
    end if;
    if v_amt is null or v_amt <= 0 then
      raise exception 'apply_entries: amount must be positive' using errcode = '23514';
    end if;

    -- La cuenta destino (si hay) debe ser del usuario.
    if v_to is not null and not exists (
      select 1 from public.accounts a where a.id = v_to and a.user_id = auth.uid()
    ) then
      raise exception 'apply_entries: destination account % not found', v_to using errcode = '23503';
    end if;

    -- Política de sobregiro sobre la cuenta origen de un gasto o transferencia.
    --   target/emergency: el retiro no puede exceder el saldo
    --   credit: deuda (= max(-saldo, 0)) + cargo no puede exceder el límite, si lo hay
    --   spending: sin validación (el saldo puede quedar negativo)
    if v_kind in ('expense','transfer') then
      -- Bloquea la cuenta origen hasta el commit: dos escrituras simultáneas sobre la misma
      -- cuenta se validan en serie, y la segunda ve el saldo que dejó la primera.
      select a.kind, a.credit_limit
        into v_src_kind, v_credit_lim
        from public.accounts a
       where a.id = v_from and a.user_id = auth.uid()
         for update;
      if v_src_kind is null then
        raise exception 'apply_entries: source account % not found', v_from using errcode = '23503';
      end if;

      select coalesce(round(sum(d.m), 2), 0)
        into v_balance
        from (
          select e.amount as m from public.entries e
           where e.to_account_id = v_from and e.user_id = auth.uid()
          union all
          select -e.amount from public.entries e
           where e.from_account_id = v_from and e.user_id = auth.uid()
        ) d;

      if v_src_kind in ('target','emergency') and v_amt > v_balance then
        raise exception 'apply_entries: insufficient balance (have %, need %)', v_balance, v_amt
          using errcode = '23514';
      end if;
      if v_src_kind = 'credit' and v_credit_lim is not null
         and greatest(-v_balance, 0) + v_amt > v_credit_lim then
        raise exception 'apply_entries: exceeds credit limit (% available, % requested)',
          v_credit_lim - greatest(-v_balance, 0), v_amt
          using errcode = '23514';
      end if;
    end if;

    insert into public.entries
      (date, kind, amount, from_account_id, to_account_id, concept, subconcept, description)
    values
      ((v_entry ->> 'date')::date, v_kind, v_amt, v_from, v_to,
       v_entry ->> 'concept', v_entry ->> 'subconcept', v_entry ->> 'description')
    returning id into v_entry_id;
    v_ids := array_append(v_ids, v_entry_id);
  end loop;

  return query
    select e.id as entry_id, e.date, e.kind, e.amount, e.from_account_id, e.to_account_id,
           fa.name as from_account, ta.name as to_account,
           e.concept, e.subconcept, e.description
      from public.entries e
      left join public.accounts fa on fa.id = e.from_account_id and fa.user_id = e.user_id
      left join public.accounts ta on ta.id = e.to_account_id   and ta.user_id = e.user_id
     where e.id = any (v_ids)
     order by e.id;
end;
$$;

comment on function public.apply_entries(jsonb) is
  'Único escritor de movimientos: valida forma, dueño de las cuentas y sobregiro/límite de crédito, '
  'e inserta el lote completo o nada. Las mismas reglas viven en money_core/src/rules.rs.';

-- ---------------------------------------------------------------------------
-- 4. Los borrados suben la revisión (antes lo hacía tombstone_after_delete).
-- ---------------------------------------------------------------------------
create or replace function public.bump_revision_after_delete()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
begin
  update public.sync_state set revision = revision + 1 where id = 1;
  return old;
end;
$$;

-- ---------------------------------------------------------------------------
-- 5. Fuera lo que solo servía al espejo local.
-- ---------------------------------------------------------------------------
drop trigger if exists trg_entries_tomb  on public.entries;
drop trigger if exists trg_budgets_tomb  on public.budgets;
drop trigger if exists trg_accounts_tomb on public.accounts;
drop trigger if exists trg_concepts_tomb on public.concepts;
drop trigger if exists trg_config_tomb   on public.config;
drop function if exists public.tombstone_after_delete();
drop table if exists public.tombstones;
drop function if exists public.pull_changes(bigint);

create trigger trg_entries_bump_on_delete  after delete on public.entries
  for each row execute function public.bump_revision_after_delete();
create trigger trg_budgets_bump_on_delete  after delete on public.budgets
  for each row execute function public.bump_revision_after_delete();
create trigger trg_accounts_bump_on_delete after delete on public.accounts
  for each row execute function public.bump_revision_after_delete();
create trigger trg_concepts_bump_on_delete after delete on public.concepts
  for each row execute function public.bump_revision_after_delete();
create trigger trg_config_bump_on_delete   after delete on public.config
  for each row execute function public.bump_revision_after_delete();

-- ---------------------------------------------------------------------------
-- 6. RPC nuevos. security invoker: RLS filtra por el usuario de la sesión.
-- ---------------------------------------------------------------------------
create function public.ledger_status()
returns jsonb
language sql
stable
security invoker
set search_path = public
as $$
  select jsonb_build_object(
    'revision',       (select s.revision from public.sync_state s where s.id = 1),
    'schema_version', (select v.version  from public.schema_version v where v.id = 1)
  );
$$;

comment on function public.ledger_status() is
  'Revisión del libro y versión del esquema. La usan la revisión de versión al conectar, '
  'el sondeo de la GUI y `db remote status`.';

-- Una sola llamada = una sola foto de la base (misma instantánea MVCC para todas las tablas),
-- así el respaldo es consistente aunque otro dispositivo esté escribiendo. Un jsonb es un solo
-- valor, así que no le aplica el tope de filas de PostgREST.
create function public.export_ledger()
returns jsonb
language sql
stable
security invoker
set search_path = public
as $$
  select jsonb_build_object(
    'revision',       (select s.revision from public.sync_state s where s.id = 1),
    'schema_version', (select v.version  from public.schema_version v where v.id = 1),
    'exported_at',    now(),
    'concepts', coalesce((
      select jsonb_agg(jsonb_build_object(
               'id', c.id, 'name', c.name, 'concept_type', c.concept_type) order by c.id)
        from public.concepts c where c.user_id = auth.uid()), '[]'::jsonb),
    'accounts', coalesce((
      select jsonb_agg(jsonb_build_object(
               'id', a.id, 'name', a.name, 'kind', a.kind,
               'target_amount', a.target_amount, 'credit_limit', a.credit_limit,
               'liquid', a.liquid, 'archived', a.archived) order by a.id)
        from public.accounts a where a.user_id = auth.uid()), '[]'::jsonb),
    'entries', coalesce((
      select jsonb_agg(jsonb_build_object(
               'id', e.id, 'date', to_char(e.date, 'YYYY-MM-DD'), 'kind', e.kind,
               'amount', e.amount,
               'from_account_id', e.from_account_id, 'to_account_id', e.to_account_id,
               'concept', e.concept, 'subconcept', e.subconcept,
               'description', e.description) order by e.id)
        from public.entries e where e.user_id = auth.uid()), '[]'::jsonb),
    'budgets', coalesce((
      select jsonb_agg(jsonb_build_object(
               'id', b.id, 'concept', b.concept,
               'monthly_limit', b.monthly_limit, 'period', b.period) order by b.id)
        from public.budgets b where b.user_id = auth.uid()), '[]'::jsonb),
    'config', coalesce((
      select jsonb_agg(jsonb_build_object('key', k.key, 'value', k.value) order by k.key)
        from public.config k where k.user_id = auth.uid()), '[]'::jsonb)
  );
$$;

comment on function public.export_ledger() is
  'Libro contable completo del usuario en un solo jsonb, para el respaldo (`db backup`).';

-- ---------------------------------------------------------------------------
-- 7. Permisos de funciones: nada sin sesión; solo 'authenticated' ejecuta los RPC.
--    Los triggers no necesitan este permiso para dispararse; se les concede solo por
--    claridad (una función de trigger no se puede llamar directo).
-- ---------------------------------------------------------------------------
revoke execute on all functions in schema public from public, anon;

grant execute on function public.apply_entries(jsonb)          to authenticated;
grant execute on function public.ledger_status()               to authenticated;
grant execute on function public.export_ledger()               to authenticated;
grant execute on function public.touch_row()                   to authenticated;
grant execute on function public.bump_revision_after_delete()  to authenticated;

-- ---------------------------------------------------------------------------
-- Esta base queda en la versión 2.
-- ---------------------------------------------------------------------------
insert into public.schema_version (id, version) values (1, 2);

commit;

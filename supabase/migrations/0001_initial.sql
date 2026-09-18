-- 0001_initial.sql — Supabase ledger schema for money-tracker.
--
-- Mirrors the SQLite mirror schema (money_core/src/db.rs, SCHEMA_VERSION 2)
-- one-to-one: same tables, same columns, same CHECK constraints, same
-- unique indices, plus three additions:
--   * user_id: whose row it is (RLS == auth.uid()).
--   * updated_at: timestamptz, set by a trigger on writes (display only —
--     the monotonic cursor for sync is `rev`).
--   * rev: monotonic bigint cursor set from sync_state.revision by the same
--     trigger. `pull_changes_since(cursor)` fetches `rev > cursor`; this is
--     timezone-proof and race-free, unlike comparing updated_at strings.
--
-- All ledger logic lives in services in Rust (Principle I); the only SQL
-- "logic" here is the CHECK constraints duplicated from the SQLite schema
-- (so both stores enforce the same invariants) and the apply_entries RPC
-- (the sole server-side writer, which re-checks overdraft atomically).

begin;

-- ---------------------------------------------------------------------------
-- sync_state: single row, uncontended revision counter.
-- ---------------------------------------------------------------------------
create table if not exists public.sync_state (
  id bigint primary key default 1 check (id = 1),
  revision bigint not null default 0
);

-- The "touch" trigger shared by every ledger table.
create or replace function public.touch_row()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
declare
  v_rev bigint;
begin
  update public.sync_state
     set revision = revision + 1
   where id = 1
  returning revision into v_rev;
  new.updated_at := clock_timestamp();
  new.rev := v_rev;
  return new;
end;
$$;

-- ---------------------------------------------------------------------------
-- concepts (shared vocabulary; also lives in SQLite mirror)
-- ---------------------------------------------------------------------------
create table if not exists public.concepts (
  id           bigserial primary key,
  user_id      uuid not null default auth.uid(),
  name         text not null,
  concept_type text not null check (concept_type in ('expense','income','both')),
  updated_at   timestamptz not null default now(),
  rev          bigint not null default 0,
  unique (user_id, name)
);

drop trigger if exists trg_concepts_touch on public.concepts;
create trigger trg_concepts_touch
  before insert or update on public.concepts
  for each row execute function public.touch_row();

-- ---------------------------------------------------------------------------
-- accounts
-- ---------------------------------------------------------------------------
create table if not exists public.accounts (
  id            bigserial primary key,
  user_id       uuid not null default auth.uid(),
  name          text not null,
  kind          text not null check (kind in ('spending','emergency','target','credit')),
  target_amount numeric check (target_amount is null or target_amount > 0),
  credit_limit  numeric check (credit_limit  is null or credit_limit  > 0),
  liquid        boolean not null default true check (liquid in (true,false)),
  archived      boolean not null default false check (archived in (true,false)),
  updated_at    timestamptz not null default now(),
  rev           bigint not null default 0,
  unique (user_id, name),
  check (kind = 'target' or target_amount is null),
  check (kind = 'credit' or credit_limit  is null)
);

drop trigger if exists trg_accounts_touch on public.accounts;
create trigger trg_accounts_touch
  before insert or update on public.accounts
  for each row execute function public.touch_row();

-- at most one active emergency account per user.
create unique index if not exists idx_accounts_one_emergency
  on public.accounts (user_id, kind)
  where kind = 'emergency' and archived = false;

-- ---------------------------------------------------------------------------
-- entries
-- ---------------------------------------------------------------------------
create table if not exists public.entries (
  id              bigserial primary key,
  user_id         uuid not null default auth.uid(),
  date            date not null,
  kind            text not null check (kind in ('income','expense','transfer','opening')),
  amount          numeric not null check (amount > 0),
  from_account_id bigint references public.accounts(id),
  to_account_id   bigint references public.accounts(id),
  concept         text,
  subconcept      text,
  description     text,
  updated_at      timestamptz not null default now(),
  rev             bigint not null default 0,
  check (
    (kind in ('income','opening')
        and from_account_id is null     and to_account_id is not null)
    or (kind = 'expense'
        and from_account_id is not null and to_account_id is null)
    or (kind = 'transfer'
        and from_account_id is not null and to_account_id is not null
        and from_account_id <> to_account_id)
  ),
  check (kind in ('transfer','opening') or concept is not null),
  foreign key (user_id, concept) references public.concepts(user_id, name) on update cascade
);

drop trigger if exists trg_entries_touch on public.entries;
create trigger trg_entries_touch
  before insert or update on public.entries
  for each row execute function public.touch_row();

create index if not exists idx_entries_date         on public.entries(user_id, date);
create index if not exists idx_entries_kind_date    on public.entries(user_id, kind, date);
create index if not exists idx_entries_concept_date on public.entries(user_id, concept, date) where kind = 'expense';
create index if not exists idx_entries_from         on public.entries(user_id, from_account_id) where from_account_id is not null;
create index if not exists idx_entries_to           on public.entries(user_id, to_account_id)   where to_account_id   is not null;
create index if not exists idx_entries_rev          on public.entries(user_id, rev);

-- ---------------------------------------------------------------------------
-- budgets (informative only — never blocks a write, mirror of SQLite)
-- ---------------------------------------------------------------------------
create table if not exists public.budgets (
  id            bigserial primary key,
  user_id       uuid not null default auth.uid(),
  concept       text not null,
  monthly_limit numeric not null check (monthly_limit > 0),
  period        text not null check (period ~ '^\d{4}-\d{2}$'),
  updated_at    timestamptz not null default now(),
  rev           bigint not null default 0,
  unique (user_id, concept, period),
  foreign key (user_id, concept) references public.concepts(user_id, name) on update cascade
);

drop trigger if exists trg_budgets_touch on public.budgets;
create trigger trg_budgets_touch
  before insert or update on public.budgets
  for each row execute function public.touch_row();
create index if not exists idx_budgets_rev on public.budgets(user_id, rev);

-- ---------------------------------------------------------------------------
-- config (app settings replicated to the ledger, not the config.toml file)
-- ---------------------------------------------------------------------------
create table if not exists public.config (
  key      text not null,
  user_id  uuid not null default auth.uid(),
  value    text not null,
  updated_at timestamptz not null default now(),
  rev      bigint not null default 0,
  primary key (user_id, key)
);

drop trigger if exists trg_config_touch on public.config;
create trigger trg_config_touch
  before insert or update on public.config
  for each row execute function public.touch_row();
create index if not exists idx_config_rev on public.config(user_id, rev);

-- ---------------------------------------------------------------------------
-- Views: mirrors of the SQLite `account_balances` / `entries_view`, used so
-- the same derived numbers come out of both stores.
-- ---------------------------------------------------------------------------
create or replace view public.account_balances
with (security_invoker = true) as
select a.id, a.name, a.kind, a.target_amount, a.credit_limit, a.liquid, a.archived,
       coalesce(m.balance, 0) as balance
from public.accounts a
left join (
  select account_id, round(sum(delta), 2) as balance
  from (
    select to_account_id   as account_id,  amount as delta
      from public.entries where to_account_id   is not null
    union all
    select from_account_id as account_id, -amount as delta
      from public.entries where from_account_id is not null
  ) d
  group by account_id
) m on m.account_id = a.id;

create or replace view public.entries_view
with (security_invoker = true) as
select e.*, fa.name as from_account, ta.name as to_account
from public.entries e
left join public.accounts fa on fa.id = e.from_account_id
left join public.accounts ta on ta.id = e.to_account_id;

-- ---------------------------------------------------------------------------
-- apply_entries: the sole server-side writer.
--
-- Takes entries as JSON (same shape as the Rust NewEntry, kind as string),
-- validates them inside a transaction *after* recomputing balances from the
-- ledger it can see (RLS protects it), applies the same overdraft policy as
-- money_core's SqliteBackend, inserts, and returns the freshly created rows
-- (this is what lets the client know the split transfer's id).
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
  v_entry     jsonb;
  v_kind      text;
  v_from      bigint;
  v_to        bigint;
  v_amt       numeric;
  v_src_kind  text;
  v_balance   numeric;
  v_credit_lim numeric;
  v_entry_id  bigint;
  v_ids       bigint[] := '{}';
begin
  if p_entries is null or jsonb_array_length(p_entries) = 0 then
    raise exception 'apply_entries: no entries given' using errcode = '23400';
  end if;

  for v_entry in select * from jsonb_array_elements(p_entries) loop
    v_kind := v_entry ->> 'kind';
    v_from := (v_entry ->> 'from_account_id')::bigint;
    v_to   := (v_entry ->> 'to_account_id')::bigint;
    v_amt  := (v_entry ->> 'amount')::numeric;

    -- Only the client-specified shape may arrive; the CHECK constraints
    -- below are the backstop, but validate here for a clear message.
    if (v_kind in ('income','opening') and v_from is not null)
       or (v_kind = 'expense' and v_to is not null)
       or (v_kind = 'transfer' and (v_from is null or v_to is null or v_from = v_to)) then
      raise exception 'apply_entries: invalid account shape for kind %', v_kind
        using errcode = '23514';
    end if;
    if v_kind not in ('income','expense','transfer','opening') then
      raise exception 'apply_entries: unknown kind %', v_kind using errcode = '23514';
    end if;
    if v_amt is null or v_amt <= 0 then
      raise exception 'apply_entries: amount must be positive' using errcode = '23514';
    end if;

  -- Overdraft policy for the source of an expense or transfer, computed
  -- from the ledger *as visible to this session*:
  --   target/emergency: withdrawal must not exceed balance
  --   credit: must not exceed credit_limit when set
  --   spending: no check — balance may go negative
  if v_kind in ('expense','transfer') and v_from is not null then
    select a.kind, a.credit_limit,
           coalesce((
             select round(sum(d.m), 2)
             from (
               select e.amount as m from public.entries e where e.to_account_id   = v_from and e.user_id = auth.uid()
               union all
               select -e.amount from public.entries e where e.from_account_id = v_from and e.user_id = auth.uid()
             ) d
           ), 0)::numeric
      into v_src_kind, v_credit_lim, v_balance
    from public.accounts a where a.id = v_from and a.user_id = auth.uid();
      if v_src_kind is null then
        raise exception 'apply_entries: source account % not found', v_from using errcode = '23503';
      end if;
      if v_src_kind in ('target','emergency') and v_amt > v_balance then
        raise exception 'apply_entries: insufficient balance (have %, need %)', v_balance, v_amt
          using errcode = '23514';
      end if;
      if v_src_kind = 'credit' and v_credit_lim is not null then
        if least(v_balance, 0) + v_amt > v_credit_lim then
          raise exception 'apply_entries: exceeds credit limit (%)', v_credit_lim
            using errcode = '23514';
        end if;
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
    left join public.accounts ta on ta.id = e.to_account_id and ta.user_id = e.user_id
    where e.id = any (v_ids)
    order by e.id;
end;
$$;

-- ---------------------------------------------------------------------------
-- tombstones: row deletions can't be represented by a row-level `rev`
-- cursor, so AFTER DELETE triggers stamp the global clock and record the
-- key. `pull_changes` returns them and the mirror replays the deletes.
-- ---------------------------------------------------------------------------
create table if not exists public.tombstones (
  id         bigint generated always as identity primary key,
  user_id    uuid not null default auth.uid(),
  table_name text not null,
  row_id     bigint not null,
  rev        bigint not null default 0,
  created_at timestamptz not null default now(),
  unique (user_id, table_name, row_id)
);

create or replace function public.tombstone_after_delete()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
declare
  v_rev bigint;
begin
  update public.sync_state set revision = revision + 1 where id = 1 returning revision into v_rev;
  insert into public.tombstones (table_name, row_id, rev)
  values (tg_table_name, old.id, v_rev)
  on conflict (user_id, table_name, row_id) do update set rev = excluded.rev;
  return old;
end;
$$;

drop trigger if exists trg_entries_tomb on public.entries;
create trigger trg_entries_tomb  after delete on public.entries  for each row execute function public.tombstone_after_delete();
drop trigger if exists trg_budgets_tomb on public.budgets;
create trigger trg_budgets_tomb  after delete on public.budgets  for each row execute function public.tombstone_after_delete();
drop trigger if exists trg_accounts_tomb on public.accounts;
create trigger trg_accounts_tomb after delete on public.accounts for each row execute function public.tombstone_after_delete();
drop trigger if exists trg_concepts_tomb on public.concepts;
create trigger trg_concepts_tomb after delete on public.concepts for each row execute function public.tombstone_after_delete();
drop trigger if exists trg_config_tomb on public.config;
create trigger trg_config_tomb   after delete on public.config   for each row execute function public.tombstone_after_delete();

-- ---------------------------------------------------------------------------
-- pull_changes: consistent snapshot of every row newer than the caller's
-- cursor, plus the global revision at that snapshot. Because it is one
-- function call, all five sub-selects run under the same MVCC snapshot —
-- row-level revision comparisons (rev > cursor) are exact even if a writer
-- commits between two separate table queries.
-- ---------------------------------------------------------------------------
create or replace function public.pull_changes(p_cursor bigint)
returns jsonb
language plpgsql
security definer
set search_path = public
as $$
declare
  v_rev bigint;
begin
  -- snapshot the global clock inside this function's snapshot
  select revision into v_rev from public.sync_state where id = 1;

  return jsonb_build_object(
    'cursor',  v_rev,
    'concepts', coalesce((
        select jsonb_agg(jsonb_build_object(
                 'id', c.id, 'name', c.name, 'concept_type', c.concept_type))
        from public.concepts c
        where c.user_id = auth.uid() and c.rev > p_cursor), '[]'::jsonb),
    'accounts', coalesce((
        select jsonb_agg(jsonb_build_object(
                 'id', a.id, 'name', a.name, 'kind', a.kind,
                 'target_amount', a.target_amount, 'credit_limit', a.credit_limit,
                 'liquid', a.liquid, 'archived', a.archived))
        from public.accounts a
        where a.user_id = auth.uid() and a.rev > p_cursor), '[]'::jsonb),
    'budgets', coalesce((
        select jsonb_agg(jsonb_build_object(
                 'id', b.id, 'concept', b.concept,
                 'monthly_limit', b.monthly_limit, 'period', b.period))
        from public.budgets b
        where b.user_id = auth.uid() and b.rev > p_cursor), '[]'::jsonb),
    'entries', coalesce((
        select jsonb_agg(jsonb_build_object(
                 'id', e.id, 'date', to_char(e.date, 'YYYY-MM-DD'),
                 'kind', e.kind, 'amount', e.amount,
                 'from_account_id', e.from_account_id, 'to_account_id', e.to_account_id,
                 'concept', e.concept, 'subconcept', e.subconcept, 'description', e.description))
        from public.entries e
        where e.user_id = auth.uid() and e.rev > p_cursor), '[]'::jsonb),
    'config', coalesce((
        select jsonb_agg(jsonb_build_object('key', c.key, 'value', c.value))
        from public.config c
        where c.user_id = auth.uid() and c.rev > p_cursor), '[]'::jsonb),
    'deletes', coalesce((
        select jsonb_agg(jsonb_build_object('table', t.table_name, 'id', t.row_id))
        from public.tombstones t
        where t.user_id = auth.uid() and t.rev > p_cursor), '[]'::jsonb)
  );
end;
$$;

grant execute on function public.pull_changes(bigint) to authenticated;
grant execute on function public.pull_changes(bigint) to service_role;

-- ---------------------------------------------------------------------------
-- RLS: everything is scoped to the single authenticated user.
-- The `authenticated` role can read/write only its own rows; `anon` nothing.
-- ---------------------------------------------------------------------------
alter table public.sync_state enable row level security;
alter table public.concepts       enable row level security;
alter table public.accounts       enable row level security;
alter table public.entries        enable row level security;
alter table public.budgets        enable row level security;
alter table public.config         enable row level security;
alter table public.tombstones     enable row level security;

-- The revision counter is readable so clients can poll it; only its own
-- process (nothing in practice) ever writes it — writes happen through the
-- trigger inside a definer function, which bypasses RLS.
drop policy if exists sync_state_policy on public.sync_state;
create policy sync_state_policy
  on public.sync_state for all
  using (true) with check (true);

drop policy if exists concepts_select on public.concepts;
create policy concepts_select on public.concepts
  for select using (user_id = auth.uid());

drop policy if exists concepts_insert on public.concepts;
create policy concepts_insert on public.concepts
  for insert with check (user_id = auth.uid());

drop policy if exists concepts_update on public.concepts;
create policy concepts_update on public.concepts
  for update using (user_id = auth.uid()) with check (user_id = auth.uid());

drop policy if exists concepts_delete on public.concepts;
create policy concepts_delete on public.concepts
  for delete using (user_id = auth.uid());

drop policy if exists accounts_select on public.accounts;
create policy accounts_select on public.accounts
  for select using (user_id = auth.uid());

drop policy if exists accounts_insert on public.accounts;
create policy accounts_insert on public.accounts
  for insert with check (user_id = auth.uid());

drop policy if exists accounts_update on public.accounts;
create policy accounts_update on public.accounts
  for update using (user_id = auth.uid()) with check (user_id = auth.uid());

drop policy if exists accounts_delete on public.accounts;
create policy accounts_delete on public.accounts
  for delete using (user_id = auth.uid());

drop policy if exists entries_select on public.entries;
create policy entries_select on public.entries
  for select using (user_id = auth.uid());

drop policy if exists entries_insert on public.entries;
create policy entries_insert on public.entries
  for insert with check (user_id = auth.uid());

drop policy if exists entries_update on public.entries;
create policy entries_update on public.entries
  for update using (user_id = auth.uid()) with check (user_id = auth.uid());

drop policy if exists entries_delete on public.entries;
create policy entries_delete on public.entries
  for delete using (user_id = auth.uid());

drop policy if exists budgets_select on public.budgets;
create policy budgets_select on public.budgets
  for select using (user_id = auth.uid());

drop policy if exists budgets_insert on public.budgets;
create policy budgets_insert on public.budgets
  for insert with check (user_id = auth.uid());

drop policy if exists budgets_update on public.budgets;
create policy budgets_update on public.budgets
  for update using (user_id = auth.uid()) with check (user_id = auth.uid());

drop policy if exists budgets_delete on public.budgets;
create policy budgets_delete on public.budgets
  for delete using (user_id = auth.uid());

drop policy if exists config_select on public.config;
create policy config_select on public.config
  for select using (user_id = auth.uid());

drop policy if exists config_insert on public.config;
create policy config_insert on public.config
  for insert with check (user_id = auth.uid());

drop policy if exists config_update on public.config;
create policy config_update on public.config
  for update using (user_id = auth.uid()) with check (user_id = auth.uid());

drop policy if exists config_delete on public.config;
create policy config_delete on public.config
  for delete using (user_id = auth.uid());

drop policy if exists tombstones_select on public.tombstones;
create policy tombstones_select on public.tombstones
  for select using (user_id = auth.uid());

-- PostgREST will honor these once a service role / authenticated role is
-- granted. Public roles are denied by default (RLS denies), so no explicit
-- revoke needed; grants happen automatically for the `authenticated` role
-- via Supabase's default grants on tables in `public`. The function is
-- callable by `authenticated` (publishable key path) and `service_role`:
grant execute on function public.apply_entries(jsonb) to authenticated;
grant execute on function public.apply_entries(jsonb) to service_role;

-- Seed the single sync_state row (only if this migration runs on a fresh
-- project; a later migration must not reset it).
insert into public.sync_state (id, revision)
values (1, 0)
on conflict (id) do nothing;

commit;

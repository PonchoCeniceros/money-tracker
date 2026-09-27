-- verify.sql — comprueba que el esquema rechaza lo que debe rechazar.
--
-- Pégalo completo en el SQL Editor y dale Run. Todo corre dentro de una transacción que
-- termina en ROLLBACK: no deja ningún dato, ni en un proyecto con información real. Aun así,
-- úsalo de preferencia en un Supabase local (supabase start) o antes de aplicar un archivo
-- nuevo en producción.
--
-- Cómo funciona: se hace pasar por usuarios con sesión (como hace Supabase con cada petición,
-- vía request.jwt.claims y el rol authenticated), crea datos mínimos e intenta cada operación
-- prohibida. Si una operación prohibida PASA, el script se detiene con «FALLÓ: …». Si todo sale
-- bien, termina con el aviso «verify.sql: N/N rechazos confirmados».
--
-- Requiere el esquema en la versión 2 (0001_setup.sql + 0002_schema_version.sql).

begin;

do $$
begin
  if (select version from public.schema_version where id = 1) is distinct from 2 then
    raise exception 'verify.sql es para la versión de esquema 2';
  end if;
end $$;

-- Usuario A (inventado; user_id no referencia auth.users) con sesión.
select set_config('request.jwt.claims',
  '{"sub":"00000000-0000-4000-a000-00000000000a","role":"authenticated"}', true);
set local role authenticated;

create temp table _ok (caso text) on commit drop;

insert into public.concepts (name, concept_type) values ('v_concepto', 'both');
insert into public.accounts (name, kind) values ('v_debito', 'spending'), ('v_fondo', 'emergency');
insert into public.accounts (name, kind, credit_limit) values ('v_tdc', 'credit', 3000);
select public.apply_entries(jsonb_build_array(
  jsonb_build_object('date', '2026-09-01', 'kind', 'opening', 'amount', 100,
    'to_account_id', (select id from public.accounts where name = 'v_fondo')),
  jsonb_build_object('date', '2026-09-01', 'kind', 'opening', 'amount', 1000,
    'to_account_id', (select id from public.accounts where name = 'v_debito')),
  jsonb_build_object('date', '2026-09-02', 'kind', 'expense', 'amount', 1902.38, 'concept', 'v_concepto',
    'from_account_id', (select id from public.accounts where name = 'v_tdc'))
));

-- 1. Cargo sobre el límite de crédito (límite 3000, deuda 1902.38, disponible 1097.62).
do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'expense', 'amount', 1097.63, 'concept', 'v_concepto',
    'from_account_id', (select id from public.accounts where name = 'v_tdc'))));
  raise exception 'FALLÓ: se aceptó un cargo sobre el límite de crédito';
exception when check_violation then
  insert into _ok values ('límite de crédito');
end $$;

-- 1b. Justo en el límite sí pasa (y se deshace con el ROLLBACK final).
do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'expense', 'amount', 1097.62, 'concept', 'v_concepto',
    'from_account_id', (select id from public.accounts where name = 'v_tdc'))));
  insert into _ok values ('cargo exacto al límite aceptado');
end $$;

-- 2. Retiro mayor al saldo de un bucket (fondo con 100).
do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'transfer', 'amount', 100.01,
    'from_account_id', (select id from public.accounts where name = 'v_fondo'),
    'to_account_id', (select id from public.accounts where name = 'v_debito'))));
  raise exception 'FALLÓ: se permitió sobregirar un bucket';
exception when check_violation then
  insert into _ok values ('sobregiro de bucket');
end $$;

-- 3. Segunda cuenta de emergencia activa.
do $$
begin
  insert into public.accounts (name, kind) values ('v_fondo2', 'emergency');
  raise exception 'FALLÓ: se aceptó una segunda cuenta de emergencia activa';
exception when unique_violation then
  insert into _ok values ('una sola emergencia');
end $$;

-- 4. Monto no positivo.
do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'income', 'amount', 0, 'concept', 'v_concepto',
    'to_account_id', (select id from public.accounts where name = 'v_debito'))));
  raise exception 'FALLÓ: se aceptó un monto de 0';
exception when check_violation then
  insert into _ok values ('monto positivo');
end $$;

-- 5. Forma inválida para el tipo (un ingreso con cuenta origen).
do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'income', 'amount', 10, 'concept', 'v_concepto',
    'from_account_id', (select id from public.accounts where name = 'v_fondo'),
    'to_account_id', (select id from public.accounts where name = 'v_debito'))));
  raise exception 'FALLÓ: se aceptó un ingreso con cuenta origen';
exception when check_violation then
  insert into _ok values ('forma por tipo');
end $$;

-- 6. Transferencia a la misma cuenta.
do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'transfer', 'amount', 10,
    'from_account_id', (select id from public.accounts where name = 'v_debito'),
    'to_account_id', (select id from public.accounts where name = 'v_debito'))));
  raise exception 'FALLÓ: se aceptó una auto-transferencia';
exception when check_violation then
  insert into _ok values ('auto-transferencia');
end $$;

-- 7. target_amount en una cuenta que no es target.
do $$
begin
  insert into public.accounts (name, kind, target_amount) values ('v_mala', 'spending', 1000);
  raise exception 'FALLÓ: se aceptó target_amount en una cuenta spending';
exception when check_violation then
  insert into _ok values ('target_amount solo en target');
end $$;

-- 8. Cuenta destino de otro usuario.
select set_config('request.jwt.claims',
  '{"sub":"00000000-0000-4000-b000-00000000000b","role":"authenticated"}', true);
insert into public.accounts (name, kind) values ('v_ajena', 'spending');
create temp table _ajena on commit drop as select id from public.accounts where name = 'v_ajena';
select set_config('request.jwt.claims',
  '{"sub":"00000000-0000-4000-a000-00000000000a","role":"authenticated"}', true);
-- (Cada comprobación va en su propio bloque: un bloque que atrapa una excepción deshace
-- todo lo que hizo, incluido el registro de una comprobación anterior dentro de él.)
do $$
begin
  if exists (select 1 from public.accounts where name = 'v_ajena') then
    raise exception 'FALLÓ: el usuario A puede ver una cuenta del usuario B (RLS)';
  end if;
  insert into _ok values ('RLS: no se ven cuentas ajenas');
end $$;

do $$
begin
  perform public.apply_entries(jsonb_build_array(jsonb_build_object(
    'date', '2026-09-03', 'kind', 'income', 'amount', 10, 'concept', 'v_concepto',
    'to_account_id', (select id from _ajena))));
  raise exception 'FALLÓ: se aceptó un ingreso a una cuenta de otro usuario';
exception when foreign_key_violation then
  insert into _ok values ('cuenta destino ajena');
end $$;

-- 9. Con sesión, sync_state es de solo lectura.
do $$
declare
  v_before bigint := (select revision from public.sync_state where id = 1);
  v_rows int;
begin
  update public.sync_state set revision = 0 where id = 1;
  get diagnostics v_rows = row_count;
  if v_rows <> 0 or (select revision from public.sync_state where id = 1) <> v_before then
    raise exception 'FALLÓ: un usuario con sesión pudo modificar sync_state';
  end if;
  insert into _ok values ('sync_state de solo lectura');
end $$;

-- 10. Borrar un movimiento sube la revisión (así la GUI de otra máquina se entera).
do $$
declare
  v_before bigint := (select revision from public.sync_state where id = 1);
begin
  delete from public.entries where id = (
    select id from public.entries where concept = 'v_concepto' order by id desc limit 1);
  if (select revision from public.sync_state where id = 1) <= v_before then
    raise exception 'FALLÓ: borrar un movimiento no subió la revisión';
  end if;
  insert into _ok values ('borrado sube la revisión');
end $$;

-- 11. Sin sesión (rol anon) no se ejecutan funciones ni se ve el contador.
grant select, insert on _ok to anon;
reset role;
set local role anon;
do $$
begin
  if exists (select 1 from public.sync_state) then
    raise exception 'FALLÓ: sin sesión se puede leer sync_state';
  end if;
  insert into _ok values ('anon no lee sync_state');
end $$;

do $$
begin
  perform public.ledger_status();
  raise exception 'FALLÓ: sin sesión se puede ejecutar ledger_status()';
exception when insufficient_privilege then
  insert into _ok values ('anon no ejecuta funciones');
end $$;

do $$
declare
  n int := (select count(*) from _ok);
begin
  raise notice 'verify.sql: %/% rechazos confirmados', n, 14;
  if n <> 14 then
    raise exception 'FALLÓ: se esperaban 14 comprobaciones y pasaron %', n;
  end if;
end $$;

rollback;

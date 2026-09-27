# Esquema de Supabase de money-tracker

Todo el libro contable vive en tu proyecto de Supabase. Esta carpeta tiene lo necesario para crearlo, cambiarlo sin
riesgos y reconstruirlo desde un respaldo.

```
supabase/
  sql/
    0001_setup.sql            # tablas, vistas, RLS, triggers y apply_entries
    0002_schema_version.sql   # versión de esquema + correcciones de seguridad y validación
  tests/
    verify.sql                # comprueba que el esquema rechaza lo que debe (termina en ROLLBACK)
  README.md                   # esta guía
```

No se usa el sistema de migraciones del CLI de Supabase (`supabase db push`): los archivos se pegan a mano en el
**SQL Editor** del dashboard. Por eso viven en `sql/` y no en `migrations/`.

## 1. Proyecto nuevo

1. Crea el proyecto en [supabase.com](https://supabase.com) y anota la **Project URL** y la **publishable key**
   (Project Settings → API, la `sb_publishable_…`). Nunca uses la `service_role` en la app.
2. En **SQL Editor**, pega y corre en orden `sql/0001_setup.sql`, luego `sql/0002_schema_version.sql`, y así hasta el
   último archivo.
3. Comprueba la versión:

   ```sql
   select * from public.schema_version;   -- debe coincidir con el número del último archivo
   ```

4. Crea tu usuario en **Authentication → Users** (email y contraseña).
5. Conecta la app: `money-tracker db remote login --url https://<ref>.supabase.co --key sb_publishable_…`, o la
   pantalla Conexión de la GUI.

## 2. Qué hay en el esquema

| Objeto | Qué es |
|---|---|
| `accounts` | Cuentas: `spending`, `emergency`, `target`, `credit`. CHECKs de meta y límite |
| `entries` | Movimientos (`income`, `expense`, `transfer`, `opening`); el tipo da la dirección |
| `concepts`, `budgets`, `config` | Conceptos, presupuestos (informativos) y configuración del libro |
| `account_balances` | Vista: saldo de cada cuenta derivado de los movimientos. Ningún saldo se guarda |
| `entries_view` | Vista: movimientos con el nombre de sus cuentas |
| `sync_state` | Contador global de revisión; sube con cada alta, cambio y borrado |
| `schema_version` | Versión del esquema aplicada (una fila) |
| `apply_entries(jsonb)` | Único escritor de movimientos: valida y guarda un lote completo o nada |
| `ledger_status()` | Revisión + versión de esquema; la app lo usa al conectar y la GUI lo sondea |
| `export_ledger()` | Libro completo en un solo JSON; lo usa `db backup` |
| `touch_row()` y `bump_revision_after_delete()` | Triggers que suben la revisión |

Reglas que impone el propio esquema, como segunda defensa (las principales viven en `money_core/src/rules.rs`):

- Todo es por usuario: RLS en todas las tablas; sin sesión no se ve ni se ejecuta nada.
- Una sola cuenta de emergencia activa por usuario (índice único parcial).
- `target_amount` solo en cuentas `target`; `credit_limit` solo en `credit`; montos siempre positivos.
- `apply_entries` rechaza sobregirar un bucket, pasar el límite de la tarjeta, formas inválidas por tipo,
  auto-transferencias y cuentas de otro usuario. Bloquea la cuenta origen para que dos escrituras simultáneas se
  validen en serie.

## 3. Cambiar el esquema

**Regla de oro: un archivo ya aplicado nunca se edita.** Cada cambio va en un archivo nuevo con el número siguiente:
`sql/0003_descripcion_corta.sql` (4 dígitos, minúsculas, guiones bajos).

Plantilla obligatoria (cambia `3` por el número del archivo):

```sql
begin;
do $$ begin
  if (select version from public.schema_version where id = 1) is distinct from 2 then
    raise exception '0003: el esquema está en la versión %, se esperaba 2',
      (select version from public.schema_version where id = 1);
  end if;
end $$;

-- ... cambios ...

update public.schema_version set version = 3, applied_at = now() where id = 1;
commit;
```

Así un archivo no se puede aplicar dos veces ni en desorden: se detiene al inicio sin cambiar nada.

Al agregar el archivo:

1. Sube `EXPECTED_SCHEMA_VERSION` en `money_core/src/schema.rs`. `cargo test` falla si no coincide con el último
   archivo de esta carpeta.
2. Si creas una función, quítale el permiso público y dáselo solo a quien lo necesite:
   `revoke execute on function public.mi_funcion() from public, anon;` y
   `grant execute on function public.mi_funcion() to authenticated;`.
3. Pruébalo en un Supabase local (sección 5) junto con `tests/verify.sql`.
4. Aplícalo en producción y después instala la app nueva. Mientras tanto, la app vieja se niega a arrancar con un
   mensaje que dice qué hacer, en vez de fallar de formas raras.

## 4. Restaurar un respaldo

`money-tracker db backup` (o **Ajustes → Respaldo** en la GUI, o el respaldo automático cada 7 días) genera un
archivo `.sql` en `~/.money-tracker/backups/`. Para reconstruir el libro en un proyecto nuevo:

1. Crea el proyecto y aplica todos los archivos de `sql/` (sección 1). La versión debe ser la misma que dice el
   encabezado del respaldo.
2. Crea tu usuario en **Authentication → Users**. Si usas otro email que el del respaldo, cámbialo en la línea
   marcada con `RESTAURAR COMO`.
3. Pega el respaldo completo en el SQL Editor y dale Run.
4. Conecta la app al proyecto nuevo (`db remote login --url … --key …`).

El respaldo corre completo o no aplica nada, y se niega a correr si el proyecto no está vacío o si su versión de
esquema es otra. Ajusta las secuencias, así que la app puede seguir registrando movimientos de inmediato.

## 5. Verificar

**`tests/verify.sql`**: pégalo en el SQL Editor y dale Run. Se hace pasar por usuarios con sesión y sin sesión,
intenta cada operación prohibida (cargo sobre el límite, sobregiro, segunda emergencia, monto 0, forma inválida,
auto-transferencia, `target_amount` en otra cuenta, cuenta ajena, escribir `sync_state`, ejecutar funciones sin
sesión) y confirma que el borrado sube la revisión. Termina en `ROLLBACK`: no deja datos. Resultado esperado:

```
NOTICE:  verify.sql: 14/14 rechazos confirmados
```

**Supabase local** (Docker), para probar sin tocar tu proyecto real:

```sh
mkdir -p /tmp/mt-local && cd /tmp/mt-local && supabase init
supabase start -x realtime,storage-api,imgproxy,inbucket,postgres-meta,studio,edge-runtime,logflare,vector,supavisor
# aplica los archivos como lo haría el SQL Editor:
for f in ~/Projects/money-tracker/supabase/sql/*.sql; do
  docker exec -i supabase_db_mt-local psql -U postgres -v ON_ERROR_STOP=1 -q < "$f"
done
docker exec -i supabase_db_mt-local psql -U postgres -q < ~/Projects/money-tracker/supabase/tests/verify.sql
```

Para usar la app contra él, crea un `config.toml` aparte y apunta la app ahí con `MONEY_TRACKER_CONFIG`, para no tocar
tu configuración real:

```toml
# /tmp/mt-local/config.toml
supabase_url = "http://127.0.0.1:54321"
supabase_publishable_key = "<anon key de `supabase status`>"
token_storage = "file"
```

**Sin sesión** (con solo la publishable key, como lo haría cualquiera que la tenga):

```sh
curl -s "$URL/rest/v1/sync_state?select=*" -H "apikey: $KEY" -H "Authorization: Bearer $KEY"
# → []
curl -s -X POST "$URL/rest/v1/rpc/ledger_status" -H "apikey: $KEY" -H "Authorization: Bearer $KEY"
# → {"code":"42501", … "permission denied for function ledger_status"}
```

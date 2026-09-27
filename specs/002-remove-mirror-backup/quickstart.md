# Quickstart: validar 002 de punta a punta

**Branch**: `002-remove-mirror-backup` | **Spec**: [spec.md](spec.md) | **Contracts**: [contracts/](contracts/)

Guía de validación. El orden importa: todo se prueba primero en un **proyecto de Supabase de prueba**
y solo al final se toca producción (research R16).

## 0. Prerrequisitos

- Un segundo proyecto de Supabase, el "de prueba", con su URL y su key publicable, y un usuario creado
  en *Authentication → Users*.
- Binario compilado desde esta rama:
  `cargo build --release -p money-tracker` y reinstalado según el README, sección 1.6.
- Una configuración aislada para no tocar la real. `MONEY_TRACKER_CONFIG` también mueve los respaldos
  y el registro del último respaldo. La sesión del llavero se separa por proyecto (research R15).

```sh
export MONEY_TRACKER_CONFIG=/tmp/mt-prueba/config.toml
money-tracker db remote login --url https://<ref-prueba>.supabase.co --key sb_publishable_…
```

Para no recibir avisos del llavero durante la validación (research R17), agrega
`token_storage = "file"` a `/tmp/mt-prueba/config.toml` antes del `login`. Verificación de SC-010: corre
diez comandos, abre la GUI y recompila (`cargo build --release -p money-tracker`); macOS no debe pedir
contraseña en ningún momento, y `security find-generic-password -s money-tracker` no debe encontrar una
entrada nueva.

## 1. Pruebas automáticas sin red (US4, SC-006, SC-008)

Con el Wi-Fi apagado:

```sh
cargo test --workspace            # todo pasa, < 10 s
cargo clippy --workspace --all-targets
cargo tree -p money_core | grep -i sqlite    # sin salida
cd gui && npx tsc --noEmit
```

Esperado: ninguna prueba intenta conectarse, y las de sobregiro, límite de crédito, una sola
emergencia y presupuesto positivo están en `rules.rs`.

## 2. Esquema en el proyecto de prueba (US3, SC-009)

En el SQL Editor del proyecto de prueba:

1. Pega y corre `supabase/sql/0001_setup.sql`, luego `0002_schema_version.sql`.
2. `select * from public.schema_version;` → `version = 2`.
3. Corre `0002` otra vez → error `… ya aplicado`, y `schema_version` sigue igual.
4. Corre `supabase/tests/verify.sql` → `verify.sql: N/N rechazos confirmados`.

Acceso sin sesión (solo la key publicable):

```sh
curl -s "$URL/rest/v1/sync_state?select=*" -H "apikey: $KEY" -H "Authorization: Bearer $KEY"
#  → []   (antes de 0002 devolvía la revisión)
curl -s -X POST "$URL/rest/v1/rpc/ledger_status" -H "apikey: $KEY" -H "Authorization: Bearer $KEY"
#  → error de permiso (42501)
```

## 3. Uso diario contra el proyecto de prueba (US1, US3)

```sh
money-tracker db remote status          # sesión, revisión, "Esquema: versión 2 (la app espera 2)"
bash scripts/setup_inicial.sh           # cuentas y saldos de prueba
money-tracker add 100 Alimentos --new-concept
money-tracker entry list -p "$(date +%Y-%m)"   # solo movimientos de este mes (research R7)
```

- Tarjeta con límite 3000 y deuda 1900: `add 2000 Discrecional --from tdc` → rechazado con "Exceeds
  credit limit" (antes de 002 se aceptaba).
- `ls ~/.money-tracker/*.db` antes y después → no aparece ni cambia ningún archivo (SC-001).
- Sin configuración: `MONEY_TRACKER_CONFIG=/tmp/vacio/config.toml money-tracker report` → mensaje
  "Supabase no está configurado…", código de salida 1, y no se crea ningún `.db`.
- Esquema desfasado: en el proyecto de prueba, `update public.schema_version set version = 1;` →
  `money-tracker report` falla con "…versión 1 y esta app espera la 2…". Regresarlo a `2`.

## 4. Respaldo manual (US2)

```sh
money-tracker db backup
#  → Respaldo: /tmp/mt-prueba/backups/money-tracker-YYYYMMDD-HHMMSS.sql (N movimientos)
ls -l /tmp/mt-prueba/backups/            # -rw------- (0600)
money-tracker db backup -o /tmp/mt-prueba/backups/<mismo-nombre>.sql   # → error, no sobrescribe
```

## 5. Respaldo automático (US2, SC-007)

```sh
# Simular que el último respaldo tiene 8 días:
sed -i '' 's/^at = .*/at = "2026-01-01T00:00:00-06:00"/' /tmp/mt-prueba/last-backup.toml
money-tracker report          # al final: "Respaldo automático: …"
money-tracker report          # sin aviso: el respaldo es de hoy
# Respaldo automático que falla con un comando exitoso: carpeta sin permiso de escritura
sed -i '' 's/^at = .*/at = "2026-01-01T00:00:00-06:00"/' /tmp/mt-prueba/last-backup.toml
chmod 500 /tmp/mt-prueba/backups
money-tracker report; echo $?   # reporte normal + "Aviso: no se pudo hacer el respaldo automático…"; sale 0
chmod 700 /tmp/mt-prueba/backups
money-tracker report            # ahora sí respalda (se reintentó)
money-tracker db backup         # nunca dispara además el automático
```

## 6. Simulacro de restauración (US2, SC-003, SC-005)

1. En el proyecto de prueba, dejar el esquema limpio: borra el proyecto y créalo de nuevo, o usa un
   tercero. Aplica `0001` y `0002`.
2. Pega en el SQL Editor el respaldo del paso 4. Si el email del usuario es otro, cambia la línea
   `RESTAURAR COMO`. Run.
3. Compara los reportes de todos los períodos contra el origen:

```sh
for p in 2026-08 2026-09; do
  MONEY_TRACKER_CONFIG=/tmp/mt-origen/config.toml  money-tracker report -p $p > /tmp/a-$p.txt
  MONEY_TRACKER_CONFIG=/tmp/mt-prueba/config.toml  money-tracker report -p $p > /tmp/b-$p.txt
  diff /tmp/a-$p.txt /tmp/b-$p.txt && echo "$p idéntico"
done
money-tracker add 50 Alimentos     # la app puede seguir escribiendo (las secuencias quedaron bien)
```

4. Correr el mismo respaldo por segunda vez → "El proyecto no está vacío…", sin cambios.

Meta: todo el procedimiento en menos de 15 minutos, sin contar la creación del proyecto.

## 7. GUI (US1, US2)

```sh
cd gui && pnpm tauri dev
```

- Sin configuración → pantalla **Conexión** (URL, key, email, contraseña), sin pánico al arrancar.
- Tras conectar → SetupWizard si no hay cuentas; si hay, pestañas.
- Registrar un gasto desde el CLI → aparece en la GUI en menos de 60 s. Borrarlo con
  `entry rm` → desaparece en menos de 60 s (research R6, punto 6).
- **Ajustes → Respaldo → Respaldar ahora** → muestra ruta y fecha. Ya no aparece nada de "espejo".
- Con el registro envejecido, al abrir la GUI → aviso de respaldo automático.

## 8. Puesta en producción (research R16)

Solo después de que los pasos 1–7 pasen:

1. **Copia previa**. En el SQL Editor de producción, corre esta consulta de solo lectura y guarda el
   resultado en un archivo:

   ```sql
   select jsonb_build_object(
     'concepts', (select jsonb_agg(c) from public.concepts c),
     'accounts', (select jsonb_agg(a) from public.accounts a),
     'entries',  (select jsonb_agg(e) from public.entries  e),
     'budgets',  (select jsonb_agg(b) from public.budgets  b),
     'config',   (select jsonb_agg(k) from public.config   k));
   ```

2. Aplica `0002_schema_version.sql` en producción → `schema_version = 2`.
3. Instala el binario nuevo (README 1.6) e inicia sesión una vez (`db remote login`; research R15).
4. `money-tracker db remote status` → esquema 2/2. Luego `money-tracker db backup`.
5. Pendiente aparte (fuera de alcance): decidir si las ediciones de los movimientos #105–#107 se
   aplican con `entry edit`.
6. Opcional: borrar `~/.money-tracker/data.db`, y en el home los `money-tracker-backup-*.db*`.

#!/usr/bin/env bash
#
# migrar_a_supabase.sh — sube tu base local actual a Supabase y reconstruye el
# archivo local como espejo del remoto (remapeando ids en el proceso).
#
# Requisitos (una sola vez, en el dashboard de Supabase):
#   - Proyecto con el esquema aplicado: SQL Editor → pegar 0001_initial.sql → Run.
#   - Un usuario en Authentication → Users → Add user (email + password).
#
# Las claves NO se hardcodean aquí (este archivo se versiona en git): se piden
# por prompt, o se toman de MONEY_TRACKER_SUPABASE_URL / _KEY / MONEY_TRACKER_DB
# si ya las tienes exportadas.
#
# Uso:
#   ./scripts/migrar_a_supabase.sh                 # flujo normal
#   MIGRATE_FORCE=1 ./scripts/migrar_a_supabase.sh # remoto ya tiene filas: añade (no fusiona)
#
# ANTES DE CORRER: cierra la GUI (no debe haber otra app escribiendo sobre el
# archivo local mientras se migra).
#
set -euo pipefail

# ---------------------------------------------------------------------------
# 0. Ir a la raíz del repo (donde están Cargo.toml, target/ y el binario).
# ---------------------------------------------------------------------------
while [ ! -f Cargo.toml ] && [ "$(pwd)" != "/" ]; do cd ..; done
[ -f Cargo.toml ] || { echo "✗ No encuentro la raíz del repo (Cargo.toml)." >&2; exit 1; }

# ---------------------------------------------------------------------------
# 0. Binario. Usa el instalado o el de target/release; si no está, lo compila.
# ---------------------------------------------------------------------------
MT=""
if [ -x target/release/money-tracker ] && target/release/money-tracker db --help 2>&1 | rg -q "remote"; then
  MT="target/release/money-tracker"
elif command -v money-tracker >/dev/null 2>&1 && money-tracker db --help 2>&1 | rg -q "remote"; then
  MT="money-tracker"
fi

# Si ninguno de los binarios existentes soporta `db remote`, los recompila.
if [ -z "$MT" ]; then
  echo "▶ Compilando release con soporte remoto..."
  cargo build --release -p money-tracker
  MT="target/release/money-tracker"
fi

# ---------------------------------------------------------------------------
# 1. Empezar por la base local a migrar.
# ---------------------------------------------------------------------------
DB_SOURCE="${MONEY_TRACKER_DB:-$HOME/.money-tracker/data.db}"
if [ ! -f "$DB_SOURCE" ]; then
  echo "✗ No existe base local en: $DB_SOURCE" >&2
  echo "  (Exporta MONEY_TRACKER_DB si tu base está en otra ruta.)" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. Credenciales. Si ya exportaste las envs, se usan tal cual; si no, se piden.
# ---------------------------------------------------------------------------
if [ -z "${MONEY_TRACKER_SUPABASE_URL:-}" ]; then
  read -rp "URL de Supabase ( https://<ref>.supabase.co ): " SUPABASE_URL
else
  SUPABASE_URL="$MONEY_TRACKER_SUPABASE_URL"
fi
if [ -z "${MONEY_TRACKER_SUPABASE_KEY:-}" ]; then
  read -rp "Publishable (anon) key: " SUPABASE_KEY
else
  SUPABASE_KEY="$MONEY_TRACKER_SUPABASE_KEY"
fi
if [ -z "${SUPABASE_URL:-}" ] || [ -z "${SUPABASE_KEY:-}" ]; then
  echo "✗ URL y key son obligatorias." >&2
  exit 1
fi

read -rp "Email del usuario de Authentication: " SUPABASE_EMAIL
[ -n "$SUPABASE_EMAIL" ] || { echo "✗ Email obligatorio." >&2; exit 1; }

# ---------------------------------------------------------------------------
# 3. Respaldo de seguridad antes de tocar nada.
# ---------------------------------------------------------------------------
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="$HOME/money-tracker-backup-$STAMP.db"
cp "$DB_SOURCE" "$BACKUP"
[ -f "$DB_SOURCE-wal" ] && cp "$DB_SOURCE-wal" "$BACKUP-wal" || true
[ -f "$DB_SOURCE-shm" ] && cp "$DB_SOURCE-shm" "$BACKUP-shm" || true
echo "✓ Respaldo: $BACKUP"

# ---------------------------------------------------------------------------
# 4. Login (guarda refresh token en el llavero) y migración.
#    La contraseña la pide el propio CLI en pantalla (prompt seguro).
# ---------------------------------------------------------------------------
# ---------------------------------------------------------------------------
# 4. Login SOLO si no hay sesión activa ya (keychain/config.toml). El refresh
#    token del keychain se reusa automáticamente al migrar; esto evita pedir
#    la contraseña cada que corres el script.
# ---------------------------------------------------------------------------
export MONEY_TRACKER_SUPABASE_URL="$SUPABASE_URL"
export MONEY_TRACKER_SUPABASE_KEY="$SUPABASE_KEY"
export MONEY_TRACKER_DB="$DB_SOURCE"

if "$MT" db remote status 2>&1 | rg -q "Sesión: activa"; then
  echo "✓ Sesión remota ya activa (keychain) — no se pide contraseña."
else
  echo "▶ Iniciando sesión (escribe tu contraseña cuando el CLI la pida)..."
  "$MT" db remote login "$SUPABASE_EMAIL"
fi

echo "▶ Migrando base local → Supabase..."
if [ "${MIGRATE_FORCE:-0}" = "1" ]; then
  "$MT" db remote migrate --yes --force
else
  "$MT" db remote migrate --yes
fi

echo "▶ Estado final:"
"$MT" db remote status

echo
echo "✓ Listo. Tu base local quedó como espejo del remoto."
echo "  Respaldo disponible en: $BACKUP"
echo "  Borra el respaldo solo cuando confirmes que el reporte se ve bien:"
echo "  $MT report --detail"
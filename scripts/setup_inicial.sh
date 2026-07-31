#!/usr/bin/env bash
#
# setup_inicial.sh — carga inicial de money-tracker, arrancando desde cero en
# agosto 2026. El Excel legado (Dashboard_Financiero.xlsx) no se migra: julio
# quedó incompleto ahí (se dejó de operar a la mitad del mes) y el dashboard ya
# se había vuelto engorroso de mantener, así que se parte de los montos
# actuales conocidos en vez de reconstruir el historial.
#
# No se seedea deuda de tarjeta (está en $0) ni un gasto de hoy que no se
# alcanzó a registrar — agosto arranca limpio.
#
# Los montos NO se hardcodean aquí por seguridad (este archivo se versiona en
# git) — llena las variables de la sección 0 antes de correrlo.
#
# Uso:
#   chmod +x scripts/setup_inicial.sh
#   MONEY_TRACKER_DB=/tmp/prueba.db ./scripts/setup_inicial.sh   # primero en un archivo descartable
#   ./scripts/setup_inicial.sh                                    # ya validado, contra ~/.money-tracker/data.db
#
set -euo pipefail

# Si no instalaste el binario (cp a /usr/local/bin), usa:
#   MT="cargo run -p money-tracker --"
MT="money-tracker"

FECHA_SETUP="2026-08-01"   # arranca el período de agosto (hoy es 2026-07-31)

# ---------------------------------------------------------------------------
# 0. AJUSTA estos montos antes de correr el script (no se hardcodean por
#    seguridad, ver nota arriba). --limit es un tope propio que la app hace
#    cumplir, no tiene que ser el límite real que te dio el banco — déjalo
#    bajo si no piensas usar la tarjeta seguido.
# ---------------------------------------------------------------------------
FONDO_EMERGENCIA=0   # saldo inicial del fondo de emergencia
DEBITO_INICIAL=0     # saldo inicial en debito
VALES_INICIAL=0      # saldo inicial en vales
TDC_LIMITE=0         # tope propio de la tarjeta de crédito

# ---------------------------------------------------------------------------
# 1. Cuentas de gasto (kind=spending)
# ---------------------------------------------------------------------------
$MT account add efectivo --kind spending
$MT account add debito   --kind spending
$MT account add vales    --kind spending --restricted   # vales de despensa: no dispara el aporte a emergencia

# ---------------------------------------------------------------------------
# 2. Fondo de emergencia (único bucket, kind=emergency)
# ---------------------------------------------------------------------------
$MT account add "Fondo de emergencia" --kind emergency

# ---------------------------------------------------------------------------
# 3. Tarjeta de crédito, saldo en $0 (limpia). credit_limit debe ser > 0 (o
#    ausente) — si dejas TDC_LIMITE en 0, se crea sin tope.
# ---------------------------------------------------------------------------
if [ "$TDC_LIMITE" != "0" ]; then
  $MT account add tdc --kind credit --limit "$TDC_LIMITE"
else
  $MT account add tdc --kind credit --yes   # sin --limit: --yes evita que pregunte por un tope opcional
fi

# ---------------------------------------------------------------------------
# 4. Configuración por defecto
# ---------------------------------------------------------------------------
$MT config set default_account debito
$MT config set income_account debito
$MT config set cash_concept Discrecional   # concepto usado por `account reconcile efectivo`
$MT config set emergency_pct 10

# ---------------------------------------------------------------------------
# 5. Saldos iniciales (setup), con fecha de arranque de agosto.
# ---------------------------------------------------------------------------
$MT setup \
  --account "Fondo de emergencia"="$FONDO_EMERGENCIA" \
  --account debito="$DEBITO_INICIAL" \
  --account vales="$VALES_INICIAL" \
  --account efectivo=0 \
  -D "$FECHA_SETUP" \
  --yes

echo "✓ Setup inicial completo. Revisa con: $MT report -p 2026-08 --detail"

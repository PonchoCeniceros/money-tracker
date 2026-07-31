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

FECHA_SETUP="2026-08-01" # arranca el período de agosto (hoy es 2026-07-31)

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
PATRIMONIO_INICIAL=0 # saldo inicial en el bucket "Patrimonio" (ahorro sin meta específica)
GASTO_HISTORICO=0    # ver nota en la sección 4 — respaldo para "meses de colchón" (0 = sin respaldo)

# ---------------------------------------------------------------------------
# 1. Cuentas de gasto (kind=spending)
# ---------------------------------------------------------------------------
$MT account add efectivo --kind spending
$MT account add debito --kind spending
$MT account add vales --kind spending --restricted # vales de despensa: no dispara el aporte a emergencia

# ---------------------------------------------------------------------------
# 2. Fondo de emergencia (único bucket, kind=emergency)
#
#    Cobertura de referencia al momento del go-live (gasto promedio real de
#    abril-junio 2026 del Excel legado — los 3 meses completos; julio se
#    excluyó por estar incompleto, igual que hace el propio cálculo de
#    "meses de colchón" del Dashboard con meses sin actividad):
#
#      Gasto promedio:  ($9,737.51 + $13,511.18 + $11,886.84) / 3 = $11,711.84/mes
#      Fondo unificado (Fondo de emergencia + cajitas de Nu, ya no separadas
#      en la app): $40,000 / $11,711.84 = 3.42 meses de cobertura
#
#    3.42 meses queda en el borde bajo del rango típico recomendado (3-6
#    meses) — no es alarmante, pero tampoco hay margen amplio. Para llegar a
#    6 meses completos con este mismo ritmo de gasto haría falta ~$70,271.
# ---------------------------------------------------------------------------
$MT account add "Fondo de emergencia" --kind emergency

# ---------------------------------------------------------------------------
# 3. Tarjeta de crédito, saldo en $0 (limpia). credit_limit debe ser > 0 (o
#    ausente) — si dejas TDC_LIMITE en 0, se crea sin tope.
# ---------------------------------------------------------------------------
if [ "$TDC_LIMITE" != "0" ]; then
  $MT account add tdc --kind credit --limit "$TDC_LIMITE"
else
  $MT account add tdc --kind credit --yes # sin --limit: --yes evita que pregunte por un tope opcional
fi

# ---------------------------------------------------------------------------
# 3b. Bucket "Patrimonio" — acumulación de ahorro sin meta específica, aparte
#     del fondo de emergencia (que sigue creciendo con su % automático sin
#     tope). Sin --target: es un bucket abierto, sin límite ni meta — el % de
#     avance no aplica aquí (se muestra "—"), solo acumula.
#
#     No hay comando para editar target_amount después de crear la cuenta, y
#     archivar no libera el nombre (UNIQUE incluso archivada) — si algún día
#     quieres ponerle una meta, es un `UPDATE accounts SET target_amount = ...
#     WHERE name = 'Patrimonio'` directo en SQLite, no hay otra forma hoy.
# ---------------------------------------------------------------------------
$MT account add Patrimonio --kind target --yes

# ---------------------------------------------------------------------------
# 4. Configuración por defecto
#
#    baseline_monthly_expense: respaldo para "Meses de colchón" del Dashboard
#    MIENTRAS no haya ningún mes real registrado en la app — en cuanto exista
#    uno, se ignora por completo, nunca se mezcla con datos reales. Útil para
#    no ver "—" en los primeros días después del go-live. Referencia de este
#    go-live: gasto promedio real de abril-junio 2026 (Excel legado, los 3
#    meses completos) = ($9,737.51 + $13,511.18 + $11,886.84) / 3 = $11,711.84.
#    Déjalo en 0 para no sembrar ningún respaldo.
# ---------------------------------------------------------------------------
$MT config set default_account debito
$MT config set income_account debito
$MT config set cash_concept Discrecional # concepto usado por `account reconcile efectivo`
$MT config set emergency_pct 10
if [ "$GASTO_HISTORICO" != "0" ]; then
  $MT config set baseline_monthly_expense "$GASTO_HISTORICO"
fi

# ---------------------------------------------------------------------------
# 5. Saldos iniciales (setup), con fecha de arranque de agosto.
# ---------------------------------------------------------------------------
$MT setup \
  --account "Fondo de emergencia"="$FONDO_EMERGENCIA" \
  --account debito="$DEBITO_INICIAL" \
  --account vales="$VALES_INICIAL" \
  --account Patrimonio="$PATRIMONIO_INICIAL" \
  --account efectivo=0 \
  -D "$FECHA_SETUP" \
  --yes

echo "✓ Setup inicial completo. Revisa con: $MT report -p 2026-08 --detail"

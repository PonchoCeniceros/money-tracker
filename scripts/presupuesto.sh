#!/usr/bin/env bash
#
# presupuesto.sh — carga el presupuesto mensual sugerido en la conversación,
# basado en el gasto real de abril-julio del Excel legado:
#
#   Alimentos       ~$2,500  (promedio/mediana real ~$2,383-2,413)
#   Transporte      ~$1,000  (rango típico $900-1,150)
#   Servicios       ~$1,600  (incluye agua/luz de julio no capturados en el Excel)
#   Discrecional    ~$3,300  (cerca del promedio real $3,416; sin recorte artificial)
#   Extraordinario  ~$2,000  (colchón de planeación, no un límite duro — es el
#                             rubro más irregular por naturaleza)
#
# Los montos NO se hardcodean por seguridad (este archivo se versiona en git)
# — llena las variables de la sección 0 antes de correrlo. `budget set` es
# idempotente por (concepto, período): correrlo de nuevo actualiza el límite
# en vez de duplicarlo.
#
# Uso:
#   chmod +x scripts/presupuesto.sh
#   MONEY_TRACKER_DB=/tmp/prueba.db ./scripts/presupuesto.sh   # primero en un archivo descartable
#   ./scripts/presupuesto.sh                                    # ya validado, contra ~/.money-tracker/data.db
#
set -euo pipefail

# Si no instalaste el binario (cp a /usr/local/bin), usa:
#   MT="cargo run -p money-tracker --"
MT="money-tracker"

# ---------------------------------------------------------------------------
# 0. AJUSTA estos montos antes de correr el script. Deja un concepto en 0 para
#    omitirlo (no se manda `budget set` para ese concepto).
# ---------------------------------------------------------------------------
PERIODO="2026-08"
ALIMENTOS=0
TRANSPORTE=0
SERVICIOS=0
DISCRECIONAL=0
EXTRAORDINARIO=0

# ---------------------------------------------------------------------------
# 1. Cargar cada concepto que quedó en un monto > 0.
# ---------------------------------------------------------------------------
set_budget() {
  local concepto="$1"
  local monto="$2"
  if [ "$monto" != "0" ]; then
    $MT budget set -c "$concepto" -l "$monto" -p "$PERIODO"
  fi
}

set_budget Alimentos "$ALIMENTOS"
set_budget Transporte "$TRANSPORTE"
set_budget Servicios "$SERVICIOS"
set_budget Discrecional "$DISCRECIONAL"
set_budget Extraordinario "$EXTRAORDINARIO"

echo "✓ Presupuesto cargado para $PERIODO. Revisa con: $MT budget show -p $PERIODO"

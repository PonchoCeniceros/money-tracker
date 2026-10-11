# Bugs detectados y corregidos — septiembre/octubre 2026

Tres defectos encontrados durante los cierres contables de septiembre y octubre de 2026, validados
contra un Supabase local reconstruido con datos reales. Ninguno se reprodujo en producción.

**Los tres están corregidos.** Este documento queda como registro del diagnóstico, porque en dos de
los tres la causa resultó ser distinta de la que parecía al principio.

| # | Bug | Severidad | Estado |
|---|---|---|---|
| 1 | La gráfica "Distribución real" cuenta doble el ahorro | alta | corregido |
| 2 | `reconcile` rechaza conceptos de tipo `income` | media | corregido |
| 3 | El reporte imprime `-0.00` | baja | corregido |

---

## Bug 1 — La gráfica "Distribución real" contaba doble el ahorro

**Dónde:** `gui/src/routes/Budgets.tsx`

### Qué pasaba

`emergencyContribution` y `targetContribution` filtraban las transferencias del período mirando
**solo la cuenta destino**, nunca el origen:

```ts
const targetContribution = (transfers.data ?? [])
  .filter((e) => e.to_account && accountKindByName.get(e.to_account) === "target")
  .reduce((sum, e) => sum + e.amount, 0);
```

El comentario que tenía encima declaraba la intención correcta —contar transferencias
`spending -> emergency/target`— pero el código nunca revisaba el origen. Consecuencia: una
transferencia **entre dos cuentas de ahorro** se contaba como aportación nueva, cuando es dinero
que ya estaba ahorrado.

### Reproducción

Ledger con una transferencia de $3,000.00 de `Fondo de emergencia` a `Patrimonio` dentro del período:

| Fuente | Aportes a ahorro |
|---|---|
| `report_service::monthly_report` (CLI) | $9,503.43 ✅ |
| Gráfica "Distribución real" (GUI) | $12,503.43 ❌ |

La diferencia era exactamente los $3,000.00 de la transferencia interna. Al inflar el ahorro,
**todas** las rebanadas de gasto se encogían proporcionalmente:

| Concepto | Correcto | Mostrado |
|---|---|---|
| Metas de ahorro | 34% | 42% |
| Discrecional | 20% | 18% |
| Alimentos | 15% | 13% |
| Transporte | 12% | 10% |

### Causa raíz

La GUI **recalculaba** los aportes desde la lista cruda de transferencias, en lugar de consumir el
`savings_contributions` que `money_core` ya exponía y que sí aplicaba la regla correcta. Dos
implementaciones de la misma regla de negocio que divergieron — exactamente lo que la arquitectura
del proyecto busca evitar.

### Cómo se corrigió

`ledger::monthly_report` ahora calcula el desglose con el filtro de origen correcto y lo expone en
dos campos nuevos de `MonthlyReport`:

```rust
let contribution_to = |kind: &str| -> f64 { /* from == "spending" && to == kind */ };
let emergency_contributions = contribution_to("emergency");
let target_contributions = contribution_to("target");
let savings_contributions = emergency_contributions + target_contributions;
```

La GUI los consume directamente y dejó de derivar nada. De paso se eliminaron dos llamadas IPC
(`accountsApi.list()` y `entriesApi.list({kind:"transfer"})`) que ya no alimentaban nada.

**Test:** `report_service::tests::savings_to_savings_transfer_is_not_a_contribution`, que además
verifica que las dos mitades sumen el total.

**Verificación con datos reales** (septiembre 2026): `emergencia=2369.53 metas=7133.90 suma=9503.43`,
idéntico al `savings_contributions` del reporte.

---

## Bug 2 — `reconcile` rechazaba conceptos de tipo `income`

**Dónde:** `cli/src/commands/account.rs` y `cli/src/commands/helpers.rs`

### Qué pasaba

Con un concepto existente de tipo `income`:

```
$ money-tracker account reconcile Patrimonio --actual 20232.31 --concept Rendimientos
Error: Not found: Concepto 'Rendimientos' no existe.
       ¿Quisiste decir 'Alimentos'? Usa `concept add` para crear uno nuevo.
```

El concepto **sí existía**. El handler resolvía siempre contra conceptos de gasto:

```rust
let concept = helpers::resolve_concept(&*be, &concept, "expense")?;
```

Pero `reconcile` escribe un asiento cuya dirección depende del signo de la diferencia: si el saldo
real es **mayor** que el derivado escribe un `income`, si es **menor** un `expense`. Registrar
rendimientos de una cuenta de ahorro cae siempre en el primer caso, y ahí un concepto de ingreso es
exactamente lo correcto.

### Problema adicional: el mensaje mentía

Decía *"no existe"* y sugería crearlo. Un usuario que seguía la sugerencia recibía un error de clave
duplicada, porque el concepto ya estaba ahí — solo era del tipo equivocado.

### Cómo se corrigió

El handler calcula la dirección antes de resolver:

```rust
let needed_type = if account.balance - actual > 0.0 { "expense" } else { "income" };
```

Y `resolve_concept` distingue los dos errores: si el nombre existe entre *todos* los conceptos pero
no pasó el filtro de tipo, lo dice explícitamente en vez de fingir que no existe.

**Tests:** cuatro casos nuevos en `cli/src/commands/helpers.rs` — `both` en ambas direcciones,
`income` aceptado/rechazado, `expense` rechazado para ingreso, y que un nombre realmente inexistente
siga sugiriendo el más parecido. Son los primeros tests del crate `cli`.

---

## Bug 3 — El reporte imprimía `-0.00`

**Dónde:** `money_core/src/storage/mod.rs` (`round2`)

### Qué pasaba

```
$ money-tracker report --detail
   Gasto del mes (devengado): $3231.33
    pagado con flujo del mes  1475.00
       financiado con ahorro  -0.00          ← debería ser 0.00
                   a crédito  1756.33
```

### La hipótesis inicial estaba equivocada

El diagnóstico original de este documento suponía que el binario instalado no correspondía al código
de `main`, porque leyendo la fuente el valor debía ser `0.00`: `from_savings` es una suma de montos
positivos (`CHECK amount > 0`) sobre un conjunto que estaba vacío.

**Era falso.** Recompilando desde `main` el `-0.00` se reproducía igual.

### Causa raíz real

Instrumentando el valor:

```
DBG raw from_savings neg=true bits=0x8000000000000000 count=0
```

Cero elementos, y aun así cero **negativo** antes de llegar a `round2`. La razón es que la
implementación de `Iterator::sum` para flotantes en Rust acumula desde `-0.0`, no desde `0.0`:

```rust
fn sum<I: Iterator<Item = f64>>(iter: I) -> f64 { iter.fold(-0.0, Add::add) }
```

Es correcto: `-0.0` es la identidad aditiva en IEEE 754, porque `-0.0 + x == x` para todo `x`,
incluido `+0.0`. Pero implica que **toda suma vacía de `f64` en el proyecto nacía como cero
negativo**, y `round2` lo propagaba: `(-0.0 * 100.0).round() / 100.0` conserva el signo.

### Cómo se corrigió

En `round2`, que es por donde pasa toda cifra monetaria derivada:

```rust
(v * 100.0).round() / 100.0 + 0.0
```

El `+ 0.0` convierte `-0.0` en `0.0` y es inocuo para cualquier otro valor. Se corrigió ahí y no en
el formateo del reporte para que proteja a todos los consumidores —CLI, GUI y respaldos— y no solo
a esa línea.

**Test:** `storage::tests::round2_normalizes_negative_zero`, que primero afirma la precondición
(la suma vacía es `-0.0`) para que el test documente el porqué.

---

## Observación transversal

Los bugs 1 y 3 comparten una forma: **una regla implementada dos veces**, o **una función de
redondeo que no normalizaba**. En ambos casos el arreglo fue mover la corrección al punto más
central posible —`money_core` y `round2`— en lugar de parchar el consumidor que la manifestaba.

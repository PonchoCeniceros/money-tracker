# Feature Specification: Supabase como fuente de la base de datos

**Feature Branch**: `001-supabase-backend`

**Created**: 2026-09-17

**Status**: Draft

**Input**: User description: "quiero implementar supabase en como fuente de la base de datos del proyecto."

## Clarifications

### Session 2026-09-17

- Q: ¿Se conserva la base SQLite local como base de sincronización para protegerte ante congelamiento
  o pérdida de tu plan de Supabase? → A: Sí, el SQLite local se conserva como base de sincronización
  y respaldo del libro mayor alojado.
- Q: ¿El SQLite local debe funcionar solo como espejo/respaldo o permitir operar sin conexión? → A:
  Solo como espejo/respaldo: Supabase es la única vía de escritura y el SQLite local replica el libro
  mayor.
- Q: ¿Cuándo debe actualizarse el espejo local SQLite? → A: Tras cada operación exitosa (tiempo
  real): antes de terminar el comando, la copia local ya refleja la fuente alojada.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Base de datos alojada como única fuente de verdad (Priority: P1)

El usuario mantiene su libro mayor financiero en un solo lugar remoto. Todas las operaciones que hoy
usa (registrar gastos e ingresos, transferencias, manejo de cuentas y buckets, cuadre de efectivo,
presupuestos, reporte mensual y configuración) funcionan de forma idéntica contra la base de datos
alojada, y la base SQLite local se conserva como base de sincronización y respaldo del mismo libro
mayor. El reporte mensual arroja exactamente los mismos números que antes, con la misma semántica
(gasto devengado vs salida real de efectivo).

**Why this priority**: Es el corazón de la mitad del pedido — sin esta historia no hay nada nuevo
que evaluar. Todo lo demás (dispositivos múltiples, migración) cuelga de que las operaciones
actuales sigan siendo correctas contra la nueva fuente.

**Independent Test**: Registrar un gasto, un ingreso con reparto automático de emergencia, una
transferencia y un `setup` de saldos iniciales en una base de datos nueva; ejecutar `report` y
comparar contra los resultados del comportamiento local actual con los mismos datos.

**Acceptance Scenarios**:

1. **Given** una base de datos alojada nueva y las cuentas creadas, **When** el usuario registra
   gastos, ingresos y transferencias, **Then** el reporte mensual muestra las mismas cifras y
   desgloses que el sistema anterior con datos equivalentes.
2. **Given** un ingreso en una cuenta líquida con un fondo de emergencia activo, **When** se registra
   el ingreso, **Then** el porcentaje configurado se separa automáticamente al fondo de emergencia,
   igual que hoy.
3. **Given** una cuenta de crédito con deuda, **When** el usuario paga la tarjeta mediante una
   transferencia, **Then** el pago no se cuenta como gasto del mes (devengado ni salida real de
   efectivo), idéntico al comportamiento actual.

---

### User Story 2 - Acceso desde cualquier lugar y dispositivo (Priority: P2)

El usuario abre el mismo libro mayor desde distintas instalaciones (por ejemplo, la PC de su casa y
otra máquina, o el CLI y la GUI) y siempre ve la misma información actualizada. Un cambio hecho desde
la GUI aparece en el CLI sin pasos manuales de copiado, y viceversa, sin perder ni duplicar datos.

**Why this priority**: Es el beneficio principal de unificar los datos en un servicio alojado: dejar
de depender de un solo archivo en una sola máquina.

**Independent Test**: Registrar un movimiento desde una instalación y verificar que una segunda
instalación lo refleja en su lista de movimientos y en el reporte.

**Acceptance Scenarios**:

1. **Given** dos instalaciones del sistema conectadas al mismo libro mayor, **When** una registra un
   gasto, **Then** la otra muestra el movimiento y su efecto en saldos en menos de 1 minuto.
2. **Given** un reporte generado en una instalación, **When** se genera el mismo período en otra,
   **Then** ambas reportan cifras idénticas.

---

### User Story 3 - Migración de los datos existentes (Priority: P3)

El usuario lleva meses registrando movimientos en su base local. Al activar la fuente alojada, su
historial (cuentas, movimientos, presupuestos, conceptos, configuración) se traslada completo, con
un solo paso explícito, y los saldos derivados no cambian.

**Why this priority**: La fiducia del usuario depende de no perder su historial; pero si el usuario
prefiere empezar limpio, la historia puede diferirse. Por eso es P3.

**Independent Test**: Migrar una base local con datos de prueba y comparar el reporte de cada período
antes/después de la migración.

**Acceptance Scenarios**:

1. **Given** un archivo local con cuentas, movimientos y presupuestos, **When** el usuario ejecuta la
   migración explícita, **Then** todos los movimientos, saldos y presupuestos quedan disponibles en la
   fuente alojada con cifras idénticas.
2. **Given** una base local con el esquema legacy (pre-rediseño), **When** el usuario intenta migrar,
   **Then** se rechaza con un error accionable, replicando la política actual.

---

### Edge Cases

- ¿Qué pasa cuando dos instalaciones modifican el mismo movimiento (o cuentas, presupuestos)
  casi al mismo tiempo? Dado que toda escritura pasa por la fuente alojada, la resolución ocurre ahí
  (última escritura ganadora) y el espejo local refleja el estado final sin duplicar dinero.
- ¿Cómo se comporta el sistema si no hay conexión al momento de una operación de escritura?
- ¿Qué pasa si las credenciales caducan o son revocadas a mitad de una sesión de uso?
- ¿Qué pasa durante la migración si el archivo local tiene movimientos con fechas futuras?
- ¿Qué pasa si la base alojada está indisponible (interrupción del servicio)?
- ¿Qué pasa si el plan de Supabase se congela o la información alojada se pierde? El espejo local
  SQLite conserva el historial y permite recuperarlo completo.
- ¿Qué pasa si la copia local se desactualiza respecto de la fuente alojada? El espejo local se
  re-sincroniza desde Supabase antes de cada recuperación, sin inventar datos.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: El sistema DEBE proveer toda operación contable actual (gasto, ingreso con reparto
  automático de emergencia, transferencia, saldo inicial, cuadre de cuenta, presupuestos, reporte
  mensual) contra una base de datos alojada, sin cambios de comportamiento ni de cifras.
- **FR-002**: El CLI y la GUI DEBEN leer y escribir la misma fuente alojada, de modo que sus
  resultados coincidan sin pasos manuales de exportación/importación.
- **FR-003**: El sistema DEBE propagar los cambios hechos desde cualquier instalación al resto de las
  instalaciones conectadas sin intervención manual. El modelo es de un solo usuario: una misma
  persona accede al mismo libro mayor desde varias instalaciones (por ejemplo, CLI en una máquina y
  GUI en otra), sin roles ni cuentas compartidas.
- **FR-004**: El sistema DEBE verificar la identidad del único usuario antes de permitir lectura o
  escritura de datos financieros, y NO DEBE exponer los datos a solicitudes no autenticadas.
- **FR-005**: El sistema DEBE mantener en la fuente alojada las reglas de integridad hoy garantizadas
  por el esquema local (un solo fondo de emergencia activo, montos siempre positivos con dirección
  por tipo de entrada, forma de las entradas por tipo, `target_amount` solo en cuentas `target`).
- **FR-006**: Los saldos DEBEN seguir derivándose del libro mayor (nunca almacenarse) contra la
  fuente alojada; la configuración (`emergency_pct`, `default_account`, `income_account`,
  `cash_concept`) DEBE mantenerse en la misma fuente.
- **FR-007**: El sistema DEBE ofrecer una migración explícita y de un solo paso del historial local
  (cuentas, movimientos, presupuestos, conceptos, configuración) que preserve las cifras y rechace
  bases con esquema legacy con un error accionable.
- **FR-008**: El sistema exige conexión permanente: toda operación de lectura o escritura DEBE
  ocurrir contra la fuente alojada en línea. Ante falta de conectividad, el sistema DEBE rechazar la
  operación con un mensaje claro y NO DEBE mutar ni corromper datos a nivel local.
- **FR-009**: El sistema DEBE conservar la base SQLite local como espejo/respaldo del libro mayor:
  Supabase es la única vía de lectura/escritura para operar y el SQLite local DEBE actualizarse tras
  cada operación exitosa (tiempo real), de modo que ante congelamiento o pérdida del plan de Supabase
  el usuario conserve íntegro su historial y pueda recuperarlo.

### Key Entities *(include if feature involves data)*

- **Cuenta**: Cuentas de gasto, fondo de emergencia, buckets y tarjetas de crédito; cada una con su
  tipo y atributos opcionales (meta, límite, restricción de liquidez). Se conserva la regla de una
  sola cuenta de emergencia activa.
- **Entrada**: Todo movimiento de dinero (ingreso, gasto, transferencia, saldo inicial); los saldos
  se derivan de la suma de entradas. Aplica al libro mayor del único usuario.
- **Presupuesto**: Límite informativo mensual por concepto; nunca bloquea gastos.
- **Concepto y Configuración**: Catálogo de conceptos y claves de configuración de comportamiento
  del sistema.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: El 100% de las operaciones disponibles hoy funciona contra la fuente alojada con
  resultados idénticos (ninguna regresión de comportamiento verificada con el mismo conjunto de
  datos de prueba).
- **SC-002**: Un cambio hecho desde una instalación se refleja en otra en menos de 1 minuto, sin
  pasos manuales.
- **SC-003**: La migración traslada el 100% de los registros existentes sin reingreso manual; los
  reportes de cada período antes y después de migrar arrojan cifras idénticas.
- **SC-004**: Ningún dato se pierde ni se duplica ante escrituras concurrentes o interrupciones de
  conectividad (cero incidentes de corrupción en las pruebas).
- **SC-005**: La base SQLite local de sincronización contiene una réplica íntegra actualizada tras
  cada operación exitosa; ante una pérdida simulada de la fuente alojada, el historial se recupera
  completo desde la copia local.
- **SC-006**: Las reglas contables existentes (reparto de emergencia, neutralidad de transferencias,
  exclusión de `opening` de ingresos) permanecen verificadas por la misma batería de pruebas que hoy
  las cubre.

## Assumptions

- El modelo es de un solo usuario: la misma persona accede al mismo libro mayor desde varias
  instalaciones; no hay roles ni cuentas compartidas.
- El sistema exige conexión permanente a la red para las operaciones; la base SQLite local se
  conserva como espejo/respaldo del libro mayor (nunca recibe escrituras directas durante la
  operación normal) y Supabase es la única fuente de verdad.
- La base SQLite local de sincronización también sirve para pruebas automatizadas y verificación
  manual con `MONEY_TRACKER_DB`; la experiencia normal de uso apunta a la fuente alojada.
- Las claves de configuración actuales se conservan tal cual en la fuente alojada y siguen
  respetadas por todas las operaciones.
- Los saldos, presupuestos y conceptos existentes se consideran historial valioso y DEBEN
  conservarse mediante la migración explícita de un solo paso.
- El comportamiento y la semántica financiera actuales (devengado vs salida real de efectivo) son la
  referencia innegociable y no cambian con el traslado de la fuente.
- Requiere una cuenta de servicio de base de datos alojada provista por el usuario (credenciales
  fuera del repositorio, nunca versionadas).
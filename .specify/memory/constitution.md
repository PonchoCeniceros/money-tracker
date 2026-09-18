# Constitución de money-tracker

## Principios Centrales

### I. Modelo Primero (money_core Es una Librería Pura)
Todo feature de dominio, servicio y regla de datos DEBE vivir en `money_core`; los handlers de
presentación NO DEBEN reimplementar lógica de negocio. `money_core` DEBE permanecer libre de
preocupaciones de presentación: sin imprimir, sin preguntar, sin parsear argumentos, sin formatear
salida. Un build de `cargo tree -p money_core` NO DEBE jamás resolver dependencias específicas de
CLI/GUI (`clap`, `dialoguer`, `tabled`, `tauri`). Razón: la corrección contable es auditable en
exactamente un lugar, y todo handler (CLI, GUI) la hereda por construcción.

### II. Handlers de Presentación Delgados (CLI y GUI Envuelven el Core)
El binario `cli` y el backend Tauri de `gui` DEBEN ser handlers delgados: cada comando DEBE mapearse
a exactamente una llamada de servicio de `money_core`, adaptando estado y errores (`AppState`,
`ApiError`) en lugar de añadir lógica. Ningún feature DEBE implementarse en un handler a menos que
exista en `money_core`. Todo handler que persista datos DEBE compartir el mismo libro mayor (modo
WAL) para que CLI y GUI puedan correr en simultáneo. Razón: dos superficies sobre una única fuente
de verdad; los features están disponibles en todas partes o en ninguna.

### III. El Libro Mayor Es la Única Fuente de Verdad
Los saldos de cuenta DEBEN derivarse del libro mayor sumado, nunca almacenarse en una columna
mutable. Los montos DEBEN almacenarse positivos, con la dirección determinada por el tipo de entrada
(`income`, `expense`, `transfer`, `opening`) y garantizada por CHECKs del esquema — no existe tal
cosa como una entrada negativa. Las reglas no negociables (un solo fondo de emergencia activo, solo
las cuentas `target` pueden tener `target_amount`) DEBEN imponerse por el esquema (índice único
parcial, CHECK), no solo por disciplina de la aplicación. Razón: los saldos no pueden desincronizarse,
los saldos sembrados se arrastran entre meses y el error de clasificación se previene por construcción.

### IV. Test-First a través de la API Pública
Cada servicio DEBE publicar pruebas unitarias para las reglas que le pertenecen, y `money_core` DEBE
llevar pruebas de escenarios de caja negra que ejerciten solo la API pública. Los invariantes
contables (split de emergencia, neutralidad de las transferencias, `opening` excluido de los
ingresos, rechazo del esquema legacy, presupuestos no bloqueantes) DEBEN tener cobertura de
regresión. Cambiar comportamiento NO DEBE eliminar ni debilitar una prueba existente sin una razón
revisada y documentada en el mismo cambio. Razón: la lógica del dinero no perdona; las pruebas son
el contrato que mantiene honestos a los handlers.

### V. Simplicidad sobre Features (YAGNI)
Los presupuestos DEBEN seguir siendo informativos: reportan y comparan, y NO DEBEN bloquear ni
fallar ante un sobrecosto. El dashboard legacy de Excel es referencia de solo lectura y NO DEBE
importarse. Una base de datos con el esquema previo al rediseño DEBE rechazarse con un error
accionable en lugar de migrarse en silencio. Los features nuevos DEBEN justificarse por un caso de
uso real, no por generalidad especulativa. Razón: el modelo se mantiene lo bastante pequeño como
para razonarlo y corregirlo a mano.

## Reglas de Dominio y Restricciones de Integridad

- A lo más una cuenta `emergency` activa; el split automático de emergencia NO DEBE dispararse sobre
  cuentas no líquidas (restringidas).
- `target_amount` es opcional y PUEDE no estar en una cuenta `target` (bucket abierto); el progreso
  se muestra como "—" cuando no está definido, nunca como error.
- Una `transfer` es la única entrada capaz de mover dinero entre dos cuentas sin tocar los totales de
  ingresos/gastos; los pagos de tarjeta, los retiros de cajero y los movimientos de buckets DEBEN
  usarla.
- Una entrada `opening` es idéntica en forma a `income` pero DEBE excluirse de los totales de
  ingreso, para que `setup` nunca infle el mes en que corre.
- `entry rm` y el cuadre de cuentas DEBEN operar sobre el mismo libro mayor derivado, sin tabla de
  saldos aparte.

## Flujo de Desarrollo y Portones de Calidad

- Antes de mergear, `cargo build --workspace`, `cargo test --workspace` y
  `cargo clippy --workspace --all-targets` DEBEN pasar. El frontend DEBE pasar type-check con
  `tsc --noEmit`.
- Cuando cambien los modelos exportados, los bindings DEBEN regenerarse vía
  `cargo test -p money_core --features ts-rs`; los campos `i64`/`Option<i64>` DEBEN llevar la
  anulación de tipo ts-rs `number` para que los bindings generados coincidan con lo que realmente
  entrega el IPC de Tauri.
- Las pruebas y la verificación manual DEBEN usar `MONEY_TRACKER_DB` apuntando a una base de datos
  desechable; la base de usuario por defecto NO DEBE usarse para pruebas.
- `AGENTS.md` es el archivo guía de desarrollo en tiempo real y DEBE consultarse para estructura del
  proyecto, convenciones y comandos.

## Gobernanza

Esta constitución sustituye a toda práctica ad-hoc previa y es el contrato que toda contribución debe
satisfacer. Las enmiendas REQUIEREN una propuesta documentada, una revisión de cumplimiento de los
principios afectados y un incremento de versión según la política siguiente; no hay requisito de plan
de migración para cambios constitucionales, pero los principios deprecados DEBEN declararse, no
eliminarse en silencio.

- **Política de versionado**: MAJOR para remociones o redefiniciones de principios incompatibles con
  versiones anteriores; MINOR para principios nuevos o guía ampliada de forma material; PATCH para
  aclaraciones y correcciones de redacción.
- **Cumplimiento**: todo PR y review DEBE verificar que el cambio honra los principios y portones de
  calidad relevantes; un cambio que aumente la complejidad DEBE justificarse contra el Principio V.
- **Expectativa de revisión**: las enmiendas constitucionales se revisan con la misma diligencia que
  el código y registran en la enmienda la versión anterior → nueva y las secciones cambiadas.

**Versión**: 1.0.1 | **Ratificada**: 2026-09-17 | **Última enmienda**: 2026-09-17
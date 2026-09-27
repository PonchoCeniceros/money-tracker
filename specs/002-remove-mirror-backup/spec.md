# Feature Specification: Supabase como única base de datos, con respaldos y esquema versionado

**Feature Branch**: `002-remove-mirror-backup`

**Created**: 2026-09-26

**Status**: Draft

**Input**: User description: "Eliminar el espejo local SQLite del modo remoto: cuando Supabase está
configurado es la única base de datos, y todas las lecturas y escrituras van directo a Supabase, sin
copia local sincronizada. La GUI detecta cambios remotos comparando la revisión de Supabase contra la
última vista. Se eliminan `db remote sync` y la reconstrucción del espejo en `db remote migrate`. El
modo local sin configuración sigue igual. Agregar un comando de respaldo que exporte todo el libro
contable de Supabase a un archivo SQLite local con fecha. Es una foto puntual, no sincronizada, que se
puede abrir como base local con `MONEY_TRACKER_DB` o volver a subir con `db remote migrate --force`.
Definir qué pasa con el `data.db` actual, que hoy es el espejo y tiene 4 filas huérfanas."

El alcance creció durante la clarificación (ver *Clarifications*): SQLite se abandona por completo,
incluido el modo local; el respaldo pasa a ser un archivo SQL; las reglas contables se mueven al
dominio; y el esquema de Supabase se corrige y se versiona.

**Supersedes**: de `001-supabase-backend`, los requisitos FR-007 (migración desde la base local) y
FR-009 (espejo local), los criterios SC-003 y SC-005, la historia 3 (migración), y las tres
clarificaciones de la sesión 2026-09-17 sobre el espejo. El objetivo que motivaba el espejo (no perder
el historial si el plan de Supabase se congela o se pierde) pasa a cubrirlo el respaldo de este spec.

## Clarifications

### Session 2026-09-26

- Q: ¿Cuándo debe hacerse el respaldo del libro contable? → A: Manual cuando el usuario quiera, más un
  respaldo automático perezoso: si el último respaldo tiene más de 7 días (fecha guardada en un archivo
  del directorio de datos de la aplicación), se hace uno al iniciar la GUI o después de correr un
  comando del CLI.
- Q: Después de subir la base local a Supabase con `db remote migrate`, ¿qué pasa con el archivo local
  de origen? → A: No aplica: la migración ya se hizo y no se volverá a migrar desde ninguna base. Si se
  pierde Supabase, se reconstruye con los archivos de esquema más el respaldo. SQLite se abandona por
  completo.
- Q: ¿Cómo se prueban las reglas contables una vez que se quita SQLite del producto? → A: Las reglas
  (sobregiro, límite de crédito, una sola cuenta de emergencia activa, presupuesto positivo) pasan al
  dominio de `money_core`, independientes de cualquier base. Las pruebas automáticas usan un almacén
  falso en memoria, sin red ni Docker. Supabase conserva sus propias validaciones como última defensa,
  y la restauración se verifica con un simulacro manual documentado contra un proyecto de Supabase de
  prueba.
- Q: ¿Cómo se aplican y controlan los cambios al esquema de Supabase, si hoy se aplican a mano en el
  SQL Editor y no se usa el sistema de migraciones de Supabase? → A: Se formaliza el flujo manual:
  archivos SQL numerados que se pegan en el SQL Editor (`0001_setup.sql` es el actual, renombrado);
  una versión de esquema guardada en la base, que cada archivo verifica al inicio y actualiza al final;
  y la aplicación revisa esa versión al arrancar.
- Q: macOS pide la contraseña de la laptop en cada uso para leer el token del llavero; ¿cómo se
  guarda la sesión? → A: Se puede elegir guardarla en un archivo solo legible por el usuario, en lugar
  del llavero del sistema. En ese modo la aplicación no toca el llavero, así que el sistema no pide
  nada. El usuario lo elige para su máquina.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Supabase como única base de datos (Priority: P1)

El usuario quiere tener la certeza de que sus datos viven en un solo lugar. Todo lo que registra y
consulta desde el CLI o la GUI va directo a Supabase, y la aplicación no mantiene ninguna copia local
que se pueda desincronizar. Hoy esa copia existe, no se lee para mostrar nada y ya divergió del libro
alojado (4 movimientos de más y saldos distintos). Tampoco existe ya un modo local: sin Supabase
configurado, la aplicación explica cómo configurarlo en vez de crear una base vacía.

**Why this priority**: Es la razón del cambio: una copia que no aporta nada al uso diario, pero que
puede divergir en silencio, genera desconfianza en las cifras.

**Independent Test**: Registrar un gasto, un ingreso y una transferencia desde el CLI y desde la GUI;
comprobar que ambos muestran las mismas cifras y que ningún archivo de base de datos local se creó ni
se modificó. Después, quitar la configuración y comprobar que ambos explican cómo configurarla.

**Acceptance Scenarios**:

1. **Given** Supabase configurado, **When** el usuario registra cualquier operación desde el CLI o la
   GUI, **Then** la operación queda solo en Supabase, y ningún archivo de base de datos local se crea
   ni se modifica.
2. **Given** la GUI abierta en una instalación, **When** otra instalación registra un movimiento,
   **Then** la GUI muestra el cambio en menos de 1 minuto sin acción manual.
3. **Given** Supabase configurado pero sin conexión, **When** el usuario intenta cualquier operación,
   **Then** recibe un mensaje claro y nada se escribe localmente.
4. **Given** que Supabase no está configurado, **When** el usuario corre cualquier comando que use el
   libro contable, o abre la GUI, **Then** recibe un mensaje que explica cómo configurarlo e iniciar
   sesión, y no se crea ningún archivo de base de datos.
5. **Given** Supabase configurado, **When** el usuario consulta el estado (CLI o GUI), **Then** ve la
   conexión, la sesión, la revisión del libro y la versión del esquema, sin referencias a una copia
   local.

---

### User Story 2 - Respaldo y restauración del libro contable (Priority: P2)

El usuario quiere protegerse contra la pérdida de su proyecto de Supabase. Con un comando del CLI, o
un botón en la GUI, obtiene un archivo con fecha que contiene todo su libro contable. Además, si el
último respaldo tiene más de 7 días, la aplicación hace uno sola al abrir la GUI o al terminar un
comando del CLI. Si pierde el proyecto, crea uno nuevo, aplica los archivos de esquema y el respaldo
en el SQL Editor, y recupera todo.

**Why this priority**: Es lo que reemplaza la protección que daba el espejo. Debe entregarse en la
misma versión que la historia 1: quitar el espejo sin respaldo dejaría al usuario sin protección.

**Independent Test**: Hacer un respaldo, restaurarlo en un proyecto de Supabase de prueba vacío
siguiendo la guía, y comparar el reporte de cada período contra el del proyecto original.

**Acceptance Scenarios**:

1. **Given** Supabase con datos, **When** el usuario hace un respaldo, **Then** se crea un archivo
   nuevo con fecha y hora en el nombre, con todas las cuentas (incluidas las archivadas), movimientos,
   presupuestos, conceptos y configuración, y el sistema le muestra la ruta.
2. **Given** un archivo de respaldo y un proyecto de Supabase nuevo con los archivos de esquema ya
   aplicados, **When** el usuario ejecuta el respaldo en el SQL Editor siguiendo la guía, **Then** el
   proyecto nuevo queda con cifras idénticas a las del original en todos los períodos, y la aplicación
   puede seguir registrando movimientos en él.
3. **Given** un respaldo hecho con una versión de esquema, **When** el usuario intenta restaurarlo en
   un proyecto con otra versión de esquema, **Then** la restauración se detiene con un mensaje claro y
   sin dejar datos a medias.
4. **Given** la GUI, **When** el usuario usa la opción de respaldo en Ajustes, **Then** obtiene el
   mismo resultado que en el CLI y ve la ruta del archivo creado.
5. **Given** que el último respaldo tiene más de 7 días, o que nunca se hizo uno, **When** el usuario
   abre la GUI o termina de correr un comando del CLI, **Then** se crea un respaldo automático y el
   usuario ve un aviso breve con la ruta.
6. **Given** que el último respaldo tiene 7 días o menos, **When** el usuario abre la GUI o corre un
   comando del CLI, **Then** no se crea ningún respaldo y no hay ningún aviso.
7. **Given** que toca un respaldo automático pero falla (sin conexión, sesión expirada, disco lleno),
   **When** el usuario abre la GUI o corre un comando del CLI, **Then** ve una advertencia, pero el
   comando conserva su resultado y la GUI abre con normalidad; se vuelve a intentar en la siguiente
   ocasión.
8. **Given** Supabase sin conexión o con la sesión expirada, **When** el usuario pide un respaldo
   manual, **Then** recibe un mensaje claro y no queda ningún archivo parcial.

---

### User Story 3 - Cambios de esquema seguros y versionados (Priority: P3)

El usuario aplica los cambios al esquema de Supabase pegando archivos SQL en el SQL Editor. Quiere que
un archivo no se pueda aplicar dos veces ni en desorden, saber en todo momento qué versión tiene su
proyecto, y que la aplicación le avise si su esquema no es el que espera, en vez de fallar de formas
raras. Como primer cambio, se corrigen los errores encontrados en el esquema actual, incluido uno que
hoy deja pasar cargos por encima del límite de la tarjeta.

**Why this priority**: Sin esto, cada cambio futuro al esquema es un riesgo silencioso. Va después del
uso diario y del respaldo porque solo importa al cambiar el esquema, pero esta feature ya requiere uno.

**Independent Test**: Aplicar el archivo de cambios a un proyecto de prueba y comprobar que su versión
avanza; aplicarlo otra vez y comprobar que se rechaza sin cambios; correr la aplicación contra un
proyecto con la versión anterior y comprobar que avisa.

**Acceptance Scenarios**:

1. **Given** un proyecto con la versión N del esquema, **When** el usuario aplica el archivo N+1,
   **Then** el cambio se aplica completo y la versión pasa a N+1.
2. **Given** un proyecto con la versión N, **When** el usuario aplica un archivo que no es el N+1 (uno
   ya aplicado, o uno posterior), **Then** el archivo se detiene al inicio con un mensaje claro y no
   cambia nada.
3. **Given** un proyecto cuya versión de esquema no es la que espera la aplicación, **When** el
   usuario corre el CLI o abre la GUI, **Then** recibe un mensaje que indica la versión encontrada, la
   esperada y qué archivo aplicar (o que debe actualizar la aplicación).
4. **Given** una cuenta de crédito con límite y deuda, **When** se intenta registrar un cargo que
   supera el disponible, **Then** se rechaza, tanto en la aplicación como en Supabase.
5. **Given** que no hay sesión iniciada, **When** alguien usa solo la clave publicable del proyecto,
   **Then** no puede leer ni modificar ningún dato, ni el contador de revisión, ni ejecutar funciones.

---

### User Story 4 - Reglas contables probadas sin internet (Priority: P4)

Quien mantiene el proyecto quiere correr todas las pruebas de las reglas contables en segundos, sin
internet, sin Docker y sin ninguna base de datos, con la seguridad de que prueban las mismas reglas que
corren en producción, porque existen en un solo lugar.

**Why this priority**: Es la condición para quitar SQLite sin perder la red de pruebas. No cambia nada
para el usuario final, por eso va al último.

**Independent Test**: Correr la batería de pruebas completa con la red desactivada y confirmar que
pasa, y que cubre sobregiro, límite de crédito, una sola cuenta de emergencia y presupuesto positivo.

**Acceptance Scenarios**:

1. **Given** una máquina sin conexión, **When** se corre la batería de pruebas, **Then** todas pasan
   sin intentar conectarse a nada.
2. **Given** una regla contable (sobregiro, límite de crédito, una sola emergencia, presupuesto
   positivo), **When** se busca dónde está implementada, **Then** existe una sola vez en el dominio, y
   las validaciones de Supabase son solo una defensa adicional.

---

### Edge Cases

- Un respaldo durante una escritura desde otra instalación: el archivo refleja un solo punto
  consistente del libro (una revisión), nunca la mitad de una operación, y registra esa revisión.
- Dos respaldos en el mismo segundo: el segundo no sobrescribe al primero.
- La carpeta de respaldos no existe: se crea.
- Falla la escritura del respaldo (disco lleno, permisos): error claro y ningún archivo parcial.
- Se restaura un respaldo en un proyecto que ya tiene datos: la restauración se detiene sin cambios;
  solo se restaura sobre un proyecto vacío.
- La restauración falla a la mitad (por ejemplo, se cierra el SQL Editor): no deja datos a medias.
- El usuario del proyecto nuevo es otro (otro identificador de usuario): los datos restaurados quedan
  a nombre del usuario que el usuario indique al restaurar.
- El proyecto de Supabase está pausado o no responde: toda operación falla con un mensaje que sugiere
  revisar la conexión y el estado del proyecto.
- Dos dispositivos registran al mismo tiempo cargos que juntos superan el límite: la validación de
  Supabase, que bloquea la cuenta durante la escritura, rechaza el segundo.
- Un binario viejo, anterior a este cambio, se corre contra el esquema nuevo: la revisión de versión
  (o su ausencia en el binario viejo) queda cubierta en la documentación de reinstalación (README,
  sección 1.6).
- Quedan en la máquina archivos del modo local y del espejo (`~/.money-tracker/data.db` y respaldos
  SQLite viejos en el home): la aplicación no los lee ni los modifica; la documentación indica que se
  pueden borrar.

## Requirements *(mandatory)*

### Functional Requirements

**Una sola base de datos**

- **FR-001**: Toda lectura y escritura del libro contable DEBE ocurrir solo contra Supabase. El sistema
  NO DEBE crear, leer ni modificar ningún archivo de base de datos local.
- **FR-002**: DEBEN eliminarse el modo local, el espejo local, y los comandos que solo existían por
  ellos: la sincronización del espejo (`db remote sync`), la migración desde una base local
  (`db remote migrate`), y los comandos del archivo local (`db status`, `db reset`).
- **FR-003**: Sin Supabase configurado, el CLI y la GUI DEBEN explicar cómo configurarlo e iniciar
  sesión, sin crear ningún archivo de base de datos.
- **FR-004**: La GUI DEBE detectar cambios hechos desde otras instalaciones y refrescar sus datos en
  menos de 1 minuto sin acción manual, usando la revisión del libro en Supabase.
- **FR-005**: El estado (CLI y GUI) DEBE mostrar la conexión, la sesión, la revisión del libro y la
  versión del esquema, y NO DEBE mostrar información de espejo ni de modo local.
- **FR-006**: Sin conexión, el comportamiento de `001-supabase-backend` FR-008 se mantiene: la
  operación se rechaza con un mensaje claro.
- **FR-024**: El usuario DEBE poder elegir dónde se guarda su sesión: en el llavero del sistema (valor
  por defecto) o en un archivo legible solo por él. Con el archivo, el sistema NO DEBE leer ni escribir
  el llavero, así que el sistema operativo no pide autorización en ningún uso, ni al recompilar la
  aplicación. La elección aplica igual al CLI y a la GUI.

**Respaldo y restauración**

- **FR-007**: El sistema DEBE ofrecer una operación de respaldo, desde el CLI y desde la GUI, que
  escriba el libro contable completo (cuentas incluidas las archivadas, movimientos, presupuestos,
  conceptos y configuración) en un archivo local nuevo.
- **FR-008**: El respaldo DEBE ser un archivo SQL que se pueda ejecutar en el SQL Editor de un
  proyecto de Supabase nuevo, después de los archivos de esquema, y que deje el libro con cifras
  idénticas. DEBE:
  - conservar los identificadores y las relaciones entre cuentas y movimientos, de modo que la
    aplicación pueda seguir registrando movimientos después de restaurar;
  - asignar los datos al usuario que se indique al restaurar, con un solo dato a completar;
  - ejecutarse completo o no aplicar nada;
  - negarse a correr si el proyecto no está vacío o si su versión de esquema no es la del respaldo.
- **FR-009**: El respaldo DEBE ser consistente: reflejar el libro en un solo punto (una revisión), y
  registrar esa revisión, la versión del esquema y la fecha de creación, dentro del archivo y en el
  mensaje al usuario.
- **FR-010**: El respaldo NO DEBE sobrescribir un archivo existente, NO DEBE dejar un archivo parcial
  si falla, y DEBE crearse legible solo por el usuario dueño (igual que `config.toml`).
- **FR-011**: Por defecto, los respaldos DEBEN guardarse en una carpeta de respaldos dentro del
  directorio de datos de la aplicación (`~/.money-tracker`), con fecha y hora en el nombre. El usuario
  DEBE poder indicar otra ruta de destino.
- **FR-012**: El respaldo DEBE poder ejecutarse a pedido del usuario en cualquier momento. Además, el
  sistema DEBE hacer un respaldo automático cuando el último tenga más de 7 días, o nunca se haya hecho
  uno, revisándolo en dos momentos: al iniciar la GUI y después de que termine un comando del CLI.
  Reglas:
  - La fecha del último respaldo exitoso, manual o automático, DEBE guardarse en un archivo del
    directorio de datos de la aplicación. Si ese archivo falta o no se puede leer, se considera que
    nunca hubo respaldo.
  - Un respaldo manual también reinicia el plazo de 7 días.
  - La revisión NO DEBE correr después del propio comando de respaldo, ni después de un comando que
    falló, ni cuando Supabase no está configurado.
  - Un respaldo automático fallido DEBE mostrarse como advertencia y NO DEBE cambiar el resultado del
    comando ni impedir que la GUI abra. No actualiza la fecha, así que se reintenta en la siguiente
    ocasión.

**Esquema versionado**

- **FR-013**: Los archivos de esquema DEBEN vivir numerados en una carpeta propia del proyecto, fuera
  de la que reconoce el CLI de Supabase. El archivo actual se conserva con el mismo contenido y se
  renombra como `0001_setup.sql`. Un archivo ya aplicado NO DEBE editarse; cada cambio va en un archivo
  nuevo.
- **FR-014**: La base DEBE guardar su versión de esquema. Cada archivo DEBE verificar al inicio que la
  versión actual sea la inmediatamente anterior a la suya (si no, detenerse sin cambiar nada) y
  actualizarla al final, todo en una sola transacción.
- **FR-015**: Al arrancar, el CLI y la GUI DEBEN comparar la versión del esquema con la que esperan y,
  si no coincide, detenerse con un mensaje que diga la versión encontrada, la esperada y qué hacer.
- **FR-016**: El primer archivo de cambios DEBE corregir el esquema actual:
  - la validación del límite de crédito, que hoy deja pasar cargos por encima del disponible;
  - el contador de revisión, que hoy se puede leer y modificar sin sesión: solo lectura, y solo con
    sesión;
  - la ejecución de funciones sin sesión, que hoy está permitida;
  - la validación del RPC de escritura: bloquear la cuenta de origen durante la validación, para que
    dos escrituras simultáneas no la pasen, y comprobar que la cuenta destino también sea del usuario;
  - eliminar los objetos que solo servían al espejo (la función de cambios incrementales y la tabla de
    borrados);
  - actualizar los comentarios que describen el esquema como réplica del espejo SQLite.

**Reglas en el dominio**

- **FR-017**: Las reglas de sobregiro (cuentas de ahorro y emergencia), límite de crédito, una sola
  cuenta de emergencia activa y presupuesto positivo DEBEN implementarse una sola vez en el dominio,
  independientes de la base de datos, y validarse antes de escribir.
- **FR-018**: Ningún handler (CLI o GUI) DEBE reimplementar reglas de dominio; cuando necesite una
  (por ejemplo, para preguntar si aplica el reparto de emergencia), DEBE obtenerla del dominio.
- **FR-019**: Las pruebas automáticas DEBEN correr sin red, sin Docker y sin ninguna base de datos,
  usando un almacén falso en memoria que solo exista al compilar pruebas.

**Documentación y gobierno**

- **FR-020**: DEBE existir una guía del esquema que explique cada tabla y función, cómo aplicar un
  archivo, cómo agregar uno nuevo, cómo saber qué versión tiene el proyecto, y el procedimiento de
  restauración (proyecto nuevo, archivos de esquema, respaldo).
- **FR-021**: DEBE existir un script de verificación que se ejecute en el SQL Editor de un proyecto de
  prueba, intente las operaciones que el esquema debe rechazar (cargo sobre el límite, segunda cuenta
  de emergencia, monto no positivo, forma de movimiento inválida, acceso sin sesión), confirme cada
  rechazo y deshaga todo al terminar.
- **FR-022**: La documentación de uso (README) y la guía de desarrollo (AGENTS.md) DEBEN actualizarse:
  Supabase pasa a ser requisito, sin modo local ni espejo; se documentan el respaldo, la restauración,
  el simulacro de restauración y los archivos de esquema.
- **FR-023**: La constitución DEBE enmendarse donde menciona SQLite, el modo WAL, `MONEY_TRACKER_DB` y
  el rechazo de bases con esquema legacy, declarando esas partes como deprecadas, según su propia
  política de versionado.

### Key Entities *(include if feature involves data)*

- **Libro contable**: la única fuente de verdad, en Supabase (cuentas, movimientos, presupuestos,
  conceptos, configuración). Tiene una revisión que avanza con cada cambio.
- **Versión de esquema**: número guardado en la base que indica qué archivos de esquema se aplicaron.
  La aplicación conoce la versión que espera.
- **Archivo de esquema**: script SQL numerado que lleva la base de la versión N-1 a la N. Nunca se
  edita después de aplicarse.
- **Respaldo**: archivo SQL con la foto completa del libro en un punto. Atributos: fecha y hora de
  creación, revisión de origen, versión de esquema. Solo se restaura sobre un proyecto vacío con la
  misma versión de esquema.
- **Registro del último respaldo**: archivo en el directorio de datos de la aplicación con la fecha y
  hora del último respaldo exitoso y la ruta del archivo creado. Es local a cada máquina.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Después de cualquier secuencia de operaciones desde CLI y GUI, se crean o modifican cero
  archivos de base de datos locales.
- **SC-002**: Un cambio hecho desde una instalación aparece en la GUI de otra en menos de 1 minuto.
- **SC-003**: El 100% de los períodos arroja cifras idénticas en el proyecto original y en un proyecto
  nuevo restaurado desde un respaldo.
- **SC-004**: Un respaldo de un libro 10 veces más grande que el actual (unos 1,100 movimientos)
  termina en menos de 30 segundos con una conexión doméstica normal.
- **SC-005**: Ante la pérdida total del proyecto, el usuario reconstruye su libro siguiendo la guía en
  menos de 15 minutos, sin contar la creación del proyecto nuevo.
- **SC-006**: Cero regresiones contables: la batería de pruebas existente sigue pasando. Las pruebas
  que solo cubrían SQLite o el espejo se eliminan o reemplazan con la razón documentada en el mismo
  cambio (constitución, principio IV).
- **SC-007**: Después de abrir la GUI o de terminar un comando exitoso del CLI, con conexión, el
  respaldo más reciente nunca tiene más de 7 días, sin que el usuario tenga que acordarse de hacerlo.
- **SC-008**: La batería de pruebas completa pasa sin conexión a internet y en menos de 10 segundos.
- **SC-009**: Aplicar un archivo de esquema dos veces, o en desorden, se rechaza el 100% de las veces
  sin ningún cambio en la base; y el script de verificación confirma el 100% de los rechazos esperados.
- **SC-010**: Con la sesión guardada en archivo, correr 10 comandos del CLI, abrir la GUI y volver a
  compilar la aplicación producen cero avisos del sistema pidiendo contraseña.

## Assumptions

- El modelo sigue siendo de un solo usuario, como en `001-supabase-backend`.
- Supabase exige conexión permanente; ya era así, porque el espejo nunca se usó para operar sin
  conexión.
- La protección ante la pérdida del plan de Supabase depende de los respaldos: como máximo quedan
  expuestos unos 7 días de movimientos, según FR-012. Es un tradeoff aceptado.
- Cada máquina lleva su propio registro del último respaldo y sus propios respaldos; no se coordinan
  entre instalaciones.
- No se agrega un comando de restauración en la aplicación: se restaura en el SQL Editor, igual que se
  aplican los archivos de esquema (constitución, principio V).
- Un respaldo solo se puede consultar restaurándolo en un proyecto de Supabase; ya no hay forma de
  abrirlo sin conexión con la aplicación. Es un tradeoff aceptado al abandonar SQLite.
- Los comandos de sesión y estado conservan su nombre actual (`db remote login`, `logout`, `status`)
  para no romper la costumbre ni la documentación.
- La migración desde la base local ya se hizo y no se volverá a necesitar.
- Corregir los 4 movimientos huérfanos del espejo actual queda fuera de alcance: el usuario decide
  aparte si las ediciones que contienen (movimientos #105–#107 de Supabase) deben aplicarse.
- La versión de `apply_entries` que corre hoy en el proyecto es la del archivo actual: el usuario lo
  confirmó el 2026-09-26 con `pg_get_functiondef` (devuelve `entry_id` y la validación de crédito con
  el error de FR-016). El primer archivo de cambios parte de ese estado.
- El proyecto nunca usó el sistema de migraciones de Supabase: el usuario confirmó el 2026-09-26 que
  `supabase_migrations.schema_migrations` no existe. No hay historial que conciliar; la versión de
  esquema de FR-014 empieza a registrarse con el primer archivo de cambios.
- Los respaldos no se cifran; los protegen los permisos de archivo del usuario, igual que
  `config.toml`.
- Guardar la sesión en un archivo protege lo mismo que `config.toml`: cualquier programa que corra
  como el usuario podría leerla. Para una laptop personal es un tradeoff aceptado a cambio de no tener
  avisos del sistema; además, facilita correr la aplicación sin sesión gráfica (un servidor MCP o una
  API en el futuro).
- Un proyecto de Supabase pausado por inactividad queda fuera de alcance más allá de un mensaje de
  error claro.

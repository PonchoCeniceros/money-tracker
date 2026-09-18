# Contract: Espejo SQLite local y sincronización

**Branch**: `001-supabase-backend` | **Spec**: [spec.md](../spec.md) | **Data model**: [data-model.md](../data-model.md)

Define cómo el SQLite local se comporta como **espejo/respaldo** del libro mayor alojado (FR-009),
sin ser almacén operativo.

## 1. Rol y fronteras

- El espejo es **solo de lectura** para los usuarios de `money_core` (los servicios siempre escriben
  vía el backend remoto). Las únicas escrituras al espejo provienen de `MirroringBackend` y
  `sync::refresh` (día 1 / migración / poll).
- Excepciones de solo infraestructura: (a) bootstrap del espejo vacío, (b) migración de un archivo
  local preexistente hacia el remoto (lee el archivo), (c) restaurar el libro desde el espejo si el
  remoto se perdió (lee el archivo).
- `MONEY_TRACKER_DB` sigue controlando la ruta del archivo, ahora con semántica de *espejo* (y para
  tests). Un archivo legacy (`transactions`/`buckets`) ahí sigue rechazado con `LegacySchema`.

## 2. Ecuación de consistencia

1. **Tras cada operación exitosa** (remoto confirmó): el decorador re-lee o reinserta las filas
   afectadas (por los ids de la respuesta; upsert `INSERT ... ON CONFLICT(id) DO UPDATE`) en el
   espejo, incluyendo `user_id`, `updated_at` y la VIEW `account_balances` regenerada por paridad.
2. **Poll de revisión** (~30 s), cada instalación que esté viva:
   - `GET sync_state.revision` → si cambió vs `last_revision` local, `pull_changes_since(watermark)`
     con `updated_at > watermark`, aplica al espejo y avanza watermark.
   - En la GUI, ante una revisión nueva también se llama `bumpRevision()` para que todos los hooks
     de la UI refactorean (la GUI ya refresca así tras cada mutación).
3. **Escritura concurrente**: la fuente alojada es el árbitro (última escritura ganadora). El espejo
   solo reproduce el estado final; no resuelve conflictos (no hay motor de sync propio).

## 3. Alta confianza del respaldo

- El espejo se actualiza **antes** de devolver éxito al CLI/GUI (requisito FR-009 y SC-005): si la
   escritura del espejo falla tras éxito remoto, el comando reporta advertencia explícita (no falso
  éxito silencioso) y el siguiente poll reconciliará.
- Restauración: comando `db remote status` expone `last_revision`, `rows`, y `db` reset/restore
  puede reconstruir el archivo desde una lectura íntegra del espejo.
- Paridad estructural comprobable: `PRAGMA integrity_check` y comparación de esquema
  (`sqlite_master`) contra el DDL de contrato.

## 4. Criterios de aceptación

1. Tras cada operación de escritura exitosa, un dump del espejo contiene exactamente las filas del
   remoto (mismos ids, montos y `updated_at`), con la VIEW `account_balances` coincidiendo.
2. Ante un fallo inyectado de escritura al espejo, el comando no reporta éxito en silencio y el poll
   posterior deja ambos lados coherentes.
3. Una simulación de "pérdida del remoto" permite restaurar el libro completo desde el espejo con
   cifras idénticas al reporte pre-pérdida.
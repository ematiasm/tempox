# backend-survives-slow-queries — timeouts y un `/health` que no miente

Un backend que puede dejar de responder y sigue diciendo `ok`. Se arregla en dos
slices, con el tercero **descartado a propósito**.

## Specs

S1. `GET /health` **no debe ejecutar SQL**. Debe responder desde el estado en
memoria del pool de conexiones, para poder contestar justo cuando la base es el
problema.

S2. `GET /health` responde `200` con `{"status":"ok"}` cuando el pool **puede
servir**, y `503` cuando no puede. El cuerpo conserva la clave `status`, porque
es el contrato que ya existe.

S3. La condición de `503` es **saturación, no vaciedad**: el pool está en su
máximo **y** no queda ninguna conexión libre. Un pool vacío (lazy antes del
primer request, o con las conexiones reapeadas tras reiniciar la base) **puede
servir** y responde `200`, igual que un pool con una conexión tomada y nueve
libres. `min_connections` queda en su default de cero, que es justo por lo que un
pool vacío tiene que poder estar sano.

S3-bis. Con **todas** las conexiones del pool en uso, `GET /health` responde
`503` y lo hace **sin bloquearse** — no puede quedar esperando una conexión,
porque eso lo dejaría dueño del deadlock que intenta reportar.

S4. `GET /health` **no ejecuta SQL ni toma una conexión** en ningún caso, ni
siquiera con el pool vacío. Responde desde contadores en memoria.

S4-bis. (Límite explícito) `GET /health` **no afirma que la base sea
alcanzable**. Un pool que no puede conectar reporta `ok` hasta que el primer
request falla. Probarlo exigiría bloquearse sobre el recurso sospechoso, que es
el error que S3 evita; la alcanzabilidad la dicen los propios requests, y
`docs/en/architecture.md` deja la observabilidad real fuera de alcance.

S5. Toda conexión del backend ejecuta con `statement_timeout = 5s`. Una consulta
que exceda ese límite **falla** en lugar de colgar la conexión para siempre.

S6. Las 29 rutas existentes siguen respondiendo igual. El más caro medido hoy es
`GET /api/sales` con **63,7 ms**; 5 s es ~80× eso, así que ninguna ruta legítima
puede acercarse al límite.

S7. (Decisión, no requisito) **No se pone timeout HTTP de request.** Abortar un
`POST /api/sales` a mitad de vuelo le dice al cliente "falló" mientras la
transacción puede haberse confirmado, lo que crearía una venta fantasma: stock
descontado y plata en caja para un documento que el cajero cree no haber
registrado. El `statement_timeout` de S5 ya acota el caso que importa —una query
lenta— y el pool ya acota el caso del pool agotado con su `acquire_timeout` de
30 s. Si algún día hace falta, se hace por método (nunca en escrituras) o con
cancelación real de la transacción, no con una capa de reloj sobre todo.

## Tasks

- **T1** — `/health` por estado del pool, con sus tres pruebas (S1-S4). Commit propio.
- **T2** — `statement_timeout` de 5 s en toda conexión, con su prueba (S5, S6). Commit propio.

## Log

L1 (pedido original, verbatim): «bueno arreglalo desde el backend. Cual es tu
propuesta?»

L2 (corrección al diagnóstico, medida): el loop del cliente no dañaba al backend.
3 loops sostenidos 20 s = 35 452 requests (1772 req/s) con el mostrador en p50
2 ms. Rate limiting no era el arreglo.

L3 (el defecto real, medido): `LOCK TABLE accounts` en otra sesión dejó
`GET /api/accounts` colgado >35 s sin timeout de servidor; 12 requests colgados
agotaron el pool de 10 y `GET /api/products` —otra ruta, otra tabla— también
colgó, mientras `/health` respondía 200 en 6 ms.

L4 (corrección de L3, medida): la caída **no es permanente**. `acquire_timeout`
de sqlx es 30 s por default. A t=5 s `/api/products` colgaba; a t=35 s respondía
200 en 2,6 ms. El daño es un parón de hasta 30 s, no un apagón. Dicho antes de
vender el plan.

L5 (decisión del dueño): separar en slices; `/health` se puede cambiar.

L6 (rechazo de la opción por clase de consulta): «lo que propones es mas dificil
de mantener, volvamos a la opcion de 5s». El diseño por clase (`Read`/`Write`/
`Report`, ~42 call sites) se descarta explícitamente. Queda un número global de
5 s, y el costo de esa simplicidad se acepta y se documenta en el código.

L7 (decisión del dueño): los slices 1 y 2 se hacen; el slice 3 se deja fuera.

L8 (cierre, 2026-10-27): los dos slices están commiteados y medidos. `13333f4`
(T1) y `6f01a46` (T2), 428 líneas autoradas en total, así que el documento
declara el límite de slice acá. Verificación en vivo sobre el binario real: con
un lock sobre `accounts`, `GET /api/accounts` falla con **500 a los 5007 ms**
(antes colgaba >35 s), y con 12 requests todavía bloqueados `GET /api/products`
responde **200 en 4,8 ms** (antes colgaba 30 s). Suites sobre el árbol
commiteado: 57 unit, 156 integración, 5 en `tests/limits.rs`, clippy
`-D warnings`, `cargo fmt --check` y `make lint` limpios.

L8-bis (error de método propio, registrado porque costó tiempo): volví a arrancar
`cargo run` para "ver `/health` andando" cuando la evidencia en vivo ya estaba
tomada, y el comando quedó colgado porque `cargo run` sirve para siempre y
`tail` no imprime hasta que su entrada se cierra. El dueño tuvo que frenarme. La
regla que se deriva: un chequeo aplicable se corre **una vez**, y un servidor no
se levanta dentro de un pipe que espera EOF.

L9 (revisión nativa `review-10890a3ae8f692e5`, `correction_required`): dos
hallazgos CRÍTICOS contra la primera versión de `/health`, y los dos correctos.
`R3-001` (reliability) y `R4-empty-pool-reports-unavailable` (resilience): el
predicado `size() == 0 || num_idle() == 0` confundía «no tengo una conexión libre
ahora» con «no puedo servir». Reportaba `503` con una conexión tomada de diez, y
con el pool vacío por reinicio de la base o por reaping — es decir, **removía un
backend sano bajo carga liviana o en reposo**, causando la caída que decía
reportar. S2/S3/S4 se reescribieron (S3 ahora nombra saturación, S4 prohíbe SQL,
S4-bis declara el límite de alcanzabilidad) y el predicado pasó a
`idle == 0 && size >= max`. Los dos casos nuevos se observaron en rojo primero
(`left: 503, right: 200`), y neutralizar el `503` hace fallar sólo el de
saturación, así que cada test cubre su rama.

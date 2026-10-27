# backend-survives-slow-queries — timeouts y un `/health` que no miente

Un backend que puede dejar de responder y sigue diciendo `ok`. Se arregla en dos
slices, con el tercero **descartado a propósito**.

## Specs

S1. `GET /health` **no debe ejecutar SQL**. Debe responder desde el estado en
memoria del pool de conexiones, para poder contestar justo cuando la base es el
problema.

S2. `GET /health` responde `200` con `{"status":"ok"}` cuando el pool tiene al
menos una conexión libre, y `503` cuando no puede atender. El cuerpo conserva la
clave `status`, porque es el contrato que ya existe.

S3. Con **todas** las conexiones del pool en uso, `GET /health` responde `503` y
lo hace **sin bloquearse** — no puede quedar esperando una conexión, porque eso
lo dejaría dueño del deadlock que intenta reportar.

S4. Con la base de datos inalcanzable, `GET /health` responde `503` en menos de
un segundo. La prueba de que no intenta conectarse es que no espera el timeout
de conexión.

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

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

L10 (segunda ronda, avisos `R4-001` y `R4-002` del linaje
`review-a9a0d5d66da4c01d`): la revisión los aprobó como informativos, pero el
dueño pidió mirarlos. El texto de los hallazgos **no era recuperable** — llegaron
al chat como id + lente + ubicación + severidad, y el store del linaje se consumió
al quemar la autoridad. Así que se reconstruyeron desde los rangos, midiendo en
vez de suponiendo, y se dice acá para que nadie los lea como citas.

`R4-001` (`limits.rs:64-71`, la función `limited`). Reconstrucción: `options()`
**agrega**, no reemplaza, así que un caller con su propio `statement_timeout`
producía dos. Medido: `-c statement_timeout=30s -c statement_timeout=5s`, con
efectivo `5s`. Funcionaba **por accidente** — porque `limited()` corre segundo y
libpq se queda con el último `-c` del nombre repetido. Nada lo garantizaba.

El primer intento de arreglo fue *reemplazar* en vez de duplicar, y se abandonó
después de medir lo que costaba: `PgConnectOptions` no tiene setter del campo
`options` ni forma de limpiarlo (`options()` sólo agrega), así que reconstruir
exigía pasar por la URL de conexión, y `to_url_lossy()` pierde
`application_name`, pierde el campo `options` entero (`app=None opts=None` tras el
round trip) y **corrompe `sslrootcert`** (`file:file: /etc/ssl/root.pem` al
reparsear). Cambiar un duplicado cosmético por una configuración TLS rota no es
un intercambio aceptable.

Arreglo final: **rechazar**, con `LimitError::{AlreadySet, Unrepresentable}` y un
mensaje que nombra la opción y el valor que aplica este backend. Un caller que
pide otro límite está tomando una decisión que nadie puede tomar en silencio.
Medido de punta a punta: con `DATABASE_URL=...?options=-c statement_timeout=30s`
el binario **sale con código 1** y no arranca; sin él, arranca normal.

`R4-002` (`health.rs:52-59`). Reconstrucción: `size()`, `num_idle()` y
`get_max_connections()` son **tres lecturas atómicas separadas** (`sqlx` mantiene
dos `AtomicU32` independientes y no tiene accessor combinado), así que una
conexión devuelta entre medio puede producir un par que nunca existió junto.
**No se pudo provocar**: 300 sondas concurrentes contra un pool genuinamente
saturado dieron `503` las 300 veces, y 300 bajo churn de conexiones dieron `200`
siempre. Un candado o un retry no comprarían nada que no haga la próxima sonda del
supervisor, así que **se documenta y no se canda** — con una nota explícita de que
las lecturas no son atómicas, para que nadie confíe en una conjunción entre ellas.

L11 (revisión `review-996c8d537c196d77`, `correction_required`): el refuter
encontró **un bug que yo introduje**, `R3-001`, CRÍTICO, y tenía razón.

El arreglo de `R4-001` parseaba el string de `options` **primero** y rechazaba si
no lo entendía. Pero el baseline nunca lo inspeccionaba: agregaba el límite a lo
que hubiera. Así que cualquier `options` que libpq aceptara y este parser no
modelara pasó de funcionar a **impedir el arranque** (`main` sale por el `?`).
Una regresión causada por el arreglo de un aviso cosmético. El test que había
escrito combinaba el token inparseable con un límite ajeno, así que no cubría el
caso.

Arreglo: la pregunta correcta es **qué contiene** el string, no si puedo
reconstruirlo. `mentions_limit()` recorre los tokens de forma tolerante — un token
que no entiende lo saltea, no aborta — y sólo cuando encuentra un
`statement_timeout` ajeno hay que decidir algo, y ahí sí se parsea y se rechaza.
Sin límite ajeno no se toca nada del caller: se agrega el nuestro y el resto queda
byte por byte como vino.

Medido de punta a punta, con el binario real y `DATABASE_URL` llevando `options`:
- `options=-c application_name=probe` → **arranca** (`backend listening on ...`).
- `options=-c statement_timeout=30s` → **sale con código 1**, como debe.
- `options=-x ...` → nuestro código ya no bloquea; es Postgres quien rechaza el
  token por inválido al hacer el handshake, que es el mismo desenlace que tenía el
  baseline.

El test nuevo (`an_unreadable_options_string_without_a_limit_is_accepted`) se
observó en rojo primero: `Unrepresentable("-x -c geqo=off")` donde se esperaba
éxito.

L12 (avisos de `review-cf2d7a5a603d3b13`, el linaje de #101). Otra vez llegaron
sin texto, así que se reconstruyeron midiendo. Uno es real y grave.

`R3-001` (`limits.rs:199-202`), el real. `mentions_limit()` sólo entendía las
formas con `-c`, pero **libpq también acepta `--name=value` sin `-c`**. Medido
contra un servidor vivo:

| `options` del caller | baseline sin nosotros | con el bug |
|---|---|---|
| `--statement_timeout=30s` | servidor reporta **30s** | append nuestro gana, servidor reporta **5s** |
| `-c statement_timeout=30s` | 30s | rechazado, correcto |
| `-cstatement_timeout=30s` | 30s | rechazado, correcto |
| `--geqo=off` | n/a | aceptado, correcto |

O sea: un caller que pedía treinta segundos recibía cinco, **en silencio**. Y peor
que el bug: **mi propio comentario afirmaba que un spelling no modelado "would
still apply the last `-c`"** — falso, porque `--name=value` no es un `-c`. Una
razón incorrecta es peor que ninguna, porque hace que un hueco sin analizar
parezca analizado. Esto es exactamente lo que el aviso señalaba.

Arreglo: `mentions_limit()` maneja las cuatro formas, y se midió cuáles son:
`-c name=value`, `-cname=value`, `--name=value` las aplica PostgreSQL;
`--name value` separado por espacio las **rechaza** ("`--statement_timeout`
requires a value"), así que no necesita manejo y un test lo fija para que no se
pudra.

Al arreglarlo quedaron dos cosas muertas que borré en vez de dejar: la variante
`LimitError::Unrepresentable` (su caso ahora devuelve `AlreadySet`, que es el
veredicto correcto — el operador tiene que sacar el límite, no adivinar cuál de
los dos tokens es el problema) y la función `option_pairs`, que existía sólo para
ese camino. Un test afirma ahora que un límite junto a un token inparseable
reporta el límite, no el token.

`R4-001` (`main.rs:37`), sin cambios de código: el operador ve una línea que
nombra la opción y el valor aplicado, y el proceso sale con código 1.

Verificado: 65 unit (13 en `limits`), 157 integración, 5 `tests/limits.rs`,
clippy `-D warnings`, `fmt --check`.

L13 (avisos de `review-9cf429d4fd484f7e`, el linaje de `998f308`). Cinco avisos,
otra vez sin texto recuperable: el store se consumió al quemar la autoridad, y el
único payload preservado está truncado a 3201 bytes con un solo finding legible.
Así que se reconstruyeron midiendo, y se dice acá.

El único leído completo (del payload truncado, `limits.rs:118` WARNING) no es un
defecto sino una **decisión de producto**: fail-closed al arrancar cuando la
config trae un `statement_timeout` propio. Es cierto que una config que el
baseline aceptaba ahora no arranca; es exactamente el intercambio que el dueño
eligió al preferir rechazo sobre sobrescritura silenciosa, y está documentado en
el código y en el cuerpo del PR. No se cambia.

De los otros cuatro, uno resultó un hueco real al medirlo:

**Comillas ocultando el límite.** `-c 'statement_timeout=30s'` — con comillas
alrededor de todo el par — hacía que `mentions_limit()` dijera "no hay límite", y
nuestro append sobrescribía. Medido que PostgreSQL **rechaza** esa forma
("unrecognized configuration parameter \"'statement_timeout\""), así que no podía
esconder un límite que funcionara: era un falso negativo del escáner, no una
pérdida real. Se cierra igual, porque el escáner es lo único que separa la
intención del caller de una sobrescritura silenciosa, y "la cadena ya es inválida"
no es un hecho del que deba depender. Ahora las comillas se pelan al comparar
nombres, y un valor con comillas que no es límite se sigue aceptando (medido).

Los tres restantes no produjeron cambio de comportamiento tras medirlos: el
parser respeta el escape con backslash (`-c application_name=has\ space` se
acepta y se preserva), los valores con `=` adentro se parten bien
(`split_once` toma el primero), y `main.rs:37` es la propagación del `?` que el
aviso anterior ya había cubierto.

Verificado: 66 unit (14 en `limits`), 157 integración, 5 `tests/limits.rs`,
clippy `-D warnings`, `fmt --check`.

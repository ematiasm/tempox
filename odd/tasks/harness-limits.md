# harness-limits — what `#[sqlx::test]` can and cannot reach

Lo aprendido al intentar ponerle un límite por sentencia a la suite. Nació de un
defecto real y terminó en tres PRs (#100, #101, #103). Se registra aparte porque
no es una tarea pendiente: es una propiedad del harness que hay que conocer antes
de tocar `tests/` otra vez.

## El hecho central

**`#[sqlx::test]` construye su propio pool, ya conectado, antes de que nuestro
código lo vea.** Eso significa que cualquier cosa que `src/limits.rs` haga al
abrir una conexión **no llega** al pool del harness.

Medido: ese pool reporta `statement_timeout = 0`, que en PostgreSQL significa *sin
límite*. El backend tenía su garantía de 5 s y el harness donde corren los tests
no tenía ninguna.

## Dónde SÍ se puede fijar el piso

| Lugar | Funciona | Por qué |
|---|---|---|
| Migración | **No** | `ALTER ROLE` escribe `pg_authid`, un catálogo **compartido por todo el cluster**. `#[sqlx::test]` aplica migraciones **en paralelo** sobre bases por test; seis a la vez abortan con `XX000 "tuple concurrently updated"` (`heapam.c:4711`). |
| `scripts/test-db.sh` | Sí | Recrea la base, un proceso, una escritura. **Pero CI no lo ejecuta**: sólo corre a mano. |
| `.github/workflows/ci.yml`, como step antes de `cargo test` | Sí | Un step corre una vez, serialmente, antes de que existan las bases por test. Es el único lugar que cubre el harness **en CI**, que es donde corre de verdad. |
| `after_connect` | Sí, pero **por pool**, no por tipo | Medido: un pool construido con `after_connect` reporta el límite; otro pool con las mismas opciones reporta `0`. Un wrapper de tipo de conexión no puede imponerlo. |

Ambos lugares hacen falta: el script para una corrida local, el step para CI. El
primero solo habría dejado el piso en la máquina del que lo escribió.

## Cómo distinguir "no hay runners" de "el código está roto"

Costó tres diagnósticos equivocados seguidos. La firma que lo resuelve:

| Señal | Significado |
|---|---|
| `runner: ""`, `billable.total_ms: 0` | ningún runner asignado |
| 0 steps ejecutados, falla en ~4 s | el workflow ni empezó |
| steps ejecutados, tarda minutos | fallo real de código |

Un `queued` de un run recién creado **no** es cuota agotada: es la cola normal.
La cuota agotada se ve como las tres primeras señales juntas.

## Reglas para la próxima vez que se toque `tests/`

1. **No asumir que el harness hereda la configuración de conexión del backend.**
   Construye su pool por su cuenta.
2. **Un `ALTER ROLE`, `ALTER DATABASE` o `ALTER ROLE ... IN DATABASE` no puede
   vivir en una migración** en este repo, por el catálogo compartido y la
   paralelidad de `#[sqlx::test]`.
3. **Todo lo que deba valer en CI tiene que estar en `ci.yml`.** Un script que
   sólo corre a mano no cubre la suite, aunque el test pase localmente.
4. **Al comparar un árbol contra uno revisado, comparar hashes de árbol**
   (`git diff <tree> HEAD`), no la lista de commits. Un commit "vacío" en el
   medio puede esconder otro que no lo es.

## Estado dejado

- `scripts/test-db.sh` fija `statement_timeout = 30s` en el rol, y lo deja
  configurado a propósito (`RESET` devolvería el próximo run al estado sin
  límite que la línea existe para evitar).
- `ci.yml` fija lo mismo como step antes de `cargo test`.
- `tests/limits.rs::the_role_sets_a_floor_that_other_connections_inherit`
  afirma que el pool del harness **no** reporta `0`. Falla si el step se borra o
  si alguien resetea el rol.
- Son 30 s y no 5 s porque `limited.rs` aplica 5 s **después** y es más
  estricto: el piso acota lo que nadie configuró, no es la política del backend.

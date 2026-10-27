# e2e-flakes — fallos intermitentes del nivel e2e, con evidencia

Deuda registrada, no tareas abiertas. Cada entrada existe porque **ocurrió**: hay
una corrida que falló, no una sospecha.

La regla de este documento, y es la razón por la que existe: **un cambio
defensivo sin RED no se registra como fix**. Se registra acá, con su evidencia, y
se arregla cuando se pueda reproducir o cuando vuelva a morder.

---

## E1 — `method.spec.ts:32`, la fila aparece y el saldo todavía no

**Cuándo**: 2026-10-27, CI del push a `main` de `d15017b` (run `40589953462`),
job `End to End (Playwright)`. El commit era **documentación pura** — no podía
ser la causa.

**El fallo, textual**:

```
Locator: locator('li.row').filter({ hasText: 'e2e-bank-mvr7zyza' }).locator('.figure')
Expected: "$ 0,00"
Timeout: 5000ms
Error: element(s) not found
```

**Qué pasó**: se creó la cuenta, `expect(accountRow).toBeVisible()` **pasó**, y la
aserción siguiente sobre `.figure` no encontró el nodo en 5 s. La fila existe; el
saldo derivado llega un instante después.

**La secuencia exacta**, en `frontend/tests/e2e/method.spec.ts`:

```ts
await page.getByRole('button', { name: 'Nueva cuenta' }).click();
const accountRow = page.locator('li.row', { hasText: accountName });
await expect(accountRow).toBeVisible();                      // ← pasó
await expect(accountRow.locator('.figure')).toHaveText(...);  // ← falló
```

**La causa sospechada, no probada**: `toBeVisible()` es una barrera insuficiente.
El `<ul>` puede remontarse cuando llega la relectura posterior al write, y el
locator queda apuntando a un nodo que fue reemplazado. `accounts.spec.ts` ya
documenta ese patrón y lo cubre contando peticiones en vez de mirando el DOM.

**Lo que se sabe, medido**:

| Escenario | Resultado |
|---|---|
| Local, `method.spec.ts` | **3/3 passed** |
| CI, primera corrida | falló |
| CI, re-run de los jobs fallidos | **4/4 passed** |

**Lo que NO se hizo, y por qué**: el arreglo natural es esperar la respuesta de
`/api/accounts` en vez de la visibilidad de la fila:

```ts
const refreshed = page.waitForResponse(isListRead);
await page.getByRole('button', { name: 'Nueva cuenta' }).click();
await refreshed;
await expect(accountRow.locator('.figure')).toHaveText(money('0,00'));
```

No se aplicó porque **no se pudo reproducir el fallo**, así que no habría RED, y
un cambio así se presentaría como fix sin serlo. Es la misma razón por la que
`playwright.config.ts` anota que `workers: 1` "removes a real source of
non-determinism; it is not proven to be the cause".

**Cuándo arreglarlo**: cuando el fallo se reproduzca (dos avistamientos), o
cuando se pueda forzar localmente — por ejemplo con un throttling de red en el
proveedor de Playwright.

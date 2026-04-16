# Benchmark Results

Mediciones de rendimiento por tipo de elemento físico.

**Protocolo:** 120 frames warmup → 600 frames medición → CSV en terminal.
**Hardware:** MacBook (Apple Silicon). **Build:** `cargo run --release -- --bench <escena> <N>`.
**Debug renderer:** desactivado en modo bench para medir el costo real del solver.

---

## Resultados — 2026-04-15

### Con debug renderer (referencia)

| escena           | count | fps_avg | fps_p01 |
|------------------|-------|---------|---------|
| falling-spheres  | 100   | 57      | 52      |
| stacked-boxes    | 100   | 98      | 52      |
| chain-grid       | 10    | 57      | 50      |
| falling-spheres  | 500   | 57      | 51      |
| stacked-boxes    | 500   | 57      | 54      |
| chain-grid       | 30    | 57      | 54      |

### Sin debug renderer (costo real del solver)

| escena           | count | fps_avg | fps_p01 |
|------------------|-------|---------|---------|
| falling-spheres  | 100   | 180     | 165     |
| stacked-boxes    | 100   | 183     | 144     |
| chain-grid       | 10    | 158     | 54      |
| falling-spheres  | 500   | 180     | 165     |
| stacked-boxes    | 500   | 156     | 55      |
| chain-grid       | 30    | 123     | 55      |

---

## Conclusiones

**El debug renderer era el cuello de botella real.**
Costaba ~10ms/frame fijo, independiente de la carga física. Con él activo, todo topa a ~57fps.

**`falling-spheres` escala perfectamente.**
100 vs 500 esferas → mismo fps (180). Rapier duerme cuerpos en reposo de forma tan eficiente
que 400 esferas extra no cuestan nada una vez estabilizadas.

**`chain-grid` tiene costo real y crece con N.**
10 cadenas (158fps) → 30 cadenas (123fps). Los joints crean un sistema acoplado: el solver
itera sobre toda la cadena cada frame, nunca hay sleep, cada interacción afecta a las demás.
Es el elemento más caro — pero 30 cadenas a 120fps deja margen amplio para la máquina de Rube Goldberg.

**El p01 de ~55fps en cadenas** ocurre en el transitorio inicial — momento de máxima tensión
en los joints antes de que la cadena encuentre equilibrio.

**Para la máquina de Rube Goldberg:**
- El debug renderer debe desactivarse en builds de presentación.
- Las cadenas son el elemento más caro; diseñar con eso en mente.
- Esferas y cajas son prácticamente gratuitas a estas escalas.

---

## Cómo correr

```bash
cargo run --release -- --bench falling-spheres 500
cargo run --release -- --bench stacked-boxes 500
cargo run --release -- --bench chain-grid 30
```

Output CSV en terminal:
```
bench,scene,count,fps_avg,fps_p01
bench,falling-spheres,500,180.4,165.1
```

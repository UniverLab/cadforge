# CADforge — Especificación y Roadmap v1.0

> Arquitectura como Código — motor determinista de geometría descriptiva para diseño arquitectónico reproducible, versionable y potenciado por agentes de IA.

---

## 1. Visión

El diseño arquitectónico actual sufre de **entropía gráfica**: los planos son colecciones de líneas sin semántica, imposibles de versionar, comparar o automatizar. CADforge propone un cambio de paradigma:

**El plano no se dibuja, se declara.**

Al igual que el código fuente de software, un espacio arquitectónico es el resultado de un lenguaje estructurado. Si el código no cambia, el plano es idéntico bit a bit cada vez que se compila. Esto elimina la ambigüedad del clic humano, habilita `git diff` sobre planos, y permite que los agentes de IA generen y modifiquen diseños con precisión quirúrgica.

CADforge no es un programa de dibujo. Es la infraestructura para que la arquitectura sea una **ciencia de datos reproducible**.

---

## 2. Ecosistema — Tres Proyectos Separados

La arquitectura se divide en tres proyectos independientes que se integran entre sí:

```
cadforge          → motor de geometría (librería Rust, crates.io)
cadforge-cli      → interfaz de línea de comandos (binario Rust, crates.io)
cadforge-view     → visor gráfico vectorial con modo calco (binario Rust)
```

### Separación de responsabilidades

| Proyecto | Tipo | Responsabilidad |
|---|---|---|
| `cadforge` | Librería | Parser `.cf`, compilador → DXF, motor de geometría, sistema de capas y constraints |
| `cadforge-cli` | Binario | Comandos, wizard, build, watch, import/export, integración con agentes |
| `cadforge-view` | Binario | Visor vectorial estilo consola, modo calco, edición bidireccional → `.cf` |

La librería `cadforge` es reutilizable por cualquier proyecto Rust — el CLI y el visor son consumidores de ella.

---

## 3. Formato de Proyecto

Un proyecto CADforge es un directorio con la siguiente estructura:

```
mi-proyecto/
├── project.toml    ← archivo raíz: metadatos, capas, constraints
├── capa-a.cf       ← capa de primitivos (ej: planta estructural)
├── capa-b.cf       ← capa de primitivos (ej: instalaciones)
└── capa-c.cf       ← capa de primitivos (ej: acabados y anotaciones)
```

El nombre de cada `.cf` lo define el usuario — CADforge no impone nomenclatura ni semántica de capas. Una capa es simplemente un conjunto de primitivos geométricos agrupados.

### project.toml

```toml
[project]
name = "Vivienda Unifamiliar Lote 12"
scale = "1:100"
units = "m"
author = "Arq. Nombre Apellido"
version = "0.3.0"

[layers]
capa-a = { file = "capa-a.cf", locked = false }
capa-b = { file = "capa-b.cf", locked = false }
capa-c = { file = "capa-c.cf", locked = false }

[constraints]
# Si un primitivo de capa-a se mueve, notificar a capa-b
capa-a → capa-b = "spatial_dependency"

# Los primitivos de capa-b deben vivir dentro del bbox de capa-a
capa-b.parent = "capa-a"

# Los primitivos de capa-c referencian explícitamente elementos de capa-b
capa-c.belongs_to = "capa-b"
```

---

## 4. Lenguaje `.cf` (TOML)

El formato `.cf` es TOML válido. Se eligió TOML sobre JSON por ser más legible para humanos y agentes — menos contexto, más señal. Los agentes de IA que ya conocen TOML pueden generar y modificar archivos `.cf` sin entrenamiento adicional.

**Principio clave:** el lenguaje `.cf` trabaja exclusivamente con **primitivos geométricos**. No existe concepto de "muro", "puerta" o "habitación" en el motor base. Esa semántica es responsabilidad del usuario o de capas de abstracción futuras (`cadforge-arch` en v2+). El motor solo sabe de formas, posiciones, atributos visuales y relaciones espaciales.

### Primitivos soportados en v1

| Primitivo | Descripción |
|---|---|
| `[[line]]` | Línea entre dos puntos |
| `[[polyline]]` | Polilínea de múltiples vértices, abierta o cerrada |
| `[[rect]]` | Rectángulo por origen, ancho y alto |
| `[[circle]]` | Círculo por centro y radio |
| `[[arc]]` | Arco por centro, radio y ángulos |
| `[[hatch]]` | Achurado sobre un contorno cerrado |
| `[[text]]` | Texto con posición, tamaño y alineación |
| `[[dim]]` | Cota lineal o angular |
| `[[point]]` | Punto de referencia |
| `[[group]]` | Agrupación de primitivos con id propio |

### Atributos comunes

Todos los primitivos comparten atributos visuales opcionales:

```toml
id          = "string"       # identificador único en la capa
color       = "#FFFFFF"      # color en hex
weight      = 0.35           # grosor de línea en mm
style       = "solid"        # solid | dashed | dotted | dashdot
layer       = "capa-a"       # capa a la que pertenece
visible     = true
locked      = false
```

### Ejemplos

```toml
# capa-a.cf

[[line]]
id = "ln-001"
from = [0.0, 0.0]
to   = [8.5, 0.0]
weight = 0.50

[[polyline]]
id = "pl-001"
points = [[0.0, 0.0], [8.5, 0.0], [8.5, 6.0], [0.0, 6.0]]
closed = true
weight = 0.35

[[rect]]
id = "rc-001"
origin = [1.0, 1.0]
width  = 3.5
height = 4.0
weight = 0.25

[[circle]]
id = "ci-001"
center = [4.0, 3.0]
radius = 0.5

[[arc]]
id = "ar-001"
center     = [2.0, 2.0]
radius     = 0.90
from_angle = 0
to_angle   = 90

[[hatch]]
id       = "ht-001"
boundary = "pl-001"      # referencia al id del contorno cerrado
pattern  = "ansi31"      # ansi31 | ansi32 | ansi33 | ansi34 | solid | none
scale    = 1.0
angle    = 45

[[text]]
id        = "tx-001"
position  = [4.0, 3.0]
content   = "SALA"
size      = 14
align     = "center"     # left | center | right

[[dim]]
id    = "dm-001"
type  = "linear"         # linear | angular | radial
from  = [0.0, 0.0]
to    = [8.5, 0.0]
offset = 0.5             # distancia de la cota al elemento

[[group]]
id      = "gr-001"
members = ["ln-001", "rc-001", "tx-001"]
```

### Atributos globales de capa

```toml
[layer]
name       = "capa-a"
color      = "#FFFFFF"
line_weight = 0.35
visible    = true
locked     = false
```

---

## 5. Sistema de Capas y Constraints

### Concepto

Cada archivo `.cf` es una capa independiente. Las capas se orquestan desde `project.toml`. Las **constraints** son reglas declarativas que definen relaciones espaciales y de pertenencia entre capas.

### Tipos de constraints

| Constraint | Descripción |
|---|---|
| `spatial_dependency` | Si un objeto de la capa A se mueve, la capa B recibe notificación de conflicto |
| `parent` | Los objetos de la capa hija deben vivir dentro del bbox de la capa padre |
| `belongs_to` | Un objeto de la capa hija referencia explícitamente un objeto de la capa padre |

### Comportamiento al compilar

Cuando `cadforge build` detecta una violación de constraints:

```
⚠ CONSTRAINT VIOLATION
  Layer:      capa-b
  Object:     ln-042
  Constraint: spatial_dependency → capa-a
  Detail:     pl-001 in capa-a moved 0.30m east — ln-042 in capa-b now outside its bbox
  Action:     build continues with warning — update capa-b.cf to resolve
```

Las constraints no bloquean el build por defecto — emiten warnings. Se puede configurar `strict = true` en `project.toml` para bloquear.

---

## 6. cadforge-cli — Comandos

```bash
# Inicialización
cadforge new mi-proyecto          # crea estructura de proyecto
cadforge init                     # inicializa en directorio existente

# Compilación
cadforge build                    # compila todas las capas → DXF
cadforge build --layer muros      # compila una capa específica
cadforge build --check            # valida constraints sin generar output

# Desarrollo
cadforge watch                    # modo watch: recompila al guardar cualquier .cf

# Importación / Migración
cadforge import archivo.dxf       # convierte DXF existente → .cf (por capas detectadas)
cadforge import archivo.dxf --layer muros  # importa a capa específica

# Visualización
cadforge view                     # abre cadforge-view con el proyecto actual
cadforge view --layer muros       # abre solo una capa

# Información
cadforge layers                   # lista capas del proyecto con estado
cadforge check                    # valida constraints y reporta conflictos

# Configuración global
cadforge config set author "Arq. Nombre Apellido"
cadforge config set units m
cadforge config show
```

---

## 7. cadforge-view — Visor Vectorial

### Filosofía de diseño

El visor evoca la consola: **fondo negro, líneas vectoriales blancas/grises, tipografía monoespaciada**. No es un editor gráfico pesado — es una ventana de precisión sobre el archivo `.cf`.

El rendering es **vectorial puro** — las líneas mantienen su calidad a cualquier zoom, sin pérdida de fidelidad como ocurriría en una TUI basada en caracteres. Se construye sobre una librería gráfica de bajo nivel (candidatos: `wgpu`, `femtovg`, `tiny-skia`).

### Modos de operación

**Modo lectura:**
- Renderiza el proyecto completo o por capas
- Zoom, pan, toggle de capas
- Muestra constraints activas

**Modo edición (bidireccional):**
- Herramientas básicas: mover objeto, ajustar dimensión, agregar anotación
- Los cambios se traducen al `.cf` correspondiente **al guardar** (no en tiempo real)
- El archivo `.cf` es siempre la fuente de verdad

**Modo calco:**
- La ventana del visor se vuelve semi-transparente (alpha configurable)
- Se puede posicionar sobre otra ventana (imagen, plano escaneado, referencia)
- El usuario traza sobre la referencia y los objetos se capturan como `.cf`
- **Auto-calco**: comando que analiza lo que está debajo de la ventana y propone objetos `.cf` detectados (experimental, v2)

### Atajos de teclado

```
Z / X       → zoom in / out
Flechas     → pan
L           → toggle lista de capas
1-9         → toggle visibilidad de capa por número
E           → entrar a modo edición
C           → entrar a modo calco
S           → guardar cambios al .cf (en modo edición)
Esc         → salir del modo actual
Q           → cerrar visor
```

---

## 8. Importación DXF → `.cf`

Una de las propuestas de valor más importantes: **migrar lo que ya existe**.

```bash
cadforge import plano-existente.dxf
```

El importador:
1. Lee las capas del DXF original
2. Mapea cada capa DXF a un archivo `.cf` nuevo
3. Convierte geometría a objetos declarativos cuando es posible (líneas paralelas → `wall`, bloques → `column`, etc.)
4. Lo que no puede inferir lo convierte a `[[line]]` genéricas — siempre importable, siempre editable
5. Genera un `project.toml` con las capas detectadas

```
cadforge import plano.dxf

✓ Detected 4 layers: MUROS, ESTRUCTURA, COTAS, TEXTO
✓ muros.cf        — 23 walls inferred, 4 openings
✓ estructura.cf   — 8 columns, 1 slab
✓ cotas.cf        — 31 annotations (as [[annotation]])
✓ texto.cf        — 12 text objects (as [[annotation]])
✓ project.toml    — generated

Import complete. Review .cf files and adjust inferred objects.
```

---

## 9. Integración con el Ecosistema Univerlab

### gitkit
Los archivos `.cf` y `project.toml` son texto plano — `git diff` muestra exactamente qué cambió:
```diff
- thickness = 0.15
+ thickness = 0.20
```
No hay comparación de binarios, no hay "Plano_Final_v3_ESTE_SI.dwg".

### agent-canopy
Los agentes pueden leer y escribir archivos `.cf` directamente — TOML es un formato que los LLMs manejan bien. Un agente puede recibir instrucción:
> "Optimiza la distribución de luz natural del salón"

Y modificar coordenadas y orientaciones en `muros.cf` de forma precisa y auditable.

### ghscaff
Plantilla `cadforge` en `ghscaff` para inicializar nuevos proyectos CADforge con estructura de repo correcta desde el primer commit.

---

## 10. Stack Técnico

| Componente | Tecnología |
|---|---|
| Motor de geometría | `truck` (B-rep Rust), `nalgebra` |
| Output DXF | crate `dxf` |
| Parser TOML | `toml` crate |
| CLI | `clap` con derive macros |
| File watcher | `notify` crate |
| Rendering visor | `femtovg` / `tiny-skia` (vectorial, ligero) |
| Ventana visor | `winit` (cross-platform) |
| Transparencia/calco | APIs nativas de ventana por OS |

---

## 11. Roadmap

### MVP
- [ ] Parser TOML para archivos `.cf`
- [ ] Primitivos base: `line`, `polyline`, `rect`, `circle`, `arc`, `hatch`, `text`, `dim`, `point`, `group`
- [ ] Atributos comunes: color, weight, style, visible, locked
- [ ] Compilador `.cf` → DXF (2D, planos de planta)
- [ ] Sistema de capas con `project.toml`
- [ ] Constraints básicas: `parent`, `belongs_to`
- [ ] `cadforge build` y `cadforge watch`
- [ ] Live preview vía visor externo (LibreCAD, FreeCAD)
- [ ] Publicación en crates.io: `cadforge` (librería) + `cadforge-cli`

### v1
- [ ] Importador DXF → `.cf` con detección automática de primitivos
- [ ] `cadforge-view` — visor vectorial propio (fondo negro, líneas vectoriales)
  - Modo lectura con zoom/pan y toggle de capas
  - Modo edición básico con escritura bidireccional al guardar
  - Modo calco con ventana semi-transparente
- [ ] Constraints con warnings en build: `spatial_dependency`
- [ ] Achurados estándar: ansi31, ansi32, ansi33, ansi34, solid
- [ ] Publicación `cadforge-view` en crates.io

### v2
- [ ] `cadforge-arch` — capa de abstracción arquitectónica sobre primitivos
  - Objetos semánticos: `wall`, `opening`, `room`, `column`, `slab`
  - Construidos sobre primitivos del motor base
  - Publicado como crate independiente
- [ ] Arch-Linter — validación de normativas arquitectónicas
  - Áreas mínimas por tipo de espacio
  - Normativas locales configurables por país/región
- [ ] Auto-calco experimental — detección de geometría desde imagen de referencia
- [ ] Output adicional: STL (volumétrico), STEP (intercambio industrial)
- [ ] Constraints `strict = true` que bloquean el build

---

## 12. No-Goals (v1)

- No es un reemplazo de Revit o AutoCAD para flujos complejos de BIM
- No genera renders fotorrealistas
- No maneja modelos 3D complejos (solo extrusión simple de planta en v1)
- No requiere conexión a internet ni licencias propietarias
- No depende del MCP de Autodesk

---

## 13. Contexto Académico

`cadforge` es el proyecto de tesis de especialización en IA con enfoque en diseño arquitectónico. La hipótesis central es que tratar la arquitectura como código — con determinismo, versionado y agentes — representa un cambio de paradigma en el flujo de trabajo del diseño arquitectónico.

El proyecto vive bajo [`univerlab`](https://github.com/univerlab) junto a `texforge`, `gitkit`, `ghscaff` y `agent-canopy`, siguiendo los mismos principios: binario standalone, offline first, sin scope creep.

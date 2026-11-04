<p align="center">
  <img src="images/EH3D_PHOENIX_PNP_BANNER.svg" alt="EH3D-PHOENIX-PNP banner" width="100%">
</p>

# 📖 EH3D-PHOENIX-PNP

<p align="center"><a href="README.md">🇺🇸 English</a> | 🇪🇸 <b>Español</b> | <a href="README_fra.md">🇫🇷 Français</a> | <a href="README_ita.md">🇮🇹 Italiano</a> | <a href="README_deu.md">🇩🇪 Deutsch</a> | <a href="README_zho.md">🇨🇳 中文</a> | <a href="README_jpn.md">🇯🇵 日本語</a> | <a href="README_por.md">🇵🇹 Português</a></p>

### Montaje pick and place: posiciones, alimentadores y orden de colocación

<p align="center">
  <img src="https://img.shields.io/badge/License-GPL%203.0--or--later-blue.svg" alt="GPL 3.0 or later">
  <img src="https://img.shields.io/badge/Stack-Rust-DEA584.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Languages-8-00E5FF.svg" alt="Languages">
  <img src="https://img.shields.io/badge/Version-0.0.4-0f6cbd.svg" alt="Version">
  <img src="https://img.shields.io/badge/Maturity-scaffolding-FFB020.svg" alt="Maturity">
</p>

---

**Comprobación de honestidad - lo que funciona hoy:** una biblioteca Rust y un programa de línea de comandos (`phoenix-pnp`) con 198 pruebas que se ejecutan en un ordenador: lee el fichero de posiciones de una placa, da a cada componente su alimentador (con prioridades, bandejas y tiras), lee los ajustes de la máquina de un fichero, planifica el orden de colocación para un cabezal de hasta seis boquillas (rangos, puntas de boquilla, límites de ángulo, tiempo de los movimientos, paneles), escribe una hoja de carga y, en una máquina simulada, ejecuta el plan paso a paso con fallos inyectados. No controla ninguna máquina: todavía no tiene ventana, no se ha probado en una máquina y no se ha comprobado con una colección de ficheros reales.

---

> **Estado de seguridad:** este programa no controla ninguna máquina y no es un sistema de seguridad. Lo que produce debe revisarlo una persona antes de ejecutar nada en una máquina; nada de esto se ha ejecutado en hardware real.


## 📋 De un vistazo

| | |
|---|---|
| Versión | `0.0.4` |
| Madurez | Andamiaje |
| Pruebas | 198 |
| Pila tecnológica | Rust |
| En hardware real | No probado: nada de esto se ha ejecutado en una máquina |

**Ir a:** [Resumen](#-resumen) · [Arquitectura](#-arquitectura) · [Inicio rápido](#-inicio-rápido) · [Pruebas](#-pruebas) · [Estructura](#-estructura-del-repositorio) · [Relacionados](#-proyectos-relacionados)

## 🚦 Estado del proyecto

Cada línea es una parte de este repositorio y la luz dice hasta dónde llega hoy. Se escribe con lo que el repositorio puede demostrar (pruebas, compilaciones, ejecuciones), no con lo previsto.

🟢 Hecho y verificado · 🟡 Funciona, con límites conocidos · 🟠 Hecho a medias o sin verificar · 🔴 Sin hacer o sin funcionar

| Área | Estado | Qué significa aquí |
|---|---|---|
| Núcleo | 🟡 | lee el fichero de posiciones de una placa, da a cada componente su alimentador (prioridad, bandeja, tira), planifica el orden de colocación para un cabezal de hasta seis boquillas con rangos, puntas de boquilla, límites de ángulo y el tiempo de los movimientos, planifica paneles y escribe la hoja de carga; probado en un ordenador con entradas normales y dañadas, no con una colección de ficheros reales. |
| Línea de comandos | 🟡 | `phoenix-pnp` ejecuta las mismas funciones que la biblioteca (`parts`, `check`, `plan`, `schedule`, `preflight`, `simulate`, `loadsheet`, `config-check`) e imprime el resultado; cada error vuelve como un mensaje con su código `PHOENIX-PNP-NNN` y un código de salida distinto de cero. |
| Pruebas | 🟡 | 198 pruebas pasan (`cargo test`), con casos normales, casos límite, entradas dañadas y pequeños casos generados. |
| Configuración de la máquina | 🟡 | un fichero `clave = valor` con versión, un validador que nombra la línea y los límites de seguridad (recorrido de Z, límites de X e Y, reintentos, ángulos de boquilla); los valores no se han comparado con una máquina real. |
| Simulador y trabajo | 🟡 | un reloj virtual, fallos inyectados al azar (con semilla) o en un lugar elegido, reintentos, las políticas alerta y aplazar, alimentadores que se apagan tras fallos repetidos, pausa, paso a paso y un estado guardado para reanudar; solo simula, nada ha movido una máquina. |
| Seguridad | 🟡 | Rust seguro y simple (`forbid(unsafe_code)`): nada entra en pánico con los ficheros recibidos y un valor malo es un error con su motivo; sin revisión independiente. |
| Ventana | 🔴 | Previsto: la ventana egui común a la suite; hoy solo existe la línea de comandos. |
| Hardware real | 🔴 | No probado: nada de esto se ha ejecutado en una máquina. |
| Documentación | 🟡 | README en ocho idiomas, el registro de cambios y las dos imágenes dibujadas desde `docs/`. |

## 🎯 Resumen

* **Los componentes** (`src/parts.rs`): un fichero `.pos` de KiCad o un CSV (coma o punto y coma, comillas, cabecera hallada por nombre) con posiciones en mm, mil o pulgadas, los ángulos llevados a 0-360, la cara superior o inferior, y cada fila sin sentido se rechaza con su número; una referencia repetida es un error.
* **Los alimentadores** (`src/feeders.rs`): un CSV de alimentadores (id, valor, encapsulado, posición de recogida, piezas que quedan); cada componente recibe el primer alimentador que contiene su valor y encapsulado, sin distinguir mayúsculas; se informa de un componente sin alimentador y de un alimentador que se agota, y un segundo alimentador del mismo tipo toma el relevo.
* **El plan** (`src/plan.rs`): los lugares se recorren al más cercano y se cortan en lotes de una a seis boquillas, los alimentadores de un lote se visitan al más cercano, la cara inferior se refleja respecto al ancho de la placa, y el plan da el recorrido y una estimación del tiempo; se escribe como trabajo JSON.
* **Las tablas y los ajustes** (`src/catalog.rs`, `src/config.rs`, `src/rotation.rs`): tablas de componentes y encapsulados cuyos valores se heredan (componente, luego encapsulado, luego los valores por defecto), un fichero de configuración de la máquina en formato `clave = valor` (`examples/machine.cfg` documenta cada clave), más nombres para las columnas de un CSV de posiciones y correcciones de la rotación por encapsulado.
* **Los alimentadores con algo más que una cinta** (`src/feeding.rs`): prioridad, bandejas, tiras y alimentadores que se apagan; el alimentador de un componente se elige igual sea cual sea el orden de los ficheros.
* **El planificador de la máquina** (`src/planner.rs`, `src/motion.rs`): rangos (bloques que se colocan uno tras otro), estrategias de orden, puntas de boquilla y sus cambios, los límites del ángulo de la boquilla, el tiempo de los movimientos con aceleración y tirón, recorridos más cortos (2-opt) y la corrección del origen de un componente.
* **Paneles, hoja de carga y exportaciones** (`src/panel.rs`, `src/loadsheet.rs`, `src/export.rs`): placas y paneles dentro de paneles, la ranura de cada tipo de cinta, un informe, la lista de lo que falta y un G-code que solo mueve el cabezal.
* **El simulador y el trabajo** (`src/sim.rs`, `src/job.rs`, `src/preflight.rs`, `src/model.rs`): las comprobaciones previas al trabajo, una máquina con reloj virtual y fallos que se pueden inyectar, y un trabajo que se puede pausar, ejecutar paso a paso, guardar y reanudar.

## 🧩 Papel en EH3D

EH3D-PHOENIX-PNP es un programa de la EH3D Fabrication Suite. Hoy funciona como biblioteca y línea de comandos; su ventana, con el aspecto que comparten los demás programas de la suite, está prevista. Lee y escribe ficheros simples, así que puede usarse solo o entre los demás programas.

## 🏛️ Arquitectura

<p align="center">
  <img src="images/architecture.svg" alt="Arquitectura de EH3D-PHOENIX-PNP: quién lo usa, qué contiene y de qué depende" width="100%">
</p>


Cómo encajan las partes de este repositorio: quién lo usa, qué contiene y de qué depende. La imagen se dibuja desde `docs/architecture.json` y se comprueba con el código.

## ✨ Funciones

* **Biblioteca y línea de comandos:** cada orden de `phoenix-pnp` es una función de la biblioteca, así que otro programa de la suite puede llamarla sin lanzar un proceso.
* **Línea de comandos estricta:** una opción mal escrita (`--powr`) o una palabra de más es un error que la nombra, no algo que se ignora; si no, el programa haría su salida con los valores por defecto.
* **Pruebas con entradas dañadas:** las pruebas también dan ficheros vacíos, basura, números enormes y ficheros cortados, y ninguna puede provocar un pánico.
* **Sin adivinar:** un valor que no tiene sentido es un error que dice cuál y por qué, nunca un resultado que solo lo parece.
* **Sin código unsafe:** `#![forbid(unsafe_code)]` en la biblioteca y el programa.
* **Un solo lector para las dos tablas, y rápido en una placa grande:** el fichero de posiciones y el de alimentadores los lee el mismo lector (coma o punto y coma, campos entre comillas con un separador o un salto de línea), cincuenta mil piezas se comprueban, se asignan y se planifican en un momento, el plan nunca se escribe sobre un fichero de origen, una opción desconocida es un error y `--origin-x` dice dónde está el borde izquierdo de la placa al espejar la cara inferior.

## 🔒 Seguridad y protección

| Amenaza | Defensa |
|---|---|
| Un fichero dañado, cortado o vacío | sin pánico: las pruebas cortan una entrada válida en cada posición y dan basura |
| Un número sin sentido (negativo, no finito, absurdamente grande) | se rechaza con un error que nombra el valor; nada se redondea hasta parecer plausible |
| Uso inseguro de memoria | Rust seguro y simple con `forbid(unsafe_code)` |

**Lo que no se defiende:** un resultado producido sin errores no es un resultado verificado. Nada de esto conoce los límites de una máquina y el programa no ha tenido revisión independiente. Informe de una vulnerabilidad en privado (véase [SECURITY.md](SECURITY.md)).

## 🔌 Interfaces

La biblioteca es `eh3d_phoenix_pnp` (`use eh3d_phoenix_pnp::...`); la línea de comandos es `phoenix-pnp`. Las órdenes son estas; cada una imprime su resultado por la salida estándar e informa de un error por la salida de errores con un código de salida distinto de cero. No hay acceso a la red.

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               las tablas de componentes y encapsulados y la herencia de sus propiedades
│   ├── cli_args.rs                              los argumentos de la línea de comandos, estrictos: una opción desconocida es un error
│   ├── commands.rs                              las órdenes que usan la configuración: schedule, preflight, simulate, loadsheet
│   ├── config.rs                                el fichero de configuración de la máquina: claves, rangos y límites de seguridad
│   ├── error.rs                                 los códigos de error PHOENIX-PNP-NNN y su significado en ocho idiomas
│   ├── export.rs                                el informe, la lista de lo que falta y el G-code de los movimientos
│   ├── feeders.rs                               una parte de la lógica
│   ├── feeding.rs                               alimentadores con prioridad, bandejas y tiras, y la elección del alimentador de un componente
│   ├── job.rs                                   el trabajo: pasos, pausa, reintentos, políticas, estadísticas y estado guardado
│   ├── lib.rs                                   la biblioteca: lo que usa cada orden
│   ├── loadsheet.rs                             la hoja de carga: la ranura de cada tipo de cinta
│   ├── main.rs                                  la línea de comandos
│   ├── model.rs                                 las partes de una máquina como traits: máquina, boquilla, alimentador, actuador, cámara, controlador
│   ├── motion.rs                                el tiempo que tarda un movimiento, con aceleración y tirón
│   ├── panel.rs                                 paneles: placas y paneles dentro de paneles, cada uno en su sitio
│   ├── parts.rs                                 una parte de la lógica
│   ├── plan.rs                                  una parte de la lógica
│   ├── planner.rs                               el planificador que conoce la máquina: rangos, orden, puntas, ángulos y tiempo
│   ├── preflight.rs                             las comprobaciones previas al trabajo, con gravedad y aquello a lo que apuntan
│   ├── rotation.rs                              correcciones de la rotación por encapsulado, y cómo gira una boquilla limitada
│   ├── sim.rs                                   la máquina simulada: reloj virtual y fallos que se pueden inyectar
│   └── units.rs                                 longitudes y lugares que llevan su unidad
├── tests/
│   ├── commands.rs                              pruebas de las órdenes que usan la configuración
│   ├── feeding.rs                               pruebas de los alimentadores: prioridad, bandejas y tiras
│   ├── job.rs                                   pruebas del trabajo: pasos, reintentos y estado guardado
│   ├── loadsheet.rs                             pruebas de la hoja de carga y las exportaciones
│   ├── model.rs                                 pruebas del modelo de datos: códigos, longitudes, configuración y tablas
│   ├── more.rs                                  más pruebas de la biblioteca y la línea de comandos
│   ├── panel.rs                                 pruebas de los paneles
│   ├── planner.rs                               pruebas del tiempo de los movimientos y del planificador
│   ├── pnp.rs                                   pruebas de la biblioteca y la línea de comandos
│   ├── preflight.rs                             pruebas de las comprobaciones previas al trabajo
│   ├── readme.rs                                pruebas de que el README se puede ejecutar y está completo
│   ├── sim.rs                                   pruebas de la máquina simulada
│   └── strict.rs                                pruebas de la línea de comandos: opciones y palabras que no conoce
├── examples/
│   ├── board.pos                                una muestra para probar el programa
│   ├── catalog.csv                              una tabla de componentes: rango y altura
│   ├── feeders.csv                              una muestra para probar el programa
│   ├── machine.cfg                              los ajustes de la máquina, cada clave explicada
│   ├── packages.csv                             una tabla de encapsulados: altura, punta, rango y cinta
│   ├── panel.json                               un panel de dos placas
│   └── slots.csv                                las ranuras de la máquina
├── docs/
│   ├── architecture.json                        datos de la imagen de Arquitectura
│   └── structure.json                           datos de la imagen de Estructura
├── tools/                                       comprobación del CI y del README y herramientas de imágenes
├── images/                                      banner y las imágenes de Arquitectura y Estructura del repositorio
├── .github/                                     flujos de CI y de publicación, plantillas de issues y pull requests
├── eh3d.project.json                            manifiesto: versión, rol, madurez y comando de compilación
├── Cargo.toml Cargo.lock                        el workspace y sus dependencias fijadas
├── rust-toolchain.toml                          la toolchain de Rust fijada
├── CHANGELOG.md                                 qué cambió en cada versión
├── SECURITY.md SUPPORT.md                       cómo informar de una vulnerabilidad y dónde preguntar
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   cómo contribuir, el código de conducta y la licencia
```

## ⚙️ Configuración

La máquina y el planificador se configuran con un fichero de texto de líneas `clave = valor` (`--config FICHERO`), el mismo formato que usan los demás programas de la suite; `examples/machine.cfg` lista cada clave con su valor por defecto y lo que hace. Una clave que falta conserva su valor por defecto; una clave desconocida, escrita dos veces o fuera de su rango es un error que nombra la línea. Las opciones de la línea de comandos (`--nozzles`, `--speed`, `--order`, `--side`) mandan sobre el fichero.

Las tablas de encapsulados y de componentes (`--packages`, `--catalog`) son ficheros CSV en los que una celda vacía hereda; un panel (`--panel`) es un fichero JSON; las ranuras de la máquina (`loadsheet`) son un CSV con `id,x,y,max_width_mm`. Un fichero mayor que `max_file_bytes` se rechaza. Cada error tiene un código `PHOENIX-PNP-NNN`; su mensaje se escribe en el idioma de la clave `language` o de `--lang` (`en`, `es`, `de`, `fr`, `it`, `ja`, `zh` o `pt`):

| Código | Significado |
|---|---|
| `PHOENIX-PNP-001` | El archivo supera el tamaño permitido |
| `PHOENIX-PNP-002` | Hay más piezas de las que permite el límite |
| `PHOENIX-PNP-003` | Una línea de la configuración no es clave = valor |
| `PHOENIX-PNP-004` | La configuración tiene una clave que no se conoce |
| `PHOENIX-PNP-005` | Un valor de la configuración no es válido |
| `PHOENIX-PNP-006` | Un valor de la configuración está fuera de su rango |
| `PHOENIX-PNP-007` | La configuración se escribió para una versión más nueva del programa |
| `PHOENIX-PNP-008` | Una clave de la configuración está escrita dos veces |
| `PHOENIX-PNP-009` | Un ajuste supera los límites de seguridad |
| `PHOENIX-PNP-010` | Una fila de la tabla de piezas o paquetes no es válida |
| `PHOENIX-PNP-011` | Una pieza o un paquete está definido dos veces |
| `PHOENIX-PNP-012` | Una regla de corrección de rotación no es válida |
| `PHOENIX-PNP-013` | Una fila de los alimentadores no es válida |
| `PHOENIX-PNP-014` | El archivo del panel no es válido |
| `PHOENIX-PNP-015` | El panel está anidado demasiado hondo o tiene demasiadas placas |
| `PHOENIX-PNP-016` | La tabla de ranuras no es válida |
| `PHOENIX-PNP-017` | Un ajuste del plan no es válido |
| `PHOENIX-PNP-018` | El nozzle no puede girar a ese ángulo dentro de sus límites |
| `PHOENIX-PNP-019` | El nozzle ya lleva una pieza: no puede recoger otra |
| `PHOENIX-PNP-020` | El nozzle no lleva ninguna pieza para colocar |
| `PHOENIX-PNP-021` | No hay ningún alimentador disponible para la pieza |
| `PHOENIX-PNP-022` | No se pudo recoger la pieza tras todos los intentos |
| `PHOENIX-PNP-023` | El alimentador no pudo avanzar tras todos los intentos |
| `PHOENIX-PNP-024` | No se pudo colocar la pieza tras todos los intentos |
| `PHOENIX-PNP-025` | El estado guardado pertenece a otro trabajo |
| `PHOENIX-PNP-026` | El estado guardado del trabajo no es válido |
| `PHOENIX-PNP-027` | El trabajo está en pausa, terminado o detenido por un error |
| `PHOENIX-PNP-028` | Las comprobaciones previas al trabajo encontraron errores |
| `PHOENIX-PNP-029` | Una longitud o un ángulo no es válido |
| `PHOENIX-PNP-030` | El controlador no aceptó la orden |
| `PHOENIX-PNP-031` | El simulador inyectó un fallo |

## 🚀 Inicio rápido

Necesita una cadena de herramientas Rust estable (1.75 o más reciente).

```bash
cargo build --release
cargo test
cargo run --release -- plan examples/board.pos examples/feeders.csv examples/plan.json --nozzles 2
cargo run --release -- schedule examples/board.pos examples/feeders.csv examples/schedule.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --state examples/job_state.json --steps 5
cargo run --release -- loadsheet examples/board.pos examples/slots.csv examples/loadsheet.csv --packages examples/packages.csv
```

## 🧪 Pruebas

La suite tiene 198 pruebas (`cargo test`, que se ejecutan en Windows y Linux en la CI). Comprueban los casos normales, los casos límite y las entradas dañadas: los ficheros vacíos, la basura y los ficheros cortados nunca provocan un pánico. Pequeños casos generados (desde una semilla fija, así que dan los mismos números en cada ejecución) comprueban que el plan no depende del orden de los ficheros, que un recorrido nunca se alarga, que un trabajo guardado y reanudado en cualquier punto acaba igual que el que nunca se detuvo, y que las longitudes y los ángulos se convierten de ida y vuelta. La CI también ejecuta `cargo fmt --check`, `cargo clippy` con las advertencias como errores y `python -I tools/ci_validate.py`.

## 🏗️ Compilación y publicación

La versión está en `Cargo.toml` y en `eh3d.project.json`; cada versión tiene su entrada en [CHANGELOG.md](CHANGELOG.md). `build.bat` / `build.sh` compilan el programa, `test.bat` / `test.sh` ejecutan las pruebas y `run.bat` / `run.sh` lo ejecutan con un ejemplo. Las versiones se publican en GitHub.

## 📂 Estructura del repositorio

<p align="center">
  <img src="images/repository-structure.svg" alt="Estructura del repositorio EH3D-PHOENIX-PNP" width="100%">
</p>

<details>
<summary>Versión en texto del árbol</summary>

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               las tablas de componentes y encapsulados y la herencia de sus propiedades
│   ├── cli_args.rs                              los argumentos de la línea de comandos, estrictos: una opción desconocida es un error
│   ├── commands.rs                              las órdenes que usan la configuración: schedule, preflight, simulate, loadsheet
│   ├── config.rs                                el fichero de configuración de la máquina: claves, rangos y límites de seguridad
│   ├── error.rs                                 los códigos de error PHOENIX-PNP-NNN y su significado en ocho idiomas
│   ├── export.rs                                el informe, la lista de lo que falta y el G-code de los movimientos
│   ├── feeders.rs                               una parte de la lógica
│   ├── feeding.rs                               alimentadores con prioridad, bandejas y tiras, y la elección del alimentador de un componente
│   ├── job.rs                                   el trabajo: pasos, pausa, reintentos, políticas, estadísticas y estado guardado
│   ├── lib.rs                                   la biblioteca: lo que usa cada orden
│   ├── loadsheet.rs                             la hoja de carga: la ranura de cada tipo de cinta
│   ├── main.rs                                  la línea de comandos
│   ├── model.rs                                 las partes de una máquina como traits: máquina, boquilla, alimentador, actuador, cámara, controlador
│   ├── motion.rs                                el tiempo que tarda un movimiento, con aceleración y tirón
│   ├── panel.rs                                 paneles: placas y paneles dentro de paneles, cada uno en su sitio
│   ├── parts.rs                                 una parte de la lógica
│   ├── plan.rs                                  una parte de la lógica
│   ├── planner.rs                               el planificador que conoce la máquina: rangos, orden, puntas, ángulos y tiempo
│   ├── preflight.rs                             las comprobaciones previas al trabajo, con gravedad y aquello a lo que apuntan
│   ├── rotation.rs                              correcciones de la rotación por encapsulado, y cómo gira una boquilla limitada
│   ├── sim.rs                                   la máquina simulada: reloj virtual y fallos que se pueden inyectar
│   └── units.rs                                 longitudes y lugares que llevan su unidad
├── tests/
│   ├── commands.rs                              pruebas de las órdenes que usan la configuración
│   ├── feeding.rs                               pruebas de los alimentadores: prioridad, bandejas y tiras
│   ├── job.rs                                   pruebas del trabajo: pasos, reintentos y estado guardado
│   ├── loadsheet.rs                             pruebas de la hoja de carga y las exportaciones
│   ├── model.rs                                 pruebas del modelo de datos: códigos, longitudes, configuración y tablas
│   ├── more.rs                                  más pruebas de la biblioteca y la línea de comandos
│   ├── panel.rs                                 pruebas de los paneles
│   ├── planner.rs                               pruebas del tiempo de los movimientos y del planificador
│   ├── pnp.rs                                   pruebas de la biblioteca y la línea de comandos
│   ├── preflight.rs                             pruebas de las comprobaciones previas al trabajo
│   ├── readme.rs                                pruebas de que el README se puede ejecutar y está completo
│   ├── sim.rs                                   pruebas de la máquina simulada
│   └── strict.rs                                pruebas de la línea de comandos: opciones y palabras que no conoce
├── examples/
│   ├── board.pos                                una muestra para probar el programa
│   ├── catalog.csv                              una tabla de componentes: rango y altura
│   ├── feeders.csv                              una muestra para probar el programa
│   ├── machine.cfg                              los ajustes de la máquina, cada clave explicada
│   ├── packages.csv                             una tabla de encapsulados: altura, punta, rango y cinta
│   ├── panel.json                               un panel de dos placas
│   └── slots.csv                                las ranuras de la máquina
├── docs/
│   ├── architecture.json                        datos de la imagen de Arquitectura
│   └── structure.json                           datos de la imagen de Estructura
├── tools/                                       comprobación del CI y del README y herramientas de imágenes
├── images/                                      banner y las imágenes de Arquitectura y Estructura del repositorio
├── .github/                                     flujos de CI y de publicación, plantillas de issues y pull requests
├── eh3d.project.json                            manifiesto: versión, rol, madurez y comando de compilación
├── Cargo.toml Cargo.lock                        el workspace y sus dependencias fijadas
├── rust-toolchain.toml                          la toolchain de Rust fijada
├── CHANGELOG.md                                 qué cambió en cada versión
├── SECURITY.md SUPPORT.md                       cómo informar de una vulnerabilidad y dónde preguntar
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   cómo contribuir, el código de conducta y la licencia
```

</details>

## 🗺️ Hoja de ruta y límites conocidos

**Límites conocidos hoy**

* Todavía no tiene ventana: solo existen la biblioteca y la línea de comandos.
* No se ha probado en una máquina ni con una colección de ficheros reales, y no ha tenido revisión independiente; todo lo que hace con una máquina es una simulación.
* No lee los ficheros de otros programas de colocación, y la regla del ángulo de la cara inferior de un CSV (`bottom_rotation`) no se ha comparado con ficheros reales de ningún programa de CAD.
* Todavía no hay cámara, ni corrección de la recogida por visión, ni controlador de una máquina real; el plan es un fichero JSON para que lo ejecute otro programa.

**Siguiente**

* Añadir la ventana con el aspecto que comparten los programas de la suite, en los ocho idiomas: la mesa de trabajo, las tablas de componentes y alimentadores y el panel de problemas y soluciones.
* Fiduciales de la placa y del panel, y la corrección con la cámara de EH3D-VISION-STUDIO; enviar un plan a una máquina solo después de probarlo en una real.
* Probarlo con ficheros reales y con los demás programas de la suite.

## 🌍 Idiomas

El README y los textos existen en inglés, español, francés, italiano, alemán, chino, japonés y portugués de Portugal.

## 🔗 Proyectos relacionados

* [EH3D-VISION-STUDIO](https://github.com/JuanenRac/EH3D-VISION-STUDIO): las cámaras, los fiduciales y la inspección
* [EH3D-MOTIONCNC](https://github.com/JuanenRac/EH3D-MOTIONCNC): el programa de control de máquina
* [EH3D-GERBERVIEWER](https://github.com/JuanenRac/EH3D-GERBERVIEWER): el visor de capas Gerber
* [EH3D-TOOL-MANAGER](https://github.com/JuanenRac/EH3D-TOOL-MANAGER): el almacén de herramientas que comparten los programas CAM
* [EH3D-SUITE-LAUNCHER](https://github.com/JuanenRac/EH3D-SUITE-LAUNCHER): el centro que encuentra y arranca los programas de la suite

## 📚 Documentación

* [Registro de cambios](CHANGELOG.md)
* [Datos de arquitectura y estructura](docs/architecture.json): los datos con los que se dibujan las dos imágenes.

## 🤝 Contribuir y comunidad

* Antes de una pull request ejecute `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` y `python -I tools/ci_validate.py`: todo debe pasar. Véanse [CONTRIBUTING.md](CONTRIBUTING.md) y el [código de conducta](CODE_OF_CONDUCT.md).
* El README existe en ocho idiomas: un cambio de texto se hace en los ocho ficheros, y las dos imágenes se dibujan desde `docs/architecture.json` y `docs/structure.json` con `tools/make_architecture.py` y `tools/make_structure.py`.
* Informe de una vulnerabilidad en privado: [SECURITY.md](SECURITY.md). Preguntas, ideas e informes: electrohobby3d@gmail.com ([SUPPORT.md](SUPPORT.md)).

## 👤 AUTOR

**JuanenRac (Electro Hobby 3D)** · electrohobby3d@gmail.com

## 📜 LICENCIA

Software: GPL-3.0-or-later. Véase [LICENSE](LICENSE). Los diseños de hardware de la suite usan CERN OHL-S.

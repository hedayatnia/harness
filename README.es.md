<p align="center">
  <img src="docs/brand/harness-icon.png" width="108" height="108" alt="Icono de la app Harness">
</p>

<h1 align="center">Harness</h1>

<p align="center">
  <strong>Un espacio de trabajo nativo para tus agentes de programación.</strong><br>
  Trabaja en local, cambia de máquina y mantén la conversación unida.
</p>

<p align="center">
  <a href="#el-espacio-de-trabajo">Ver la app</a> ·
  <a href="#primeros-pasos">Primeros pasos</a> ·
  <a href="#codegraff">CodeGraff</a> ·
  <a href="#licencia">Licencia</a>
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.de.md">Deutsch</a> ·
  Español ·
  <a href="README.hindi.md">हिन्दी</a> ·
  <a href="README.thai.md">ไทย</a> ·
  <a href="README.indo.md">Bahasa Indonesia</a> ·
  <a href="README.malay.md">Bahasa Melayu</a>
</p>

<p align="center">
  <img src="docs/media/readme/codegraff-chat-dark.png" width="960" alt="Conversación en Harness con el tema Codegraff Dark y el agente graff">
  <br><sub>Harness con graff seleccionado y el tema integrado Codegraff Dark. Conversación de ejemplo sintética.</sub>
</p>

## El espacio de trabajo

Harness reúne **CodeGraff (graff)**, Claude Code, Codex, Cursor, Devin, Grok,
Hermes, Pi, OpenCode, Antigravity y [Exo](https://github.com/exoharness/exo)
en una sola app de escritorio. Elige el agente y el modelo de cada
conversación, mantén juntos los espacios de trabajo y las sesiones, y revisa
los cambios junto al chat. Los archivos, un navegador y las terminales quedan
siempre a mano.

Exo viene desactivado. Harness se conecta a él mediante el adaptador agent-cli
de Exo (`./exo.sh --setup agent-cli`) a través de un puente ACP integrado; en
cuanto el socket del adaptador esté activo, activa Exo en
**Settings → Agents**.

<p align="center">
  <img src="docs/media/readme/codegraff-agents-dark.png" width="960" alt="Ajustes actuales de agentes de Harness con el tema Codegraff Dark, con graff en primer lugar">
  <br><sub>Los agentes instalados aparecen en <strong>Settings → Agents</strong>. Inicia sesión cuando quieras que otro dispositivo de confianza siga o controle una sesión.</sub>
</p>

Harness arranca en modo local, sin cuenta. La app de escritorio usa Rust y
GPUI; su compañera para iOS es una vista en SwiftUI de las sesiones
sincronizadas.

## Primeros pasos

Compila desde una copia del repositorio con la versión de Rust indicada en
[`rust-toolchain.toml`](rust-toolchain.toml):

| Plataforma | Ejecuta |
| --- | --- |
| macOS | `./scripts/run-macos-dev.sh` |
| Linux | `cargo build -p harness && ./target/debug/harness` |
| Windows | `cargo run --locked -p harness` |

El script de macOS abre `target/macos-dev/Harness.app`. Los datos de
desarrollo se guardan en `target/macos-dev/data`, salvo que definas
`HARNESS_DEV_DATA_DIR`. Cierra la app antes de volver a compilar. Para un
recorrido sin conexión con sesiones de ejemplo, ejecuta
`./scripts/dev-demo.sh`.

Las versiones portables para Windows usan `harness.exe`; deja
`harness-update.json` a su lado para las actualizaciones desde la app.
Consulta [desarrollo en Windows](docs/reference/windows-development.md).
El navegador de la barra lateral en Linux necesita el
[entorno del navegador](docs/reference/linux-browser.md).

### Arch Linux

Descarga `harness-VERSION-linux-x86_64.pkg.tar.zst` (o `-aarch64`) desde
[Releases](https://github.com/justrach/harness/releases) e instálalo con
`sudo pacman -U ./harness-VERSION-linux-x86_64.pkg.tar.zst`. Hace falta un
controlador Vulkan para el renderizado. Las instalaciones con pacman nunca se
actualizan solas: vuelve a instalar el nuevo `.pkg.tar.zst` en cada versión.

## CodeGraff

Harness incluye un adaptador de primera clase para
[CodeGraff](https://github.com/justrach/codegraff). Su icono se diseñó para
convivir con el logotipo de CodeGraff. Las imágenes de abajo proceden de
CodeGraff y muestran el lenguaje visual y el flujo de contexto en los que se
basa graff.

<p align="center">
  <img src="docs/brand/codegraff-emblem.png" width="82" alt="Emblema de CodeGraff">
  &nbsp;&nbsp;
  <img src="docs/brand/codegraff-workshop.png" width="156" alt="Ilustración del taller de CodeGraff">
</p>

<p align="center">
  <img src="docs/brand/codegraff-context.png" width="900" alt="Cómo circula el contexto en graff">
</p>

La app de macOS incluye una CLI `graff` gestionada. Mientras Harness está
abierto, comprueba la
[última versión estable de CodeGraff](https://github.com/justrach/codegraff/releases)
poco después de arrancar y cada seis horas. Solo instala una versión más
reciente tras verificar la suma SHA-256 del archivo de la versión. También
puedes actualizar graff en **Settings → Agents**. Elige **Install beta** ahí
para usar la beta publicada más reciente de CodeGraff en su rama de versión
actual. Esto actualiza el motor CLI gestionado por Harness para las nuevas
sesiones de Graff; las sesiones abiertas conservan su motor actual. Vuelve a
pulsar **Install beta** para obtener una beta más nueva; las comprobaciones
estables en segundo plano mantienen la beta elegida. La acción **Update**
vuelve a la versión estable.
Define `HARNESS_GRAFF_AUTO_UPDATE=0` para desactivar las comprobaciones en
segundo plano; Harness no toca un `GRAFF_EXECUTABLE` personalizado.

Los créditos de las imágenes de CodeGraff están en los
[avisos de terceros](THIRD_PARTY_NOTICES.md).

## Dispositivos y cuentas

Cada dispositivo de escritorio ejecuta un motor y guarda sus sesiones locales.
La interfaz se conecta a un motor en el puerto IPC local o arranca uno dentro
del proceso de la app. `harness headless` ejecuta el motor sin ventana.

En la app de escritorio, abre el menú de la cuenta y elige **Enable sync**.
Inicia sesión con tu cuenta de CodeGraff y decide si llevas el trabajo local
existente al espacio sincronizado o empiezas de cero. Harness no sube las
sesiones locales existentes hasta que eliges **Bring my work**. El trabajo
nuevo en el espacio sincronizado se guarda en `edge.codegraff.com` y aparece
en tus otros dispositivos con la sesión iniciada.

En un dispositivo sin interfaz, detén el motor antes de cambiar de perfil:

```sh
harness daemon stop
harness login
harness daemon start
```

La sincronización de Harness usa OAuth de CodeGraff; iniciar sesión en el
agente `graff` es un paso aparte. Los agentes de programación guardan sus
propias credenciales de proveedor en el dispositivo anfitrión. El contenido de
los chats sincronizados y los metadatos del espacio de trabajo van al servicio
edge de Harness. Un dispositivo remoto de confianza puede pedir a un anfitrión
conectado que lea y escriba archivos del espacio de trabajo; **Show ignored
files** también expone archivos ignorados como `.env`. Inicia sesión solo en
dispositivos a los que confiarías esos archivos. Para volver al perfil local en
un dispositivo sin interfaz, detén el daemon, ejecuta `harness logout` y
vuelve a arrancarlo.

Para los detalles internos, consulta la [arquitectura](ARCHITECTURE.md). Para
la apariencia y la importación de temas, consulta
[la guía de temas](docs/theme-system.md).

## Licencia

**GNU AGPL-3.0 modificada** ([`LICENSE`](LICENSE)), construida igual que la de
[CodeGraff](https://github.com/justrach/codegraff). El uso en red activa la
Sección 13. Standard Harness Pte. Ltd., **Rach Pradhan (justrach)** y
**Yu Xi Lim (yxlyx)** se reservan el derecho de ofrecer versiones propietarias
o alojadas de las contribuciones originales de Harness que les pertenecen. La
licencia AGPL de quien la recibe es perpetua salvo que la incumpla. El permiso
comercial sin copyleft solo existe si **los tres lo conceden conjuntamente y
por escrito**, y es revocable. El material anterior y el de terceros conserva
sus propios avisos de copyright y de licencia; consulta los
[avisos de terceros](THIRD_PARTY_NOTICES.md).

**Las empresas necesitan una licencia comercial.** Cada versión posterior a
v0.2.108 añade una condición. Tu organización, contada junto con sus
filiales, necesita una licencia comercial para cualquier uso de Harness
(incluidos sus propios equipos, nube, CI y red) si cumple **cualquiera** de
estas condiciones:

- ha recaudado más de **500.000 US$** en total de inversores o prestamistas;
- su patrimonio neto o su última valoración supera los **500.000 US$**;
- sus ingresos brutos superaron los **100.000 US$** en su último ejercicio
  fiscal o en cualquier periodo de 12 meses.

Una empresa que supere uno de estos umbrales debe contactarnos en un plazo de
5 días laborables. Las personas físicas siempre usan Harness bajo la AGPL,
ganen lo que ganen, y lo mismo las organizaciones que no cumplen ninguna de
estas condiciones. Las versiones hasta la v0.2.108 conservan la licencia con
la que se publicaron. ¿Tienes dudas? Considera que te afecta y ponte en
contacto con nosotros.
Contacto: [rach@standardharness.com](mailto:rach@standardharness.com).

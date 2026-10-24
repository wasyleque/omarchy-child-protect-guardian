# Omarchy Child Protect Guardian 🛡️

**🌐 Idiomas:** [English](README.md) · [Polski](README.pl.md) · **Español** · [Deutsch](README.de.md) · [Français](README.fr.md) · [中文](README.zh.md)

> Control parental y seguridad infantil en línea para **Omarchy Linux** (Arch + Hyprland),
> con **aprobación remota de instalaciones de apps** desde el teléfono del padre/madre:
> un flujo de aprobación push igual que confirmar un inicio de sesión 2FA.

> **Privacidad:** activa el **cifrado del payload** (`[ntfy].encrypt_payload`) y el broker solo verá
> texto cifrado: el nombre del paquete, el origen y el comando van sellados con la clave del padre/madre
> (verificado de extremo a extremo en un teléfono real). Las decisiones además van firmadas
> criptográficamente en todos los casos. Para máxima privacidad también puedes usar un **ntfy autoalojado con auth/ACLs**.

Objetivo ambicioso: ser **más cómodo y más eficaz** que Microsoft Family Safety,
Apple Screen Time, Google Family Link y Qustodio, manteniendo la **privacidad** (sin telemetría,
sin enviar los datos del menor a la nube de una corporación).

---

## 🚀 Instalación y actualización

> **Guía completa de despliegue ordenada por seguridad:** **[`packaging/README.md`](packaging/README.md)**
> (base de hardware → demonio → hook de pacman → salida nftables → fapolicyd → endurecimiento → actualización). Versión corta:

**En el ordenador del menor (Arch / Omarchy):**
```bash
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
sudo packaging/install.sh                   # seguro por defecto (solo bloquea 'pacman -S sl'); añade --enforce cuando estés listo
sudo packaging/setup-child.sh <nombre-niño> # cuenta restringida del menor (sin sudo) + QR de vinculación
```
`install.sh --enforce` también instala y activa el **filtro de contenido DNS** (lista de bloqueo de
pornografía/categorías + SafeSearch) y las reglas de salida **nftables**, y hace que persistan tras
reiniciar. Un solo comando guiado y reversible activa el filtrado como prefieras:
```bash
sudo /usr/lib/guardian/guardian-dns-mode.sh child-only      # solo se filtra al menor (recomendado)
sudo /usr/lib/guardian/guardian-dns-mode.sh whole-machine   # todo el ordenador (incluido el adulto)
```
Después aplica el resto de capas específicas del equipo — la lista blanca de ejecución de **fapolicyd**
(`guardian-fapolicyd-setup.sh`, *obligatoria* para una protección real), el bloqueo de navegadores/NM y la
base de hardware — todo explicado en [`packaging/README.md`](packaging/README.md). Compruébalo de extremo
a extremo en cualquier momento con `sudo packaging/smoke-test.sh <menor>` (incluso confirma que un dominio
porno queda bloqueado para el menor).

**En el teléfono del padre/madre:** instala la app de Android desde la
**[página de Releases](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** y
escanea el QR de vinculación — detalles en [📱 App de Android para padres](#-app-de-android-para-padres).
¿No tienes Android? Consulta [🍏 App para padres en iPhone/iPad, Windows, Mac o Linux](#-app-para-padres-en-iphoneipad-windows-mac-o-linux-sin-necesidad-de-android).

**Actualizar más adelante:** `git pull` → `cargo build --release` → `sudo packaging/install.sh` →
`sudo systemctl restart guardiand` (con fapolicyd en modo enforcing, vuelve a confiar primero en los
nuevos binarios) — pasos completos en [`packaging/README.md` → *7. Updating*](packaging/README.md#7-updating-to-a-new-version).
El APK del teléfono se actualiza **en el sitio** sobre el anterior (conserva la vinculación).

---

## Por qué Linux/Omarchy gana aquí

Cosas que la competencia en Windows/macOS/Android **no puede** hacer, y nosotros sí:

1. **Zero-Bypass (realmente a prueba de manipulación).** En Windows/macOS los niños matan el
   proceso en el administrador de tareas o restablecen permisos. Aquí el demonio está protegido
   por Polkit, el menor no tiene `sudo` y cgroups v2 lo hacen imposible de matar. El filtrado de
   red ocurre en el kernel (`nftables`/eBPF), así que VPN gratuitas, proxies y Tor no escapan de la política.
2. **Control a nivel del compositor (Hyprland/Wayland).** Podemos *congelar* un proceso
   (`SIGSTOP`), difuminar una ventana no autorizada, bloquear la captura/compartición de pantalla,
   sin extensiones invasivas en el navegador.
3. **Cero sobrecarga, 100% privacidad.** Sin bloatware ni telemetría; las políticas se aplican
   localmente y sin conexión. La nube solo **transmite** la decisión del adulto.
4. **Filtrado de todo el sistema, no por navegador.** Una sola política DNS/red cubre cada app
   (juegos, lanzadores, mensajería), no solo Chrome.

## Pilares del producto

- **Aprobación remota de instalaciones** — se retiene un intento de `pacman`/`yay`/`flatpak`;
  el adulto recibe un push con una descripción legible de la app y pulsa *Permitir / Denegar*.
- **Presupuesto de tiempo y horario** — aplicado con firmeza mediante cgroups freeze + idle de Hyprland.
- **Filtro de contenido de todo el sistema** — un resolvedor local con filtrado bloquea dominios de
  pornografía/categorías (lista StevenBlack, actualizada a diario) y fuerza SafeSearch en
  Google/YouTube/Bing para *cada* aplicación, no solo el navegador. Instalado y persistente tras el
  arranque; un solo comando guiado elige entre filtrado **solo para el menor** o **para todo el ordenador**
  y es totalmente reversible. Las vías de escape DoH/DoT/VPN se bloquean en el kernel (`nftables`).
- **Informes legibles para el adulto** — qué hizo y pidió el menor (sin espionaje corporativo).
- **Aprobaciones firmadas criptográficamente — y opcionalmente cifradas** — ni un broker push
  comprometido puede falsificar un "Permitir", y con el cifrado del payload activo ni siquiera puede ver
  el nombre del paquete o el comando.
- **Root remoto acotado (Intent Binding)** — una aprobación concede root para *exactamente una* acción
  descrita, nunca una shell; vinculada criptográficamente a lo que el adulto vio realmente.
- **Asistente IA de riesgos** — comprobación bajo demanda en lenguaje sencillo (semáforo + traductor de
  permisos); solo metadatos públicos de la app, ningún dato del menor sale del dispositivo.
- **Clasificación por edades abierta LACS** — escala comunitaria y multidimensional más allá de
  PEGI/ESRB, cubriendo apps de Linux/AUR/Flathub.
- **Estadísticas suaves y resumen IA para padres** — horas, apps y categorías de sitios, más un resumen
  que orienta hacia la conversación, no un dossier de vigilancia.

## Documentación

| Documento | Contenido |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) / [`docs/KONCEPCJA.md`](docs/KONCEPCJA.md) | Concepto completo, ventajas, casos de uso, ideas no obvias |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) / [`docs/ARCHITEKTURA.md`](docs/ARCHITEKTURA.md) | Arquitectura: demonio, puntos de intercepción, push-approval, seguridad |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Etapas de construcción (MVP → v1), en pasos pequeños y verificables |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Diseño anti-bypass: niveles de adversario, cada vector → su mitigación, límites honestos |
| [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) · [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md) | Respuestas a las dos auditorías de seguridad externas (cada hallazgo → corrección/estado) |
| [`android-app/`](android-app/) | **App nativa de Android para padres** (Kotlin) — cliente recomendado; firma Ed25519 en el dispositivo, notificaciones en segundo plano |
| [`parent-app/`](parent-app/) | App multiplataforma para padres (PWA instalable) — firma Ed25519 en el dispositivo |
| [`packaging/`](packaging/) | Despliegue: instalador, unidad systemd, hook pacman, salida nftables, lista blanca fapolicyd, auditoría |
| [`docs/TESTING.md`](docs/TESTING.md) | Plan de pruebas paso a paso en un sistema limpio (capa por capa + intentos de evasión red-team) |

## 📱 App de Android para padres

El cliente recomendado para el padre/madre es una **app nativa de Android** (Kotlin): descárgala desde la
**[página de Releases](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** y
vincúlala escaneando un código QR. Interfaz en **English · Polski · Español · Deutsch · Français · 中文**
(sigue el idioma del teléfono).

<p align="center">
  <img src="docs/screenshots/android-app.png" alt="Guardian Parent — Android app" width="320">
</p>

Qué hace:

- **Aprobar o denegar instalaciones** — cuando el menor ejecuta `pacman`/`yay`/`flatpak`/`snap`/`nix`, el
  intento se retiene y recibes un push con el nombre de la app, el comando exacto y un semáforo de riesgo
  IA 🟢/🟡/🔴; pulsa **Permitir / Denegar** desde la notificación *o* desde la lista en la app. Cada
  decisión se firma en el dispositivo, de modo que ni un broker comprometido puede falsificar un "Permitir".
- **Varios niños en un solo teléfono** — añade a cada menor (su propio PC o un PC compartido) y cambia
  entre ellos; cada solicitud/notificación lleva el nombre del menor.
- **Horario de pantalla sin escribir** — elige un límite diario de un desplegable y las horas permitidas
  con un selector de hora, igual todos los días o por día de la semana. Se envía firmado al ordenador.
- **Bloqueo instantáneo** — bloquea el ordenador del menor ahora mismo (y desbloquéalo) desde el móvil.
- **Tiempo extra** — concede a un menor concreto +15 / +30 minutos con un toque.
- **Resumen diario con IA** — un resumen opcional una vez al día sobre qué se usó, si el menor intentó
  algo que no debía y una advertencia clara si aparecieron temas preocupantes (drogas, autolesiones,
  armas…), generado por un modelo **local** en tu propia máquina para que nada salga de tu red.

## 🍏 App para padres en iPhone/iPad, Windows, Mac o Linux (sin necesidad de Android)

¿No tienes un teléfono Android? La app para padres también funciona como **aplicación web instalable (PWA)**:
la misma firma en el dispositivo, pero abierta en el navegador. Sin App Store, sin cuenta.

1. **Abre el enlace de la app** en tu navegador (tu copia alojada de [`parent-app/`](parent-app/) sobre
   HTTPS, p. ej. una URL gratuita de GitHub Pages; la configuración está en [`parent-app/README.md`](parent-app/README.md)).
2. **Instálala:**
   - **iPhone / iPad:** toca **Compartir → Añadir a pantalla de inicio**, y ábrela siempre desde ese
     nuevo icono. (Apple solo permite notificaciones en apps web abiertas así — iOS 16.4 o posterior.)
   - **Windows / Mac / Linux:** haz clic en **Instalar** en la barra de direcciones del navegador
     (Chrome/Edge) para tenerla como app de escritorio, o simplemente usa la pestaña.
3. **Vincúlala:** escanea el mismo código QR mostrado en el ordenador del menor (o pega una vez el
   servidor + tema).
4. **Activa los avisos en segundo plano:** toca **Enable background notifications** para que una
   solicitud de instalación suene aunque la app esté cerrada. Al tocar la notificación se abre la app y
   muestra la solicitud real verificada criptográficamente.

Obtienes lo esencial con seguridad: **Permitir / Denegar firmados** que un broker secuestrado no puede
falsificar, tu clave nunca sale del dispositivo y funciona sin conexión salvo para el flujo en vivo de
solicitudes. La **app de Android sigue siendo el cliente más completo** (editor de horario, bloqueo
instantáneo, concesión de tiempo extra, vista del resumen diario y ajustes del servidor IA); la app web
se centra en **aprobaciones y alertas**. Elige la que mejor se adapte a tu teléfono.

## Estado

✅ **Funcionando — en pruebas activas, aún no como única protección del menor.** El núcleo está
construido y probado (unit/e2e), corre en hardware real (el bucle de aprobación push, tiempo de pantalla,
fapolicyd, nftables y la cadena de auditoría funcionan según lo diseñado), y la app nativa de Android está
instalada y operativa. Dos auditorías de seguridad externas (2026-10-09, 2026-10-14) y tres pruebas en
hardware real (2026-10-21/23) impulsaron correcciones de fallos reales. **Hasta que configures y
verifiques en tu máquina la base completa de hardware + fapolicyd en modo enforcing, trátalo como un
apoyo sólido, no como la única línea de defensa.** Consulta las auditorías
[`guardian-security-review.md`](guardian-security-review.md) + [`guardian-security-review-2.md`](guardian-security-review-2.md),
las respuestas [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) + [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md)
y los informes en hardware [`guardian-test-report-2026-10-21.md`](guardian-test-report-2026-10-21.md) +
[`guardian-test-report-2026-10-23.md`](guardian-test-report-2026-10-23.md). Despliegue según [`docs/TESTING.md`](docs/TESTING.md).

---

## 👤 Autor

**Creado por [wasyleque](https://github.com/wasyleque).**

## ❤️ Apoya el proyecto

Si Omarchy Child Protect Guardian te resulta útil, puedes apoyar su desarrollo vía **PayPal**:
**[wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)**

Cada aporte ayuda a mantenerlo gratuito, privado y abierto.

## 🤝 Contribuye y comparte ideas

Este proyecto crece con las ideas de la comunidad. **Te invitamos cordialmente a:**
- 💡 **compartir ideas** — abre un [Issue](../../issues) con la etiqueta `idea` (hay plantilla),
- 🛠️ **co-crear funciones** — elige algo del [Roadmap](docs/ROADMAP.md) y abre un PR,
- 🌍 **traducir la documentación** a más idiomas.

Consulta [`CONTRIBUTING.md`](CONTRIBUTING.md). Ninguna idea es demasiado pequeña: construyamos
juntos la mejor herramienta de seguridad infantil de cualquier plataforma.

## Licencia

Por definir (propuesta: GPL-3.0 — una herramienta de seguridad cuyo valor nace de ser abierta y auditable).

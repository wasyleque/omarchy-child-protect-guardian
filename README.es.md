# Omarchy Child Protect Guardian 🛡️

**🌐 Idiomas:** [English](README.md) · [Polski](README.pl.md) · **Español** · [Deutsch](README.de.md) · [Français](README.fr.md) · [中文](README.zh.md)

> Control parental y seguridad infantil en línea para **Omarchy Linux** (Arch + Hyprland),
> con **aprobación remota de instalaciones de apps** desde el teléfono del padre/madre:
> un flujo de aprobación push igual que confirmar un inicio de sesión 2FA.

**Estado:** 🌱 borrador conceptual (fase 0 — recopilando ideas). Ver [`AGENTS.md`](AGENTS.md).

Objetivo ambicioso: ser **más cómodo y más eficaz** que Microsoft Family Safety,
Apple Screen Time, Google Family Link y Qustodio, manteniendo la **privacidad** (sin telemetría,
sin enviar los datos del menor a la nube de una corporación).

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
- **Filtro de contenido de todo el sistema** — DNS + eBPF, SafeSearch / modo restringido forzados.
- **Informes legibles para el adulto** — qué hizo y pidió el menor (sin espionaje corporativo).
- **Aprobaciones firmadas criptográficamente** — ni un broker push comprometido puede falsificar un "Permitir".

## Documentación

| Documento | Contenido |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) | Concepto completo, ventajas, casos de uso, ideas no obvias |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) | Arquitectura: demonio, puntos de intercepción, push-approval, seguridad |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Etapas de construcción (MVP → v1), en pasos pequeños y verificables |

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

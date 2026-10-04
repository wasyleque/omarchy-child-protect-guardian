# Omarchy Child Protect Guardian 🛡️

**🌐 Sprachen:** [English](README.md) · [Polski](README.pl.md) · [Español](README.es.md) · **Deutsch** · [Français](README.fr.md) · [中文](README.zh.md)

> Kindersicherung & Online-Kinderschutz für **Omarchy Linux** (Arch + Hyprland),
> mit **Freigabe von App-Installationen aus der Ferne** über das Elternhandy — ein
> Push-Approval-Ablauf, genau wie die Bestätigung eines 2FA-Logins.

**Status:** 🌱 Konzeptentwurf (Phase 0 — Ideensammlung). Siehe [`AGENTS.md`](AGENTS.md).

Ehrgeiziges Ziel: **bequemer und wirksamer** sein als Microsoft Family Safety,
Apple Screen Time, Google Family Link und Qustodio — und dabei **privat** bleiben (keine Telemetrie,
keine Übertragung der Kinderdaten in eine Konzern-Cloud).

---

## Warum Linux/Omarchy hier gewinnt

Dinge, die die Konkurrenz auf Windows/macOS/Android **nicht** kann, wir aber schon:

1. **Zero-Bypass (wirklich manipulationssicher).** Auf Windows/macOS beenden Kinder den Prozess
   im Task-Manager oder setzen Rechte zurück. Hier ist der Daemon durch Polkit geschützt, das Kind
   hat kein `sudo`, und cgroups v2 machen ihn unabschießbar. Die Netzwerkfilterung läuft im Kernel
   (`nftables`/eBPF), sodass kostenlose VPNs, Proxys und Tor nicht an der Richtlinie vorbeikommen.
2. **Kontrolle auf Compositor-Ebene (Hyprland/Wayland).** Wir können einen Prozess *einfrieren*
   (`SIGSTOP`), ein nicht autorisiertes Fenster weichzeichnen, Bildschirmaufnahme/-freigabe
   blockieren — ohne invasive Browser-Erweiterungen.
3. **Kein Overhead, 100% Privatsphäre.** Keine Bloatware, keine Telemetrie; Richtlinien laufen
   lokal und offline. Die Cloud **übermittelt nur** die Entscheidung der Eltern.
4. **Systemweite Filterung, nicht pro Browser.** Eine DNS-/Netzwerkrichtlinie deckt jede App ab
   (Spiele, Launcher, Messenger), nicht nur Chrome.

## Produktsäulen

- **Ferngesteuerte Installationsfreigabe** — ein `pacman`/`yay`/`flatpak`-Versuch wird angehalten;
  die Eltern erhalten einen Push mit lesbarer App-Beschreibung und tippen *Erlauben / Ablehnen*.
- **Zeitbudget & Zeitplan** — hart durchgesetzt via cgroups-Freeze + Hyprland-Idle.
- **Systemweiter Inhaltsfilter** — DNS + eBPF, erzwungenes SafeSearch / eingeschränkter Modus.
- **Lesbare Berichte für die Eltern** — was das Kind getan und angefragt hat (keine Konzern-Spionage).
- **Kryptografisch signierte Freigaben** — selbst ein kompromittierter Push-Broker kann kein „Erlauben" fälschen.

## Dokumentation

| Dokument | Inhalt |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) | Vollständiges Konzept, Vorteile, Anwendungsfälle, nicht offensichtliche Ideen |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) | Architektur: Daemon, Abfangpunkte, Push-Approval, Sicherheit |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Baustufen (MVP → v1), in kleine überprüfbare Schritte unterteilt |

---

## 👤 Autor

**Erstellt von [wasyleque](https://github.com/wasyleque).**

## ❤️ Projekt unterstützen

Wenn Omarchy Child Protect Guardian dir nützt, kannst du die Entwicklung per **PayPal** unterstützen:
**[wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)**

Jeder Beitrag hilft, das Projekt kostenlos, privat und offen zu halten.

## 🤝 Mitmachen & Ideen teilen

Dieses Projekt lebt von den Ideen der Community. **Wir laden dich herzlich ein:**
- 💡 **Ideen teilen** — öffne ein [Issue](../../issues) mit dem Label `idea` (Vorlage vorhanden),
- 🛠️ **Funktionen mitgestalten** — wähle etwas aus der [Roadmap](docs/ROADMAP.md) und öffne einen PR,
- 🌍 **die Doku übersetzen** in weitere Sprachen.

Siehe [`CONTRIBUTING.md`](CONTRIBUTING.md). Keine Idee ist zu klein — lass uns gemeinsam das beste
Kinderschutz-Tool auf jeder Plattform bauen.

## Lizenz

Noch offen (Vorschlag: GPL-3.0 — ein Sicherheitstool, dessen Wert aus Offenheit und Prüfbarkeit entsteht).

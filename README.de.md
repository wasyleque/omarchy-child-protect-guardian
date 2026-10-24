# Omarchy Child Protect Guardian 🛡️

**🌐 Sprachen:** [English](README.md) · [Polski](README.pl.md) · [Español](README.es.md) · **Deutsch** · [Français](README.fr.md) · [中文](README.zh.md)

> Kindersicherung & Online-Kinderschutz für **Omarchy Linux** (Arch + Hyprland),
> mit **Freigabe von App-Installationen aus der Ferne** über das Elternhandy — ein
> Push-Approval-Ablauf, genau wie die Bestätigung eines 2FA-Logins.

> **Privatsphäre:** Schalte die **Payload-Verschlüsselung** (`[ntfy].encrypt_payload`) ein und der Broker
> sieht nur noch Geheimtext — Paketname, Quelle und Befehl sind für den Schlüssel der Eltern versiegelt
> (Ende-zu-Ende auf einem echten Smartphone bestätigt). Entscheidungen sind außerdem in jedem Fall
> kryptografisch signiert. Für maximale Privatsphäre kannst du weiterhin einen **selbst gehosteten ntfy mit Auth/ACLs** betreiben.

Ehrgeiziges Ziel: **bequemer und wirksamer** sein als Microsoft Family Safety,
Apple Screen Time, Google Family Link und Qustodio — und dabei **privat** bleiben (keine Telemetrie,
keine Übertragung der Kinderdaten in eine Konzern-Cloud).

---

## 🚀 Installation & Update

> **Vollständige, nach Sicherheit geordnete Installationsanleitung:** **[`packaging/README.md`](packaging/README.md)**
> (Hardware-Basis → Daemon → Pacman-Hook → nftables-Egress → fapolicyd → Härtung → Update). Kurzfassung:

**Auf dem Computer des Kindes (Arch / Omarchy):**
```bash
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
sudo packaging/install.sh                   # sicherer Standard (nur 'pacman -S sl' abgefangen); --enforce wenn bereit
sudo packaging/setup-child.sh <kind-name>   # eingeschränktes Kinderkonto (kein sudo) + Kopplungs-QR
```
`install.sh --enforce` installiert und aktiviert außerdem den **DNS-Inhaltsfilter**
(Porno-/Kategorien-Sperrliste + SafeSearch) sowie die **nftables**-Egress-Regeln und macht sie
neustartfest. Ein einziger geführter, umkehrbarer Befehl schaltet die Filterung nach Wunsch ein:
```bash
sudo /usr/lib/guardian/guardian-dns-mode.sh child-only      # nur das Kind wird gefiltert (empfohlen)
sudo /usr/lib/guardian/guardian-dns-mode.sh whole-machine   # der gesamte Computer (inkl. Eltern)
```
Anschließend die übrigen hostspezifischen Schichten anwenden — die **fapolicyd**-Ausführungs-Allowlist
(`guardian-fapolicyd-setup.sh`, *erforderlich* für echte Durchsetzung), Browser-/NM-Sperre und die
Hardware-Basis — alles in [`packaging/README.md`](packaging/README.md). Jederzeit Ende-zu-Ende prüfen mit
`sudo packaging/smoke-test.sh <kind>` (prüft sogar, ob eine Porno-Domain für das Kind blockiert wird).

**Auf dem Telefon der Eltern:** die Android-App von der
**[Releases-Seite](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)**
installieren und den Kopplungs-QR scannen — Details unter [📱 Android-Eltern-App](#-android-eltern-app).
Kein Android? Siehe [🍏 Eltern-App auf iPhone/iPad, Windows, Mac oder Linux](#-eltern-app-auf-iphoneipad-windows-mac-oder-linux-kein-android-nötig).

**Später aktualisieren:** `git pull` → `cargo build --release` → `sudo packaging/install.sh` →
`sudo systemctl restart guardiand` (unter aktivem fapolicyd zuerst den neuen Binärdateien neu vertrauen)
— alle Schritte in [`packaging/README.md` → *7. Updating*](packaging/README.md#7-updating-to-a-new-version).
Die APK auf dem Telefon installiert sich **direkt über** die alte Version (Kopplung bleibt erhalten).

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
- **Systemweiter Inhaltsfilter** — ein lokaler Filter-Resolver blockiert Porno-/Kategorie-Domains
  (StevenBlack-Liste, täglich aktualisiert) und erzwingt SafeSearch auf Google/YouTube/Bing für *jede*
  App, nicht nur im Browser. Installiert und neustartfest; ein geführter Befehl wählt zwischen Filterung
  **nur für das Kind** oder für den **gesamten Computer** und ist vollständig umkehrbar.
  DoH/DoT/VPN-Umgehungen werden im Kernel (`nftables`) blockiert.
- **Lesbare Berichte für die Eltern** — was das Kind getan und angefragt hat (keine Konzern-Spionage).
- **Kryptografisch signierte — und optional verschlüsselte — Freigaben** — selbst ein kompromittierter
  Push-Broker kann kein „Erlauben" fälschen, und mit aktiver Payload-Verschlüsselung sieht er nicht
  einmal den Paketnamen oder Befehl.
- **Begrenztes Remote-Root (Intent Binding)** — eine Freigabe gewährt Root-Rechte für *genau eine*
  beschriebene Aktion, niemals eine Shell; kryptografisch an das gebunden, was die Eltern gesehen haben.
- **KI-Risikoassistent** — verständliche Risikoprüfung auf Abruf (Ampel + Berechtigungsübersetzer); nur
  öffentliche App-Metadaten, keinerlei Kinderdaten verlassen das Gerät.
- **Offene Altersfreigaben LACS** — mehrdimensionale Community-Altersskala jenseits von PEGI/ESRB für
  Linux/AUR/Flathub-Apps.
- **Sanfte Statistiken & KI-Eltern-Zusammenfassung** — Stunden, Apps und Seitenkategorien sowie eine
  Zusammenfassung, die das Gespräch fördert — kein Überwachungsdossier.

## Dokumentation

| Dokument | Inhalt |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) / [`docs/KONCEPCJA.md`](docs/KONCEPCJA.md) | Vollständiges Konzept, Vorteile, Anwendungsfälle, nicht offensichtliche Ideen |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) / [`docs/ARCHITEKTURA.md`](docs/ARCHITEKTURA.md) | Architektur: Daemon, Abfangpunkte, Push-Approval, Sicherheit |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Baustufen (MVP → v1), in kleine überprüfbare Schritte unterteilt |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Anti-Bypass-Design: Angreiferstufen, jeder Umgehungsvektor → Gegenmaßnahme, ehrliche Grenzen |
| [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) · [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md) | Antworten auf die zwei externen Sicherheitsprüfungen (jeder Befund → Fix/Status) |
| [`android-app/`](android-app/) | **Native Android-Eltern-App** (Kotlin) — empfohlener Eltern-Client; Ed25519-Signatur auf dem Gerät, Hintergrund-Push |
| [`parent-app/`](parent-app/) | Plattformübergreifende Eltern-App (installierbare PWA) — Ed25519-Signatur auf dem Gerät |
| [`packaging/`](packaging/) | Bereitstellung: Installer, systemd-Unit, Pacman-Hook, nftables-Egress, fapolicyd-Allowlist, Härtungs-Audit |
| [`docs/TESTING.md`](docs/TESTING.md) | Schritt-für-Schritt-Testplan für ein sauberes System (Schicht für Schicht + Red-Team-Tests) |

## 📱 Android-Eltern-App

Der empfohlene Client für Eltern ist eine **native Android-App** (Kotlin) — lade sie von der
**[Releases-Seite](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)**
herunter und kopple sie per QR-Code-Scan. Oberfläche auf
**English · Polski · Español · Deutsch · Français · 中文** (folgt der Systemsprache des Telefons).

<p align="center">
  <img src="docs/screenshots/android-app.png" alt="Guardian Parent — Android app" width="320">
</p>

Was sie bietet:

- **Installationen erlauben oder ablehnen** — wenn das Kind `pacman`/`yay`/`flatpak`/`snap`/`nix`
  ausführt, wird der Vorgang angehalten und du erhältst einen Push mit App-Namen, exaktem Befehl und
  einer 🟢/🟡/🔴 **KI-Risikoampel**; tippe **Erlauben / Ablehnen** direkt in der Benachrichtigung *oder*
  in der App-Liste. Jede Entscheidung wird auf dem Gerät signiert, sodass selbst ein kompromittierter
  Broker kein „Erlauben" fälschen kann.
- **Mehrere Kinder, ein Telefon** — füge jedes Kind hinzu (eigener PC oder geteilter PC) und wechsle
  zwischen ihnen; jede Anfrage/Benachrichtigung trägt den Namen des Kindes.
- **Bildschirmzeit-Plan ohne Tippen** — wähle ein Tageslimit per Dropdown und erlaubte Uhrzeiten per
  Zeitwähler, gleich für jeden Tag oder pro Wochentag. Kryptografisch signiert an den PC gesendet.
- **Sofortsperre** — den Computer des Kindes sofort vom Telefon aus sperren (und entsperren).
- **Extra-Zeit** — einem bestimmten Kind mit einem Fingertipp +15 / +30 Minuten gewähren.
- **Tägliche KI-Zusammenfassung** — ein optionaler täglicher Rückblick über genutzte Apps, blockierte
  Versuche und eine deutliche Warnung bei bedenklichen Themen (Drogen, Selbstverletzung, Waffen…),
  erstellt von einem **lokalen** Modell auf deiner eigenen Maschine, sodass nichts dein Netzwerk verlässt.

## 🍏 Eltern-App auf iPhone/iPad, Windows, Mac oder Linux (kein Android nötig)

Kein Android-Smartphone? Die Eltern-App läuft auch als **installierbare Web-App (PWA)** — dieselbe
Signatur auf dem Gerät, einfach im Browser geöffnet. Kein App Store, kein Konto.

1. **App-Link im Browser öffnen** (deine gehostete Kopie von [`parent-app/`](parent-app/) über HTTPS —
   z. B. eine kostenlose GitHub-Pages-URL; Einrichtung in [`parent-app/README.md`](parent-app/README.md)).
2. **Installieren:**
   - **iPhone / iPad:** tippe auf **Teilen → Zum Home-Bildschirm**, und öffne sie danach immer über
     dieses neue Symbol. (Apple erlaubt Benachrichtigungen für Web-Apps nur auf diesem Weg — ab iOS 16.4.)
   - **Windows / Mac / Linux:** klicke auf **Installieren** in der Adressleiste des Browsers
     (Chrome/Edge), um sie als Desktop-App zu nutzen, oder verwende einfach den Tab.
3. **Koppeln:** scanne denselben QR-Code vom Computer des Kindes (oder füge einmal Server + Topic ein).
4. **Hintergrund-Warnungen einschalten:** tippe auf **Enable background notifications**, damit eine
   Installationsanfrage auch bei geschlossener App klingelt. Ein Tipp auf die Benachrichtigung öffnet
   die App und zeigt die echte, signaturgeprüfte Anfrage.

Du erhältst die Kernfunktionen sicher: **signiertes Erlauben / Ablehnen**, das ein gekaperter Broker
nicht fälschen kann, dein Schlüssel verlässt nie das Gerät, und sie funktioniert offline bis auf den
Live-Anfragestrom. Die **Android-App bleibt der vollständigste Client** (Zeitplan-Editor, Sofortsperre,
Extra-Zeit, Ansicht der Tageszusammenfassung und KI-Server-Einstellungen); die Web-App konzentriert sich
auf **Freigaben und Warnungen**. Wähle einfach das, was zu deinem Telefon passt.

## Status

✅ **Funktionsfähig — im aktiven Testbetrieb, noch nicht als alleiniger Schutz für ein Kind gedacht.**
Der Kern ist gebaut und per Unit-/E2E-Tests geprüft, läuft auf echter Hardware (Push-Approval-Schleife,
Bildschirmzeit, fapolicyd, nftables und Audit-Kette verhalten sich wie vorgesehen), und die native
Android-App ist installiert und im Einsatz. Zwei externe Sicherheitsreviews (2026-10-09, 2026-10-14) und
drei Hardware-Tests (2026-10-21/23) führten zur Behebung echter Durchsetzungsfehler. **Solange die
vollständige Hardware-Basis + fapolicyd im Enforcing-Modus auf deiner Maschine noch nicht eingerichtet
und verifiziert sind, betrachte es als starke Unterstützung, nicht als einzige Verteidigungslinie.**
Siehe die Reviews [`guardian-security-review.md`](guardian-security-review.md) +
[`guardian-security-review-2.md`](guardian-security-review-2.md), die Antworten
[`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) + [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md)
und die Testberichte [`guardian-test-report-2026-10-21.md`](guardian-test-report-2026-10-21.md) +
[`guardian-test-report-2026-10-23.md`](guardian-test-report-2026-10-23.md). Bereitstellung gemäß [`docs/TESTING.md`](docs/TESTING.md).

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

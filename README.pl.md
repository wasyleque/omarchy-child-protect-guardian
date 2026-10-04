# Omarchy Child Protect Guardian 🛡️

**🌐 Języki:** [English](README.md) · **Polski** · [Español](README.es.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [中文](README.zh.md)

> Kontrola rodzicielska i bezpieczeństwo dzieci w sieci dla **Omarchy Linux** (Arch + Hyprland),
> z **zdalnym zatwierdzaniem instalacji aplikacji** z telefonu rodzica — push-approval,
> dokładnie jak potwierdzenie logowania 2FA.

**Status:** 🌱 zarys koncepcji (faza 0 — zbieranie pomysłów). Patrz [`AGENTS.md`](AGENTS.md).

Cel ambitny: ma być **wygodniejszy i skuteczniejszy** niż Microsoft Family Safety,
Apple Screen Time, Google Family Link i Qustodio — a jednocześnie **prywatny** (żadnej
telemetrii, żadnego wysyłania danych dziecka do korporacyjnej chmury).

---

## Dlaczego akurat Linux/Omarchy to wygrywa

Rzeczy, których konkurencja na Windows/macOS/Android **nie może** zrobić, a my możemy:

1. **Zero-Bypass (naprawdę nie do obejścia).** Na Windows/macOS dzieci ubijają proces
   w menedżerze zadań albo kasują uprawnienia. Tu demon jest chroniony przez Polkit,
   dziecko nie ma `sudo`, a cgroups v2 czynią go nie do ubicia. Filtrowanie sieci robimy
   w jądrze (`nftables`/eBPF), więc darmowe VPN-y, proxy i Tor nie omijają polityki.
2. **Kontrola na poziomie kompozytora (Hyprland/Wayland).** Możemy *zamrozić* proces
   (`SIGSTOP`), nałożyć blur na nieautoryzowane okno, zablokować zrzut/udostępnianie ekranu —
   bez inwazyjnych wtyczek w przeglądarce.
3. **Zero narzutu i 100% prywatności.** Brak bloatware'u i telemetrii; polityki działają
   lokalnie i offline. Chmura służy **wyłącznie** do przekazania decyzji rodzica.
4. **Filtrowanie dla całego systemu, nie per-przeglądarka.** Jedna polityka DNS/sieci
   obejmuje każdą apkę (gry, launchery, komunikatory), a nie tylko Chrome.

## Filary produktu

- **Zdalne zatwierdzanie instalacji** — próba `pacman`/`yay`/`flatpak` jest wstrzymywana,
  rodzic dostaje push z czytelnym opisem apki i klika *Zezwól / Odrzuć*.
- **Budżet czasu i harmonogram** — egzekwowane twardo przez cgroups freeze + idle Hyprland.
- **Filtr treści dla całego systemu** — DNS + eBPF, wymuszony SafeSearch / tryb ograniczony.
- **Czytelne raporty dla rodzica** — co dziecko robiło i o co prosiło (bez korpo-szpiegowania).
- **Kryptograficznie podpisane zgody** — nawet przejęcie brokera push nie pozwala podrobić „Zezwól".
- **Zdalny root ograniczony do akcji (Intent Binding)** — zgoda daje roota na *dokładnie jedną*
  opisaną akcję, nigdy shell; kryptograficznie związana z tym, co rodzic faktycznie zobaczył.
- **Asystent AI oceny ryzyka** — na żądanie, prostym językiem (światła + tłumacz uprawnień);
  wyłącznie publiczne metadane apki, żadne dane dziecka nie opuszczają urządzenia.
- **Otwarty ranking wiekowy LACS** — społecznościowa, wielowymiarowa skala poza PEGI/ESRB,
  obejmująca apki z Linux/AUR/Flathub.
- **Łagodne statystyki i digest AI dla rodzica** — godziny, aplikacje i kategorie stron oraz
  tygodniowe streszczenie prowadzące do rozmowy — a nie dossier inwigilacji.

## Dokumentacja

| Dokument | Zawartość |
|---|---|
| [`docs/KONCEPCJA.md`](docs/KONCEPCJA.md) | Pełna koncepcja, przewagi, scenariusze, nieoczywiste pomysły |
| [`docs/ARCHITEKTURA.md`](docs/ARCHITEKTURA.md) | Architektura: demon, punkty przechwycenia, push-approval, bezpieczeństwo |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Etapy budowy (MVP → v1), podział na małe weryfikowalne kroki |

## Jak powstaje (workflow)

Model wieloagentowy tej maszyny: **logika/architektura/integracje → `agy`**,
**proste zadania implementacyjne → lokalny Ollama (`qwen3-coder:30b`) przez `aider`**.
`AGENTS.md` pełni rolę pliku statusu.

---

## 👤 Autor

**Stworzone przez [wasyleque](https://github.com/wasyleque).**

## ❤️ Wsparcie projektu

Jeśli Omarchy Child Protect Guardian jest dla Ciebie przydatny, możesz wesprzeć rozwój przez **PayPal**:
**[wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)**

Każda złotówka pomaga utrzymać projekt darmowym, prywatnym i otwartym.

## 🤝 Dołącz i dziel się pomysłami

Ten projekt żyje pomysłami społeczności. **Serdecznie zachęcamy, byś:**
- 💡 **dzielił się pomysłami** — załóż [Issue](../../issues) z etykietą `idea` (jest szablon),
- 🛠️ **współtworzył funkcje** — wybierz coś z [Roadmapy](docs/ROADMAP.md) i otwórz PR,
- 🌍 **tłumaczył dokumentację** na kolejne języki.

Zobacz [`CONTRIBUTING.md`](CONTRIBUTING.md). Żaden pomysł nie jest zbyt mały — zbudujmy razem
najlepsze narzędzie do bezpieczeństwa dzieci na jakiejkolwiek platformie.

## Licencja

Do ustalenia (propozycja: GPL-3.0 — narzędzie bezpieczeństwa, wartość z otwartości i audytowalności).

# Omarchy Child Protect Guardian 🛡️

**🌐 Języki:** [English](README.md) · **Polski** · [Español](README.es.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [中文](README.zh.md)

> Kontrola rodzicielska i bezpieczeństwo dzieci w sieci dla **Omarchy Linux** (Arch + Hyprland),
> z **zdalnym zatwierdzaniem instalacji aplikacji** z telefonu rodzica — push-approval,
> dokładnie jak potwierdzenie logowania 2FA.

**Status:** ✅ **Działa — w fazie aktywnych testów, jeszcze nie jako jedyna ochrona.** Rdzeń jest zbudowany
i przetestowany (unit/e2e), działa na prawdziwym sprzęcie (pętla push-approval, czas ekranowy, fapolicyd,
nftables i łańcuch audytu zachowują się jak zaprojektowano), a natywna apka na Androida jest zainstalowana
i działa. Dwa zewnętrzne audyty (2026-10-09, 2026-10-14) i trzy testy na sprzęcie (2026-10-21/23)
doprowadziły do naprawy realnych błędów egzekucji. **Dopóki nie ustawisz i nie zweryfikujesz u siebie
pełnej bazy sprzętowej + fapolicyd w trybie egzekwowania, traktuj to jako mocne wsparcie, nie jedyną
linię obrony.** Zobacz audyty
[`guardian-security-review.md`](guardian-security-review.md) + [`guardian-security-review-2.md`](guardian-security-review-2.md),
odpowiedzi [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) + [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md)
oraz raporty z testów [`guardian-test-report-2026-10-21.md`](guardian-test-report-2026-10-21.md) +
[`guardian-test-report-2026-10-23.md`](guardian-test-report-2026-10-23.md). Wdrożenie wg [`docs/TESTING.md`](docs/TESTING.md).

Cel ambitny: ma być **wygodniejszy i skuteczniejszy** niż Microsoft Family Safety,
Apple Screen Time, Google Family Link i Qustodio — a jednocześnie **prywatny** (żadnej
telemetrii, żadnego wysyłania danych dziecka do korporacyjnej chmury).

---

## 🚀 Instalacja i aktualizacja

> **Pełny przewodnik wdrożenia (w kolejności bezpieczeństwa):** **[`packaging/README.md`](packaging/README.md)**
> (baza sprzętowa → demon → hook pacmana → nftables → fapolicyd → utwardzenie → aktualizacja). Skrót:

**Na komputerze dziecka (Arch / Omarchy):**
```bash
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
sudo packaging/install.sh                   # bezpiecznie domyślnie (gated tylko 'pacman -S sl'); --enforce gdy gotów
sudo packaging/setup-child.sh <imię-dziecka>  # konto dziecka bez roota + QR do parowania
```
Potem warstwy zależne od hosta — **nftables**, lista dozwolonych wykonań **fapolicyd**
(`guardian-fapolicyd-setup.sh`, *wymagane* do realnej egzekucji), blokada przeglądarek/NM i baza
sprzętowa — wszystko w [`packaging/README.md`](packaging/README.md). Weryfikacja:
`sudo packaging/smoke-test.sh <dziecko>`.

**Na telefonie rodzica:** zainstaluj apkę z
**[strony Releases](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** i
zeskanuj QR parowania — szczegóły w [📱 Android parent app](README.md#-android-parent-app).

**Aktualizacja:** `git pull` → `cargo build --release` → `sudo packaging/install.sh` →
`sudo systemctl restart guardiand` (pod fapolicyd w trybie enforce najpierw ponownie zaufaj nowym
binarkom) — pełne kroki w [`packaging/README.md` → *7. Updating*](packaging/README.md#7-updating-to-a-new-version).
APK na telefonie instaluje się **w miejscu** (parowanie zostaje).

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
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Projekt „nie do obejścia": poziomy przeciwnika, każdy wektor obejścia → obrona, uczciwe granice |
| [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) | Odpowiedź na zewnętrzny audyt bezpieczeństwa (każdy finding → poprawka/status) |
| [`parent-app/`](parent-app/) | Wieloplatformowa apka rodzica (instalowalna PWA) — podpis Ed25519 na urządzeniu |
| [`packaging/`](packaging/) | Wdrożenie: instalator, unit systemd, hook pacman, egress nftables, allowlista fapolicyd, audyt hardeningu |
| [`docs/TESTING.md`](docs/TESTING.md) | Plan testów na czystym systemie (warstwa po warstwie + próby obejścia / czerwony zespół) |

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

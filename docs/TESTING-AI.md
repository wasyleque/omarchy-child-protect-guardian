# Test modeli AI i trybów autonomii — świeża instalacja Omarchy (BETA, v0.4.0)

Instrukcja krok po kroku na **czystym Omarchy (Arch)** + telefonie rodzica. Opis funkcji, ryzyk i ograniczeń:
[`AI-MODELS.md`](AI-MODELS.md). Ogólny plan testów guardiana: [`TESTING.md`](TESTING.md).

> Trzymaj otwarty **drugi terminal z rootem** przez cały test. Wszystko wraca do starego działania poleceniem
> `guardian-ctl autonomy ask`.

## 0. Co przygotować
- Komputer z Omarchy (8 GB RAM, bez GPU wystarczy), konto rodzica (z sudo) i **konto dziecka** (bez sudo).
- Telefon z Androidem, internet na obu urządzeniach (ntfy.sh).
- Modele (~750 MB, nie ma ich w git — za duże na drzewo gita): folder `ocpg-models` (`content_lite/`,
  `content_main_xlmr/`, `pkg/`, `grooming/`, `config.json`). Najłatwiej pobrać gotowy tarball:
  `wget https://omega-lubin.com/models/ocpg-models.tar.gz && tar -xzf ocpg-models.tar.gz` (powstanie `~/ocpg-models`).
  Albo skopiuj z maszyny, która je zbudowała: `scp -r wasyl@<laptop>:Work/ocpg-models ~/ocpg-models`.

## 1. Aplikacja rodzica na telefonie
1. Na telefonie otwórz **https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest**
   i pobierz `guardian-parent-<wersja>.apk` (aktualnie 0.1.16). Zezwól na instalację z tego źródła, zainstaluj, uruchom.
2. W ustawieniach aplikacji zezwól na powiadomienia (i wyłącz oszczędzanie baterii dla niej), inaczej wieczorny digest może nie dojść.
3. Parowanie zrobisz w kroku 3 (kod QR z komputera).

## 2. Zainstaluj guardiana na komputerze
```bash
sudo pacman -S --needed git base-devel rust python
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian && git log --oneline -1      # sprawdź, że masz wersję z autonomią
cd daemon && cargo build --release && cd ..
grep '^version' daemon/Cargo.toml                                # 0.4.0
sudo packaging/install.sh
sudo packaging/setup-child.sh <imie-dziecka>                     # konto dziecka + kod QR do parowania
```

## 3. Sparuj telefon
W aplikacji: **Parowanie → skanuj QR** z terminala (`setup-child.sh` go wyświetla; ponownie: `guardian-ctl pair --topic <topic>`).
Zatwierdź odcisk klucza w terminalu. **Sprawdź:** `sudo systemctl status guardiand` jest *active*, a `guardian-ctl list` działa.

Kontrola wstępna (stare działanie): `sudo -u <dziecko> guardian-ctl request pacman sl` → na telefonie karta
„Zezwól/Odmów”; zatwierdź z telefonu, polecenie dziecka kończy się decyzją. ✅ = baza działa.

## 4. Zainstaluj sidecar z modelami
```bash
sudo ai-sidecar/install.sh ~/ocpg-models
systemctl status guardian-ai          # active (running)
sudo ls -l /run/guardian-ai/          # ai.sock, prawa 0600
```
Dopisz do `/etc/guardian/policy.toml` (na końcu):
```toml
[autonomy]
mode = "ask"
child_age = 10
digest_hour = 19
```
```bash
sudo systemctl restart guardiand
guardian-ctl autonomy                  # → autonomy mode: ask
```

## 5. Testy
Dziecko zgłasza prośby tak: `sudo -u <dziecko> guardian-ctl request pacman <paczka>` (w drugim terminalu; blokuje do decyzji).
Logi decyzji: `journalctl -u guardiand -f`.

**A. Tryb `ask` (domyślny)** — `... request pacman gcompris-qt`: karta na telefonie, w logu `AI` nic nie robi. ✅ jak przed zmianą.

**B. Tryb `quiet`** — `sudo guardian-ctl autonomy quiet`:
1. `request pacman gcompris-qt` (zielone) → **bez pytania** wraca *allow*; w logu `AI Allow 'gcompris-qt' (light green …)`; telefon **nic nie dostaje**.
2. `request pacman steam` (czerwone) → karta na telefonie, w logu `AI defers 'steam' to the parent (needs the parent)`.
3. `request pacman telegram-desktop` → na liście `always_ask`, więc karta na telefonie, choć model mógłby ocenić inaczej.
4. `request pacman nieistniejaca-paczka-xyz` → karta na telefonie (`package not recognised`).

**C. Tryb `auto`** — `sudo guardian-ctl autonomy auto`:
1. `gcompris-qt` → *allow*; `nmap` albo `hashcat` → *deny* bez pytania; `supertuxkart` (żółte) → *deny*.
2. `sudo systemctl stop guardian-ai` i `request pacman gcompris-qt` → **karta na telefonie** (po ~20 s lub od razu przy braku gniazda): przy awarii modelu decyduje rodzic, nigdy automatycznie. Potem `sudo systemctl start guardian-ai`.
3. **Cofnięcie odmowy:** `sudo guardian-ctl autonomy ask-about nmap`, potem `request pacman nmap` → teraz karta na telefonie.

**D. Wieczorny digest (najważniejszy)**
1. `sudo guardian-ctl digest-now` → w ciągu chwili na telefonie alert „Guardian: podsumowanie dnia” z końcówką
   *„Decyzje modelu AI (tryb: auto, wersja beta). Zatwierdził: gcompris-qt (green, 7+). Odrzucił: … Do Ciebie trafiło: N z M próśb.”*
2. Czy wygląda dobrze także **bez Ollamy** (domyślnie jej nie ma) — tak ma być: wysyłane są same fakty.
3. Dzień bez zdarzeń: na koncie bez próśb `digest-now` daje „Dziś nie było żadnych próśb o instalację.” (digest nie znika w ciszy).
4. Odłącz telefon od internetu, wykonaj `digest-now`, po minucie włącz internet: alert **dochodzi** (ponawianie 30 s / 2 min / 10 min…; `journalctl -u guardiand | grep digest`).
5. Zaplanowany digest: ustaw `digest_hour` na najbliższą pełną godzinę, `sudo systemctl restart guardiand`, poczekaj — alert przyjdzie sam.

**E. Audyt** — `sudo grep auto_decided /var/log/guardian/audit.log | tail` pokazuje paczkę, kolor, wiek i tryb (bez żadnego tekstu);
`guardian-ctl audit-verify /var/log/guardian/audit.log` → łańcuch poprawny.

**F. Zasoby i dyskrecja** — przy trybie `auto` uruchom kilka próśb i obserwuj `systemctl status guardian-ai` (Memory ≈ do 1 GB, CPU znikome, `Nice 19`),
a po 5 min bez ruchu pamięć spada (modele się zwalniają). Komputer dziecka nie powinien odczuwalnie zwolnić.

**G. Bezpieczeństwo** — jako dziecko: `socat - UNIX-CONNECT:/run/guardian-ai/ai.sock` → brak dostępu (gniazdo 0600, katalog 0700).
`ss -xp | grep guardian-ai` nie pokazuje połączeń sieciowych, a `systemctl show guardian-ai -p PrivateNetwork` = `yes`.

## 6. Co zgłosić (ważne dla bety)
- Paczki, które model **zatwierdził, a nie powinien** (najgroźniejszy błąd), i takie, które **odrzucił niesłusznie**
  (wiemy, że `gimp`, `blender`, `wine` bywają czerwone).
- Czy digest przyszedł o czasie i czy treść była zrozumiała.
- Ile pamięci i CPU zajmuje sidecar oraz czy komputer dziecka coś zauważył.
Zapisuj nazwa paczki + wiek + wynik: to są ludzkie etykiety potrzebne do poprawy modeli.

## 7. Powrót do stanu wyjściowego
```bash
sudo guardian-ctl autonomy ask                      # wszystko znów do rodzica
sudo systemctl disable --now guardian-ai            # wyłącz modele
sudo rm -rf /opt/guardian-ai /usr/share/guardian-ai /etc/systemd/system/guardian-ai.service
```

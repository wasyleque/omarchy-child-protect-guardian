# Szybki test na świeżym Omarchy (PL) — v0.4.0

Zwięzła, „krok po kroku” instrukcja po polsku na czysty komputer testowy z Omarchy + telefon rodzica.
Pełny opis funkcji, ryzyk i ograniczeń AI: [`AI-MODELS.md`](AI-MODELS.md). Szczegółowy wariant: [`TESTING-AI.md`](TESTING-AI.md).

Duże pliki (modele ~750 MB i APK) są hostowane poza gitem, bo nie mieszczą się w drzewie repo:
**https://omega-lubin.com/models/**.

> Trzymaj otwarty drugi terminal z rootem. Wszystko wraca do starego działania: `guardian-ctl autonomy ask`.

## 1. Telefon — apka 0.1.16
W przeglądarce telefonu: **https://omega-lubin.com/models/guardian-parent-0.1.16.apk** → zainstaluj
(na GitHub Releases wisi starsza wersja — jej nie bierz). Zezwól na **powiadomienia** i **wyłącz
oszczędzanie baterii** dla aplikacji.

## 2. Demon
```bash
sudo pacman -S --needed git base-devel rust python
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
sudo packaging/install.sh            # tryb bezpieczny (hook testowy 'sl')
sudo packaging/install.sh --enforce  # pełny: realny hook + filtr DNS + fapolicyd
sudo packaging/setup-child.sh dziecko   # na pytanie o logowanie wybierz [c] = autologin dziecka
```
Przy `--enforce` z aktywnym `systemd-resolved` skrypt pokaże polecenie wyboru trybu DNS
(`guardian-dns-mode child-only` — filtr tylko dla dziecka).

## 3. Parowanie telefonu
Zeskanuj kod QR z terminala (sekcja „Add a child”). Sprawdzian:
```bash
sudo -u dziecko guardian-ctl request pacman sl    # karta na telefon; Zezwól -> ALLOW, Odmów -> DENY
```

## 4. Modele AI (sidecar child-guardiana)
```bash
cd ~ && wget https://omega-lubin.com/models/ocpg-models.tar.gz && tar -xzf ocpg-models.tar.gz
cd ~/omarchy-child-protect-guardian
sudo bash ai-sidecar/install.sh ~/ocpg-models
```
Dopisz do `/etc/guardian/policy.toml` (wzór: `packaging/policy.example.toml`):
```toml
[autonomy]
mode = "ask"
child_age = 10
```
```bash
sudo systemctl restart guardiand
journalctl -u guardian-ai -f     # podgląd werdyktów (nigdy treść)
```

## 5. Ollama — wieczorne podsumowanie (opcjonalnie)
Osobna warstwa (duży model językowy; NIE jest w `ocpg-models`).
```bash
ollama pull qooba/bielik-4.5b-v3.0-instruct:Q8_0     # polski; lżej: ollama pull llama3.2
```
Blok `[ai]` w `policy.toml` (albo ustaw z telefonu, sekcja „AI / Ollama”):
```toml
[ai]
enabled = true
provider = "ollama"
endpoint = "http://127.0.0.1:11434/api/generate"   # lub http://192.168.0.11:11434/api/generate
model = "qooba/bielik-4.5b-v3.0-instruct:Q8_0"
timeout_secs = 30
digest_enabled = true
digest_hour = 19
digest_lang = "pl"
```
```bash
sudo systemctl restart guardiand
sudo guardian-ctl digest-now     # podsumowanie na telefon OD RAZU
```

## 6. Testy
```bash
guardian-ctl autonomy quiet      # gcompris-qt bez pytania; steam/telegram-desktop -> telefon
guardian-ctl autonomy auto       # gcompris-qt auto-zatwierdzone; nmap/supertuxkart odrzucone
sudo systemctl stop guardian-ai  # brak modelu => każda prośba trafia na telefon (nie zatwierdza sama)
sudo systemctl start guardian-ai
guardian-ctl autonomy ask        # powrót do starego działania
```
Filtr treści (po `--enforce`), jako dziecko:
```bash
sudo -u dziecko getent ahostsv4 pornhub.com    # 0.0.0.0
```
→ na telefon przychodzi podpisany alert „zablokowana strona”. **Reboot:** sesja dziecka wstaje sama
(autologin), filtr działa, rodzic wchodzi przez SSH.

## 7. Powrót / odinstalowanie
```bash
guardian-ctl autonomy ask
sudo systemctl disable --now guardian-ai
sudo packaging/uninstall.sh
```

## 8. Co zapisywać
Model jest w BECIE i bywa ZA OSTRY (dla 9-latka „czerwone”: gimp, blender, kdenlive, wine). Zapisuj,
które paczki ocenił **niesłusznie** (zatwierdził lub odrzucił) — to etykiety do poprawy modeli.

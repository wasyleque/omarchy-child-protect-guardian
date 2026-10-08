# Plan testów — Omarchy Child Protect Guardian

Instrukcja do wykonania na **czystym systemie Omarchy/Arch na osobnym sprzęcie**. Testujesz warstwami:
najpierw bezpiecznie (hook tylko na `sl`), potem włączasz egzekucję, na końcu próbujesz to obejść.

Oznaczenia: 🟢 ma zadziałać · 🔴 ma zostać zablokowane/odmówione · ↩️ rollback.

> Zasada nadrzędna każdego testu: w razie wątpliwości **fail-closed** = instalacja ma być ODRZUCONA.
> Trzymaj otwarty **drugi terminal z rootem** przez cały czas (na wypadek fapolicyd/nftables).

---

## 0. Przygotowanie
```bash
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
id -u <dziecko>            # zapamiętaj UID konta dziecka (potrzebny do nftables)
```

## 1. Instalacja (tryb bezpieczny) i podstawy demona
```bash
sudo packaging/install.sh          # instaluje demona + wrappery + watchdog + hook TESTOWY (tylko 'sl')
systemctl status guardiand         # 🟢 active (running)
ls -l /run/guardian/               # 🟢 guardian.sock (0660) + submit.sock (0666)
sudo cat /var/log/guardian/audit.log   # 🟢 pierwsza linia: {"event":"started",...}
```
- [ ] demon aktywny, oba gniazda istnieją, audit log powstał.

## 2. Pacman: wstrzymanie → decyzja (hook testowy)
```bash
# terminal A (jako dziecko lub ty):
sudo pacman -S sl                  # 🟢 zawiśnie na "waiting for parent approval"
# terminal B:
guardian-ctl list                  # 🟢 widać żądanie: <id> Pacman sl
guardian-ctl allow <id>            # terminal A dokończy instalację
```
- [ ] **allow** → instalacja przechodzi.
- [ ] powtórz i **deny** <id> → 🔴 `failed to commit transaction`, `pacman -Q sl` = brak.
- [ ] **timeout**: zrób `pacman -S sl` i nie decyduj ~5 min (lub ustaw `decision_timeout_secs=15`
      w policy) → 🔴 DENY po czasie (fail-closed).
- [ ] **aktualizacja nie jest blokowana**: `sudo pacman -Syu` istniejących pakietów → 🟢 bez pytania.
↩️ nic nie trzeba — to tylko pakiet `sl`.

## 3. Kontrola dostępu do gniazda (SO_PEERCRED)
Jako **dziecko** (inny UID niż root):
```bash
printf '{"kind":"resolve","id":"00000000-0000-0000-0000-000000000000","decision":"allow"}\n' \
  | socat - UNIX-CONNECT:/run/guardian/guardian.sock   # lub: nc -U
```
- [ ] 🔴 połączenie odrzucone / brak odpowiedzi; w logu demona `refused control connection from uid …`;
      jeśli skonfigurowany ntfy — na telefon przychodzi **alert „blocked control attempt"**.
- [ ] Dziecko **nie może** samo zatwierdzić swojego żądania.

## 4. Limit zgłoszeń per-uid (anti-flood)
```bash
for i in 1 2 3 4 5 6; do (printf '{"kind":"submit","source":"pacman","package":"p%s"}\n' $i \
  | socat - UNIX-CONNECT:/run/guardian/submit.sock &); done
```
- [ ] po 5 oczekujących 6. dostaje 🔴 `too many pending requests`.

## 5. Zdalny push na telefon (ntfy)
### 5a. Parowanie apki rodzica
```bash
# wygeneruj losowy topic i wstaw do /etc/guardian/policy.toml → [ntfy] (enabled=true, request_topic=…)
guardian-ctl pair --topic <twój_topic>     # pokaże QR
# zeskanuj QR telefonem → apka (https://wasyleque.github.io/.../parent-app/) otworzy się skonfigurowana
# skopiuj z apki "public key" → /etc/guardian/policy.toml → [ntfy].parent_pubkey (tryb SIGNED)
sudo systemctl restart guardiand
```
### 5b. Testy decyzji zdalnej
- [ ] `pacman -S sl` → 🟢 push na telefon z nazwą pakietu; **Allow** w apce → instalacja rusza.
- [ ] powtórz → **Deny** → 🔴 instalacja anulowana.
- [ ] **telefon offline / brak decyzji** → 🔴 DENY po timeout (fail-closed).
- [ ] **próba fałszerstwa** (tryb signed): ktoś zna topic i wysyła „allow" bez klucza →
      🔴 odrzucone; na telefon **alert „FORGED approval blocked"**; audyt zapisuje odrzucenie.
      (szybki test: `curl -s -X POST https://ntfy.sh/<resp_topic> -d '{"id":"…","decision":"allow","nonce":"…","ts":…,"sig":"AAAA…"}'`)

## 6. Flatpak / Snap / Nix (kanały poza pacmanem)
```bash
flatpak install flathub org.videolan.VLC    # 🟢 zawiśnie → decyzja jak w pkt 2
flatpak list                                 # 🟢 działa od razu (nie-install nie jest bramkowany)
# jeśli masz snap/nix:
snap install code                            # 🔴/🟢 wg decyzji
nix-env -iA nixpkgs.hello                     # 🔴/🟢 wg decyzji
```
- [ ] instalacje bramkowane, reszta poleceń przechodzi.
- [ ] GUI sklep (GNOME Software/Discover) jako dziecko → 🔴 instalacja systemowa Flatpak wymaga hasła admina (reguła polkit).

## 7. Allowlista wykonywania (fapolicyd) — NAJWAŻNIEJSZE przeciw „pobierz i odpal"
Najpierw **permissive** (patrz `packaging/fapolicyd/README.md`), dostrój trust z pacmana, dopiero potem enforce.
Po `systemctl enable --now fapolicyd`, jako **dziecko**:
```bash
# pobierz dowolny portable ELF / AppImage do $HOME i spróbuj uruchomić:
chmod +x ~/jakis-appimage.AppImage && ~/jakis-appimage.AppImage     # 🔴 Permission denied (nie na trust-liście)
cp /bin/true ~/t && ~/t                                             # 🔴 kopia w $HOME nie wykona się
/lib64/ld-linux-x86-64.so.2 ~/t                                     # 🔴 trik z loaderem też nie
```
- [ ] 🟢 normalne, zainstalowane przez pacmana programy działają.
- [ ] 🟢 **po zatwierdzonej instalacji** nowy program działa (auto-trust: hook `60-guardian-trust`).
- [ ] 🔴 cokolwiek dziecko samo ściągnie do `$HOME`/`/tmp`/USB — nie uruchomi się.
↩️ `sudo systemctl stop fapolicyd`.

## 8. Sieć — egress (nftables)
Ustaw `CHILD_UID` + `DNS_SERVERS` w `packaging/nftables/guardian-egress.nft`, potem:
```bash
sudo nft -f packaging/nftables/guardian-egress.nft
```
Jako **dziecko**:
- [ ] 🟢 zwykłe przeglądanie (http/https) działa.
- [ ] 🔴 `dig @1.1.1.1 example.com` / inny publiczny DNS → zablokowany (wymuszony tylko Twój resolver).
- [ ] 🔴 DoH w przeglądarce (ręcznie 1.1.1.1) → nie rozwiązuje; DoT `:853` → drop.
- [ ] 🔴 portable WireGuard/Shadowsocks/Tor (obfs4) → brak połączenia (UDP/arbitralne porty deny).
- [ ] 🟢 **Twój** VPN na koncie rodzica/routerze — nietknięty (polityka dotyczy tylko UID dziecka).
↩️ `sudo nft delete table inet guardian`.

## 9. NetworkManager + DoH przeglądarek (lockdown)
Po wgraniu `packaging/network/*` (patrz jego README), jako **dziecko**:
- [ ] 🔴 dodanie nowego połączenia / VPN / hotspotu w NetworkManager → wymaga hasła admina.
- [ ] 🔴 „Secure DNS/DoH" w Firefox/Chromium → wyszarzone/zablokowane (polityka).

## 10. Watchdog + odporność na manipulację (jako root — symulacja ataku scriptletu)
```bash
# najpierw utrwórz kopię zapasową i włącz timer (robi to install.sh); potem:
sudo chattr -i /usr/lib/guardian/guardian-hook; sudo rm -f /usr/lib/guardian/guardian-hook
sudo systemctl start guardian-watchdog.service    # albo poczekaj na timer (~3 min)
ls -l /usr/lib/guardian/guardian-hook             # 🟢 przywrócony z backupu
```
- [ ] 🟢 skasowany hook/binarka demona → **odtworzone**; na telefon **alert „Integrity: restored …"**.
- [ ] `sudo systemctl stop guardiand` → watchdog 🟢 restartuje + alert.
- [ ] ręczna edycja `/var/log/guardian/audit.log` → watchdog 🟢 alert „audit failed chain verification".

## 11. Audyt (łańcuch skrótów)
```bash
guardian-ctl audit-verify /var/log/guardian/audit.log    # 🟢 "audit OK: N entries"
sudo sed -i '0,/"decision":"allow"/s//"decision":"deny"/' /var/log/guardian/audit.log   # zepsuj 1 wpis
guardian-ctl audit-verify /var/log/guardian/audit.log    # 🔴 "audit FAILED: content tampered at entry …"
```
(po teście przywróć log z backupu lub usuń — to niszczy łańcuch celowo).

## 12. Baza sprzętowa (fundament — bez tego reszta jest do obejścia)
```bash
packaging/harden-audit.sh <dziecko>     # ma pokazać PASS dla UEFI pw/SecureBoot/LUKS/bootloader
```
Fizyczne próby (jako „atakujący"):
- [ ] 🔴 w GRUB wciśnij `e`, dopisz `init=/bin/sh` → ma żądać hasła / nie dać edytować (UKI/locked GRUB).
- [ ] 🔴 boot z live-USB → zablokowany (UEFI pw + Secure Boot).
- [ ] 🔴 (opcjonalnie) wyjęcie dysku do innego PC → dane zaszyfrowane (LUKS).
> Uczciwie: reset CMOS na desktopie i inne urządzenie są POZA zasięgiem software (patrz THREAT_MODEL §9).

## 13. Czerwony zespół — wejdź w buty dziecka i próbuj obejść
Zalogowany jako **dziecko**, bez hasła roota, przejdź listę — każda próba ma 🔴:
- [ ] `sudo pacman -S cokolwiek` → brak sudo.
- [ ] pobrany AppImage/gra z `$HOME`/USB → nie uruchamia się (fapolicyd).
- [ ] `flatpak --user install …` → bramkowane (wrapper) / apka i tak nie wykona się (fapolicyd).
- [ ] `pip install --user` / `npm i -g` w $HOME → binarka nie wykona się (fapolicyd).
- [ ] darmowy VPN w przeglądarce / DoH / Tor → sieć nie wypuszcza (nftables).
- [ ] drugie konto / `Ctrl+Alt+F2` → brak logowania (konto zablokowane, getty zmaskowane).
- [ ] ubicie/wyłączenie `guardiand` → brak uprawnień; a jak padnie, nowe instalacje i tak = DENY.
- [ ] podsłuch topicu ntfy i wysłanie „allow" → 🔴 bez podpisu Ed25519 (alert do rodzica).

## 14. Włączenie pełnej egzekucji i rollback
```bash
sudo packaging/install.sh --enforce     # prawdziwy systemowy hook pacman + auto-trust
# ...pełne testy...
sudo packaging/uninstall.sh             # pełny rollback (zdejmuje też chattr +i)
```

---

### Co zanotować z testu
Dla każdego 🔴, który **NIE** zadziałał (dało się obejść) — zapisz: krok, co się stało, `journalctl -u guardiand`
i `cat /var/log/guardian/audit.log`. To są dziury do załatania. Dla 🟢 które nie działa (za restrykcyjne) —
też zanotuj (np. fapolicyd blokuje legalny program → dodać do trust).

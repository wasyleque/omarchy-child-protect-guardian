# Architektura — Omarchy Child Protect Guardian

Szkic techniczny fazy 0. Decyzje nie są zabetonowane — to punkt wyjścia do dyskusji.

```
┌──────────────────────── Komputer dziecka (Omarchy) ─────────────────────────┐
│                                                                              │
│  pacman / yay / flatpak ──┐                                                  │
│  Omarchy TUI installer ───┤                                                  │
│                           ▼                                                  │
│                 [ Punkty przechwycenia ]                                     │
│                 • Polkit rule  • PAM hook  • flatpak wrapper (D-Bus)         │
│                           │                                                  │
│                           ▼                                                  │
│                 ┌───────────────────────┐    egzekucja:                      │
│                 │  guardiand (root)      │──► nftables/eBPF (sieć)           │
│                 │  - polityki (read-only)│──► cgroups v2 + SIGSTOP (czas)    │
│                 │  - kolejka żądań       │──► Hyprland IPC (okna/blur)       │
│                 │  - weryfikacja podpisu │──► Bubblewrap (sandbox)           │
│                 └───────────┬───────────┘                                    │
│                             │ (wychodzące: POST żądanie; nasłuch: SSE/WS)    │
└─────────────────────────────┼────────────────────────────────────────────────┘
                              ▼
                   ┌─────────────────────┐   darmowy broker pub/sub
                   │   ntfy.sh / self-   │   (push + przyciski akcji)
                   │   hosted ntfy       │
                   └──────────┬──────────┘
                              ▼
                   ┌─────────────────────┐   Zezwól / Odrzuć (podpis Ed25519)
                   │  Telefon rodzica    │   MVP: apka ntfy; v2: PWA/native
                   └─────────────────────┘
```

## 1. Przechwytywanie instalacji

Trzy ścieżki instalacji → trzy mechanizmy haka:

| Ścieżka | Mechanizm przechwycenia |
|---|---|
| `pacman`, `yay`/`paru`, AUR (`pacman -U`), instalatory GUI | **Hook ALPM `PreTransaction`** (`/etc/pacman.d/hooks/`) uruchamiający mały `guardian-hook`, który łączy się z gniazdem i blokuje do decyzji. Zakres `Operation = Install`, więc aktualizacje systemu (`-Syu`) przechodzą bez blokady; z opcją **`AbortOnFail`** na hooku kod wyjścia ≠ 0 anuluje całą transakcję, zanim cokolwiek zostanie zapisane (fail-closed — opcja wymagana, brakowało jej do audytu 2026-10-09). Wszystko, co instaluje pakiet, przechodzi przez `libalpm` — więc **nie da się tego obejść z konta użytkownika**, a w przeciwieństwie do reguły Polkit łapie też zwykłe `sudo pacman` w terminalu (którego Polkit nie pośredniczy). |
| Flatpak CLI (`flatpak install`, też `--user`) | **Wrapper** w `/usr/local/bin/flatpak` zgłasza na gniazdo submit i uruchamia prawdziwy flatpak dopiero po zgodzie. Instalacje systemowe z GUI (polkit) blokuje reguła polkit; instalacje `--user` z GUI łapie allowlista fapolicyd. Patrz `packaging/flatpak/`. |

Budowanie pakietu AUR (jako user) jest nieszkodliwe — blokujemy dopiero *instalację* do systemu.
Dla pełnej szczelności rozważamy też tryb, w którym dziecko w ogóle nie ma ścieżki `sudo`,
a instalacje idą wyłącznie przez autoryzowany kanał Guardiana.

## 2. `guardiand` — demon kontrolny

- Proces roota pod `systemd` (`Restart=always`), chroniony przez cgroups (dziecko nie ubije).
- Przechowuje polityki (YAML/TOML, read-only dla dziecka), kolejkę oczekujących żądań, klucze.
- Egzekwuje:
  - **Sieć:** `nftables` + eBPF — blocklisty domen/IP, wymuszony DNS (SafeSearch/tryb ograniczony),
    blokada obejść (znane endpointy VPN/proxy, DoH do nieautoryzowanych serwerów).
  - **Czas:** cgroups v2 — zliczanie czasu aktywnych apek, `SIGSTOP`/`SIGCONT` zamiast kill.
  - **Okna:** Hyprland IPC — blur/komunikat na zamrożonym oknie, reguły zrzutu ekranu.
  - **Sandbox:** Bubblewrap — tymczasowy, odcięty od sieci domowej i `$HOME`.

## 3. Push-approval bez drogiego backendu

Zasada: **nie stawiamy własnego serwera stanu**. Potrzebny tylko „listonosz" wiadomości.

- **Broker: [ntfy](https://ntfy.sh)** — open-source pub/sub z gotowymi apkami Android/iOS,
  wspiera **przyciski akcji** w powiadomieniu i publikację przez zwykły HTTP POST.
  Można użyć publicznego `ntfy.sh` (prywatny, losowy topic) lub self-hosted (1 mały VPS/RPi).
- **Przepływ:**
  1. `guardiand` tworzy UUID żądania, POST na prywatny topic z przyciskami *Zezwól/Odrzuć*.
  2. Rodzic klika w powiadomieniu → akcja uderza w lekki endpoint
     (**Cloudflare Worker**, darmowy tier ~100k req/dzień) lub w topic zwrotny ntfy.
  3. `guardiand` nasłuchuje (SSE/WebSocket ntfy) i odbiera decyzję dla danego UUID.
- **Koszt:** 0 zł na start (publiczny ntfy + ewentualnie darmowy Worker).

## 4. Bezpieczeństwo samego mechanizmu zgód (zero-trust)

Push przez publiczny broker nie może oznaczać, że „ktokolwiek z linkiem" zatwierdzi instalację.

- **Parowanie** telefon↔PC przez QR: wymiana kluczy publicznych (Ed25519).
- **Podpis decyzji:** telefon podpisuje `{uuid, decyzja, timestamp}` kluczem prywatnym;
  `guardiand` weryfikuje podpis kluczem publicznym rodzica. Przejęcie brokera/topicu **nie**
  pozwala podrobić „Zezwól".
- **Anti-replay:** jednorazowe UUID + krótkie TTL + timestamp.
- **Uwaga o MVP:** przyciski akcji w ntfy to zwykły HTTP (bez podpisu klienta). Dlatego:
  - **MVP:** zaufanie oparte na sekretnym topicu + jednorazowym tokenie (wystarczające na start,
    udokumentowany kompromis).
  - **v2:** cienka **PWA/aplikacja** która trzyma klucz i realnie **podpisuje** decyzje → pełen zero-trust.

## 5. Komponenty i stos (propozycja)

- `guardiand` — **Rust** (bezpieczeństwo pamięci, małe zużycie, dobre biblioteki nft/eBPF/D-Bus).
- Integracje: Polkit (reguły JS), PAM (C/Rust), D-Bus, Hyprland IPC (socket).
- Konfiguracja/polityki: TOML.
- Telefon: MVP = ntfy; v2 = PWA (Web Crypto do Ed25519) lub natywna apka.
- Opcjonalny AI-TL;DR: lokalny model (Ollama) parsujący metadane pakietu — **offline, prywatnie**.

## 6. Ryzyka / do przemyślenia

- Poprawne i pełne pokrycie wszystkich ścieżek instalacji (żeby nie było „tylnej furtki").
- eBPF/nftables vs. uprawnienia i stabilność po aktualizacjach jądra Arch.
- UX zawieszonego terminala (jasny komunikat, timeout, tryb offline gdy brak neta).
- Pewność dostarczenia pusha (retry, fallback kanał).

## 7. Zdalny root ograniczony do akcji — Intent Binding

Zgoda = jednorazowe, związane z akcją podniesienie uprawnień (nigdy shell). Przepływ:

1. `guardiand` rozwiązuje żądanie do konkretnej akcji, **pobiera z góry** pakiet + metadane do
   zamrożonego cache roota (`/var/cache/guardian/<uuid>/`) i liczy SHA-256.
2. Buduje kanoniczny obiekt **intent** `{action, package_id, artifact_sha256, uuid, nonce, ts,
   description_sha256}` i wysyła czytelny opis na telefon.
3. Telefon podpisuje intent kluczem Ed25519 rodzica; `guardiand` weryfikuje podpis, nonce (niezużyty),
   TTL (~5 min) i że `artifact_sha256` wciąż zgadza się z zamrożonym cache.
4. Wykonanie: dedykowany helper przez `execve` w izolowanym namespace z jedną potrzebną capability,
   instalacja **wyłącznie** z zamrożonego cache (bez ponownego pobierania → bez TOCTOU).
5. Podpisany intent + wynik dopisywane do chronionego append-only logu (SQLite/journald).

## 8. Feed LACS (źródło danych wiekowych)

- Dostarczany jako **podpisany feed metadanych** (styl TUF / Sigstore) z publicznego rejestru Git.
- `guardiand` trzyma **lokalny podpisany cache**, więc działa **offline**; podpis weryfikowany przed użyciem.
- Klucz = tożsamość pakietu (app-id Flathub / pkgname AUR + zakres wersji). Wpis = 5 wymiarów LACS.
  Zasila kartę decyzji rodzica i asystenta AI.

## 9. Statystyki + pipeline digestu AI

- **Kolektor (lokalny):** `guardiand` agreguje czas apek na pierwszym planie (cgroups/Hyprland) i
  liczniki domen/kategorii sieci (warstwa nftables/eBPF) do **lokalnego** magazynu (SQLite).
  Zapis jako **kategorie + czas**, nie surowe klawisze czy pełna historia URL.
- **Retencja i przejrzystość:** okno kroczące; dziecko widzi własne statystyki.
- **Digest AI (na żądanie, local-first):** lokalny model zamienia agregaty w tygodniowe streszczenie
  prostym językiem + sugestie rozmowy. Surowe dane nie opuszczają maszyny.

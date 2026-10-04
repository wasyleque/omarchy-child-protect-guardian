# Architektura — Omarchy Guardian

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
| `pacman`, `flatpak` (systemowy) | **Reguła Polkit** w `/etc/polkit-1/rules.d/` — żądanie autoryzacji akcji instalacji trafia do `guardiand`, który zamiast pytać o hasło roota wstrzymuje i czeka na zdalny token. |
| `sudo`/`doas pacman -U` (AUR via `yay`) | **Moduł PAM** (`pam_exec` lub własny) — terminal zawiesza się na etapie PAM z komunikatem *„Oczekiwanie na zgodę rodzica…"*, dokładnie jak serwerowe 2FA (wzorzec Duo Unix). |
| `flatpak --user` | **Wrapper** w `/usr/local/bin/flatpak` (przed systemowym w `PATH`) gadający z `guardiand` po D-Bus; blokuje do decyzji. |

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

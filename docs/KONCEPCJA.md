# Koncepcja — Omarchy Child Protect Guardian

Dokument roboczy fazy 0. Zbieramy i porządkujemy pomysły zanim napiszemy pierwszą linijkę kodu.

## 1. Problem i grupa docelowa

Rodzic chce, żeby dziecko mogło używać komputera (Omarchy/Linux) **samodzielnie**, ale:
- nie instalowało dowolnych gier/aplikacji bez zgody,
- nie siedziało bez końca i o dowolnych porach,
- nie trafiało na treści nieodpowiednie,
- a przy tym **nie dało się tego łatwo obejść** (dzieci są pomysłowe).

Konkurencja (Family Safety, Screen Time, Family Link, Qustodio) jest albo łatwa do obejścia,
albo ciężka i szpiegująca, albo wymaga konta w korporacyjnej chmurze. My robimy coś lżejszego,
prywatnego i realnie szczelnego — bo Linux na to pozwala.

## 2. Główna funkcja: zdalne zatwierdzanie instalacji (push-approval)

Przepływ „jak 2FA", ale dla instalacji aplikacji:

1. Dziecko uruchamia instalację (`yay -S jakas-gra`, `flatpak install ...`, Omarchy TUI itp.).
2. Guardian **przechwytuje** żądanie i wstrzymuje je — w terminalu/GUI pojawia się
   *„Oczekiwanie na zgodę rodzica…"* (z możliwością dopisania powodu prośby).
3. Na telefon rodzica leci **push** z czytelnym opisem: co to za apka, kategoria, czy open-source,
   kto prosi, na jakim urządzeniu.
4. Rodzic klika **Zezwól** / **Odrzuć** (opcjonalnie: *Zezwól na 1h w piaskownicy*).
5. PC odbiera **podpisaną** decyzję i kontynuuje lub anuluje instalację.

Kluczowe: rodzic **nie musi** być w domu ani przy komputerze. Decyzja zajmuje 3 sekundy z poziomu
powiadomienia na telefonie.

### Zasada apki rodzica
Aplikacja na telefon musi być **prosta i przejrzysta** (rodzic decyduje w ~3 sekundy i dokładnie
rozumie, co zatwierdza) **i jednocześnie w 100% skuteczna** (klik zawsze dociera do PC i jest
niezawodnie wyegzekwowany — żadnych cichych awarii, żadnego obejścia). Prostota nigdy nie kosztuje
skuteczności ani odwrotnie.
Klient rodzica działa **na każdym wiodącym OS** — Android i iOS oraz desktopy Windows, macOS i Linux —
żeby rodzic mógł zatwierdzać z dowolnego urządzenia pod ręką. Jeden wspólny rdzeń (PWA z podpisem
WebCrypto Ed25519) działa wszędzie, opakowany natywnie per platforma dla obecności w sklepach i pushy.

## 3. Przewagi możliwe dzięki Linux/Arch

### Zero-Bypass
- Dziecko działa jako zwykły user bez `sudo`/`doas`; demon i polityki należą do roota.
- Demon chroniony: `systemd` (Restart=always), cgroups v2 (dziecko nie ma prawa go ubić),
  Polkit blokuje podniesienie uprawnień.
- Sieć filtrowana w jądrze (`nftables` + eBPF) — VPN/proxy/Tor nie wyprowadzą ruchu poza politykę.
- Polityki read-only dla dziecka (pliki roota, ewentualnie wsparcie dla immutable/`chattr +i`).

### Kontrola przez kompozytor (Hyprland)
- Twarde egzekwowanie czasu: po przekroczeniu budżetu proces apki dostaje `SIGSTOP`
  (zamrożony, nie zamknięty — nie traci postępu), okno przyciemnione z komunikatem.
- Reguły okien: blokada zrzutów/udostępniania ekranu dla wrażliwych kontekstów.

### Prywatność i lekkość
- Zero telemetrii. Raporty zostają lokalnie; rodzic widzi je u siebie.
- Chmura tylko jako „listonosz" decyzji — nie przepływa przez nią treść aktywności dziecka.

### Filtr systemowy, nie per-aplikacja
- Jedna polityka DNS/sieci obejmuje wszystko: przeglądarki, gry, komunikatory, launchery.

## 4. Nieoczywiste pomysły (wyróżniki)

1. **Playtest Sandbox (tymczasowy dostęp bez czekania).** Zamiast frustrującego „czekaj aż
   rodzic odpisze", apka od razu startuje w **15-minutowej izolowanej piaskownicy** (Bubblewrap):
   bez dostępu do sieci domowej i prywatnych plików. Po 15 min Hyprland zamraża okno (`SIGSTOP`) —
   dalej dopiero po akceptacji rodzica. Dziecko nie siedzi sfrustrowane, a rodzic nie czuje presji.
2. **AI-TL;DR zamiast technicznego bełkotu.** Rodzic nie widzi `prismlauncher-bin` czy
   `lib32-mangohud`. Demon parsuje metadane z AUR/Flathub (opis, kategoria, licencja, popularność)
   i opcjonalnie streszcza lokalnym modelem: *„PrismLauncher — alternatywny launcher do Minecrafta.
   Gry. Open-source, bezpieczne."* Decyzja staje się świadoma, a nie „co to w ogóle jest?".
3. **Prośba z kontekstem.** Dziecko może dopisać powód („to na projekt do szkoły") — rodzic widzi
   to w powiadomieniu. Mniej telefonów „tato wejdź zatwierdź".
4. **Dwoje rodziców / wielu opiekunów.** Każdy opiekun ma swój klucz; zatwierdzić może dowolny
   (albo tryb „wymagana zgoda obojga" dla wrażliwych kategorii).
5. **Panic/appeal log.** Odrzucenia i prośby lądują w tygodniowym podsumowaniu — rozmowa z
   dzieckiem na podstawie faktów, nie domysłów.

## 5. Scenariusze użycia (szkic)

- *„Dziecko chce nową grę o 20:00"* → push do taty → *Zezwól na weekend* → instalacja rusza.
- *„Przekroczony limit 2h"* → gra zamrożona, komunikat na ekranie, prośba o +30 min do mamy.
- *„Próba wejścia na zablokowaną stronę"* → blokada systemowa + wpis do raportu (bez pełnej historii URL).

## 6. Otwarte pytania (do decyzji)

- Native app na telefon czy start od gotowego **ntfy** + (później) PWA? (patrz ARCHITEKTURA)
- Zakres filtra treści w MVP: tylko DNS blocklisty czy od razu eBPF per-app?
- Model licencyjny i czy publiczne od początku.
- ~~Nazwa/brand~~ → ustalona: **Omarchy Child Protect Guardian**.

## 7. Zgoda = „zdalny root" ograniczony do jednej akcji (Intent Binding)

„Zezwól" rodzica to **nie** root shell dla dziecka — to **jednorazowe, wąsko ograniczone podniesienie
uprawnień**, które upoważnia `guardiand` (już root) do wykonania **dokładnie jednej** wcześniej
opisanej akcji (np. „zainstaluj flatpak X"). Telefon decyduje, *czy dać zdalnego roota na tę jedną akcję*.

- **Intent Binding:** telefon podpisuje (Ed25519) kanoniczny JSON: `action`, dokładny identyfikator
  pakietu, timestamp, jednorazowy nonce oraz **SHA-256 opisu, który rodzic faktycznie zobaczył**.
  PC wykona wyłącznie to, co pasuje do podpisanego zamiaru — nic więcej.
- **Bez TOCTOU:** przed zapytaniem `guardiand` pobiera pakiet + metadane do zamrożonego cache roota
  (`/var/cache/guardian/`) i liczy hash; rodzic autoryzuje *ten konkretny hash*; instalacja idzie
  tylko z zamrożonego cache, bez ponownego pobierania z sieci.
- **Najmniejsze uprawnienia:** żadnego `bash -c`; bezpośrednie `execve` dedykowanej binarki w
  izolowanym namespace, z porzuceniem wszystkich capabilities poza tą jedną potrzebną.
- **Krótki TTL + ochrona przed replay:** ważność ~5 min; zużyte nonce są zapamiętywane.
- **Niezaprzeczalny audyt:** każde nadanie + podpis rodzica trafia do chronionego, tylko-dopisywalnego
  dziennika.

Komunikat zrozumiały dla każdego — nie „nadaj CAP_SYS_ADMIN", tylko:
*„Zuzia chce zainstalować »PrismLauncher« (launcher do Minecrafta). Zezwolić na tę jedną instalację?"*

## 8. Asystent AI oceny ryzyka w apce rodzica (na żądanie)

Przycisk **„Pomóż mi zdecydować"** daje ocenę ryzyka tej konkretnej apki/akcji prostym językiem.

- **Prywatność (zero wiedzy o dziecku):** analizowane są wyłącznie **publiczne metadane aplikacji** —
  manifest Flatpak, `PKGBUILD`, żądane uprawnienia, domeny sieciowe, wpis z bazy LACS. Żadne dane
  dziecka (IP, host, login, historia) nie opuszczają urządzenia.
- **Model hybrydowy:** domyślnie **lekki model on-device** (np. Gemma 2B / Phi-3) do natychmiastowej,
  offline kategoryzacji znanych uprawnień; **opcjonalnie na żądanie** zapytanie do zewnętrznego API
  (klucz rodzica, polityka zero-retention) dla „wyjaśnij dokładnie nieznany pakiet z AUR".
- **Czytelny wynik:** **światła** (🟢 bezpieczna / 🟡 sieć lub płatności / 🔴 wysokie uprawnienia lub
  treści dla dorosłych), **TL;DR w 3 punktach** i **tłumacz uprawnień**
  (`filesystem=home` → *„aplikacja widzi Twoje prywatne zdjęcia i dokumenty"*).

## 9. Społecznościowy, open-source ranking wiekowy — LACS

**Nowa otwarta skala wychodząca poza PEGI/ESRB** (które w ogóle nie pokrywają apek z Linux/AUR/Flathub).
Robocza nazwa **LACS — Linux App Content Standard** (albo *OpenAge Matrix*). Wielowymiarowa, nie jedna liczba:

1. **Treści drażliwe** — przemoc, wulgaryzmy, nagość/horror (0–3).
2. **Komunikacja i sieć** — czat tekstowy/głosowy, P2P, otwarty multiplayer, telemetria (brak/moderowany/otwarty).
3. **Monetyzacja i dark patterns** — reklamy, mikrotransakcje, lootboxy, FOMO.
4. **Uprawnienia systemowe** — kamera/mikrofon, pliki domowe, sieć, root.
5. **Wiek poznawczy** — 3+ / 7+ / 12+ / 16+ / 18+.

- **Kuracja (Web of Trust):** zmiany przez PR-y w otwartym rejestrze; scalenie wymaga podpisów
  kryptograficznych **zweryfikowanych kuratorów** (maintainerzy dystrybucji, zaufane organizacje
  edukacyjne); zwykli użytkownicy mogą jedynie *oflagować* niespójność (próg reputacji/historii, by
  uciąć trolling i manipulację).
- **Dystrybucja:** publiczne repo Git + **podpisany feed metadanych** (styl TUF / Sigstore),
  cache'owany lokalnie i odpytywany **offline**.

Ten dataset zasila zarówno kartę decyzji rodzica, jak i asystenta AI — a będąc otwartym, może stać się
standardem społeczności, którego zamknięta, przywiązana do sklepów konkurencja nie przebije.

## 10. Statystyki dla rodzica + łagodny digest AI do rozmowy

Czytelny pulpit dla rodzica: **ile godzin** dziecko spędziło przy komputerze, **w jakich aplikacjach**
i **na jakich stronach/kategoriach** — dziennie/tygodniowo, z trendami.

- **Wyważona prywatność, nie inwigilacja.** Pokazujemy **kategorie i czas**, a nie log każdego
  klawisza; agregaty zamiast surowej historii URL. Dziecko widzi **własne** statystyki — przejrzystość
  buduje zaufanie, nie strach.
- **Opcjonalny digest AI (na żądanie).** AI streszcza tydzień i **sam uprzedza o tematach wartych
  uwagi rodzica** — tych, którymi warto się *delikatnie zmartwić* (np. skok nocnego używania, wyszukania
  wokół trudnego tematu) i tych, z których warto się *ucieszyć* (nowe twórcze hobby, nauka) — a potem
  **sugeruje, jak zacząć rozmowę** z dzieckiem, tak by je chronić **bez poczucia nadmiernej obserwacji**.
  Ton: coaching dla *rodzica*, nigdy policjant wobec dziecka.
- **Lokalnie i prywatnie.** Digest powstaje z lokalnych statystyk; surowe dane dziecka nie trafiają do chmury.

To przeciwieństwo „szpiegowskich raportów" konkurencji: celem jest lepsza *rozmowa*, nie dłuższy *dossier*.

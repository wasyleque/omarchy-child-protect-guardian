# Koncepcja — Omarchy Guardian

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
- Nazwa/brand (robocza: *Omarchy Guardian*).

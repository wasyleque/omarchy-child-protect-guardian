# Omarchy Child Protect Guardian 🛡️

**🌐 Langues :** [English](README.md) · [Polski](README.pl.md) · [Español](README.es.md) · [Deutsch](README.de.md) · **Français** · [中文](README.zh.md)

> Contrôle parental et sécurité des enfants en ligne pour **Omarchy Linux** (Arch + Hyprland),
> avec **approbation à distance des installations d'applis** depuis le téléphone du parent —
> un flux d'approbation push, exactement comme la confirmation d'une connexion 2FA.

> **Confidentialité :** activez le **chiffrement du payload** (`[ntfy].encrypt_payload`) et le broker ne
> voit jamais que du texte chiffré — le nom du paquet, la source et la commande sont scellés avec la clé
> du parent (vérifié de bout en bout sur un vrai téléphone). Les décisions sont également signées
> cryptographiquement dans tous les cas. Pour une confidentialité maximale, vous pouvez toujours utiliser
> un **ntfy auto-hébergé avec auth/ACLs**.

Objectif ambitieux : être **plus pratique et plus efficace** que Microsoft Family Safety,
Apple Screen Time, Google Family Link et Qustodio — tout en restant **privé** (aucune télémétrie,
aucun envoi des données de l'enfant vers le cloud d'une entreprise).

---

## 🚀 Installation & mise à jour

> **Guide complet de déploiement par ordre de sécurité :** **[`packaging/README.md`](packaging/README.md)**
> (base matérielle → démon → hook pacman → sortie nftables → fapolicyd → durcissement → mise à jour). Version courte :

**Sur l'ordinateur de l'enfant (Arch / Omarchy) :**
```bash
git clone https://github.com/wasyleque/omarchy-child-protect-guardian
cd omarchy-child-protect-guardian/daemon && cargo build --release && cd ..
sudo packaging/install.sh                   # sûr par défaut (seul 'pacman -S sl' est filtré) ; ajoutez --enforce quand vous êtes prêt
sudo packaging/setup-child.sh <nom-enfant>  # compte enfant verrouillé (sans sudo) + QR d'association
```
`install.sh --enforce` installe et active également le **filtre de contenu DNS** (liste de blocage
porno/catégories + SafeSearch) ainsi que les règles de sortie **nftables**, et les rend persistants au
redémarrage. Une seule commande guidée et réversible active ensuite le filtrage comme vous le souhaitez :
```bash
sudo /usr/lib/guardian/guardian-dns-mode.sh child-only      # seul l'enfant est filtré (recommandé)
sudo /usr/lib/guardian/guardian-dns-mode.sh whole-machine   # tout l'ordinateur (parent inclus)
```
Appliquez ensuite les couches restantes propres à la machine — la liste blanche d'exécution **fapolicyd**
(`guardian-fapolicyd-setup.sh`, *indispensable* pour une application réelle), le verrouillage
navigateurs/NM et la base matérielle — le tout dans [`packaging/README.md`](packaging/README.md).
Vérifiez l'ensemble de bout en bout à tout moment avec `sudo packaging/smoke-test.sh <enfant>` (qui
confirme même qu'un domaine porno est bloqué pour l'enfant).

**Sur le téléphone du parent :** installez l'application Android depuis la
**[page Releases](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** et
scannez le QR d'association — détails dans [📱 Application parent Android](#-application-parent-android).
Pas d'Android ? Voir [🍏 Application parent sur iPhone/iPad, Windows, Mac ou Linux](#-application-parent-sur-iphoneipad-windows-mac-ou-linux-sans-android).

**Mise à jour ultérieure :** `git pull` → `cargo build --release` → `sudo packaging/install.sh` →
`sudo systemctl restart guardiand` (sous fapolicyd en mode enforcing, faites d'abord à nouveau confiance
aux nouveaux binaires) — étapes complètes dans [`packaging/README.md` → *7. Updating*](packaging/README.md#7-updating-to-a-new-version).
L'APK sur le téléphone s'installe **sur place** par-dessus l'ancienne version (l'association est conservée).

---

## Pourquoi Linux/Omarchy l'emporte ici

Ce que la concurrence sur Windows/macOS/Android **ne peut pas** faire, mais nous si :

1. **Zero-Bypass (réellement inviolable).** Sur Windows/macOS, les enfants tuent le processus
   dans le gestionnaire de tâches ou réinitialisent les permissions. Ici le démon est protégé par
   Polkit, l'enfant n'a pas `sudo`, et cgroups v2 le rendent impossible à tuer. Le filtrage réseau
   se fait dans le noyau (`nftables`/eBPF), si bien que les VPN gratuits, proxys et Tor ne contournent pas la politique.
2. **Contrôle au niveau du compositeur (Hyprland/Wayland).** Nous pouvons *geler* un processus
   (`SIGSTOP`), flouter une fenêtre non autorisée, bloquer la capture/le partage d'écran — sans
   extensions invasives dans le navigateur.
3. **Zéro surcharge, 100% de confidentialité.** Pas de bloatware ni de télémétrie ; les politiques
   s'appliquent localement et hors ligne. Le cloud **relaie uniquement** la décision du parent.
4. **Filtrage à l'échelle du système, pas par navigateur.** Une seule politique DNS/réseau couvre
   chaque appli (jeux, lanceurs, messageries), pas seulement Chrome.

## Piliers du produit

- **Approbation d'installation à distance** — une tentative `pacman`/`yay`/`flatpak` est suspendue ;
  le parent reçoit un push avec une description lisible de l'appli et tape *Autoriser / Refuser*.
- **Budget de temps & horaires** — appliqués fermement via cgroups freeze + idle Hyprland.
- **Filtre de contenu à l'échelle du système** — un résolveur filtrant local bloque les domaines
  porno/catégories (liste StevenBlack, actualisée chaque jour) et force SafeSearch sur
  Google/YouTube/Bing pour *chaque* application, pas seulement le navigateur. Installé et persistant au
  démarrage ; une commande guidée permet de choisir entre un filtrage **enfant uniquement** ou
  **ordinateur entier**, de façon totalement réversible. Les contournements DoH/DoT/VPN sont bloqués dans
  le noyau (`nftables`).
- **Rapports lisibles pour le parent** — ce que l'enfant a fait et demandé (sans espionnage corporatif).
- **Approbations signées cryptographiquement — et chiffrées en option** — même un broker push compromis
  ne peut falsifier un « Autoriser », et avec le chiffrement du payload activé, il ne voit même pas le
  nom du paquet ni la commande.
- **Root distant limité à l'action (Intent Binding)** — une approbation accorde root pour *exactement
  une* action décrite, jamais un shell ; liée cryptographiquement à ce que le parent a réellement vu.
- **Assistant IA d'évaluation des risques** — vérification à la demande en langage clair (feu tricolore +
  traducteur de permissions) ; métadonnées publiques uniquement, aucune donnée de l'enfant ne quitte
  l'appareil.
- **Autonomie optionnelle de l'IA locale (BÊTA, désactivée par défaut)** — de petits modèles sur l'appareil
  (sans réseau, CPU uniquement) peuvent répondre *à ta place* aux demandes d'installation : tu choisis `ask`
  (par défaut — tout continue de t'être soumis), `quiet` (n'approuve automatiquement que les apps clairement
  sûres) ou `auto` (le modèle refuse aussi les risquées). Tout doute te revient, tu reçois chaque soir un
  résumé garanti de toutes les décisions, et une seule commande le désactive. Voir [`docs/AI-MODELS.md`](docs/AI-MODELS.md).
- **Classification d'âge ouverte LACS** — échelle communautaire multidimensionnelle au-delà de PEGI/ESRB,
  couvrant les applis Linux/AUR/Flathub.
- **Statistiques douces & bilan parental IA** — heures, applis et catégories de sites, plus un résumé qui
  aide le parent à engager le dialogue — pas un dossier de surveillance.

## Documentation

| Document | Contenu |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) / [`docs/KONCEPCJA.md`](docs/KONCEPCJA.md) | Concept complet, avantages, cas d'usage, idées non évidentes |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) / [`docs/ARCHITEKTURA.md`](docs/ARCHITEKTURA.md) | Architecture : démon, points d'interception, push-approval, sécurité |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Étapes de construction (MVP → v1), en petites étapes vérifiables |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Conception anti-contournement : niveaux d'adversaire, chaque vecteur → sa parade, limites honnêtes |
| [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) · [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md) | Réponses aux deux audits de sécurité externes (chaque constat → correctif/statut) |
| [`android-app/`](android-app/) | **Application parent native Android** (Kotlin) — client recommandé ; signature Ed25519 sur l'appareil, push en arrière-plan |
| [`parent-app/`](parent-app/) | Application parent multiplateforme (PWA installable) — signature Ed25519 sur l'appareil |
| [`packaging/`](packaging/) | Déploiement : installateur, unité systemd, hook pacman, sortie nftables, liste blanche fapolicyd, audit |
| [`docs/TESTING.md`](docs/TESTING.md) | Plan de test pas à pas sur un système propre (couche par couche + tests d'intrusion red-team) |

## 📱 Application parent Android

Le client recommandé pour le parent est une **application Android native** (Kotlin) — téléchargez-la sur
la **[page Releases](https://github.com/wasyleque/omarchy-child-protect-guardian/releases/latest)** et
associez-la en scannant un QR code. Interface en
**English · Polski · Español · Deutsch · Français · 中文** (suit la langue du téléphone).

<p align="center">
  <img src="docs/screenshots/android-app.png" alt="Guardian Parent — Android app" width="320">
</p>

Ce qu'elle permet :

- **Autoriser ou refuser les installations** — lorsque l'enfant lance `pacman`/`yay`/`flatpak`/`snap`/`nix`,
  la demande est retenue et vous recevez un push avec le nom de l'appli, la commande exacte et un voyant
  de risque IA 🟢/🟡/🔴 ; appuyez sur **Autoriser / Refuser** depuis la notification *ou* depuis la liste
  dans l'application. Chaque décision est signée sur l'appareil, de sorte qu'un broker compromis ne peut
  jamais falsifier un « Autoriser ».
- **Plusieurs enfants, un seul téléphone** — ajoutez chaque enfant (son propre PC ou un PC partagé) et
  basculez de l'un à l'autre ; chaque demande/notification indique le prénom de l'enfant.
- **Horaire de temps d'écran sans saisie** — choisissez une limite quotidienne dans un menu déroulant et
  les heures autorisées via un sélecteur d'heure, identiques chaque jour ou par jour de la semaine.
  Envoyé à l'ordinateur avec signature cryptographique.
- **Verrouillage instantané** — verrouillez (et déverrouillez) immédiatement l'ordinateur de l'enfant
  depuis votre téléphone.
- **Temps supplémentaire** — accordez +15 / +30 minutes à un enfant précis en un geste.
- **Bilan quotidien IA** — un résumé quotidien optionnel indiquant ce qui a été utilisé, si l'enfant a
  tenté quelque chose d'interdit et un avertissement clair si des sujets préoccupants (drogues,
  automutilation, armes…) sont apparus — généré par un modèle **local** sur votre propre machine, pour
  que rien ne quitte votre réseau.

## 🍏 Application parent sur iPhone/iPad, Windows, Mac ou Linux (sans Android)

Vous n'avez pas de téléphone Android ? L'application parent fonctionne aussi comme **application web
installable (PWA)** — la même signature sur l'appareil, simplement ouverte dans un navigateur. Sans App
Store, sans compte.

1. **Ouvrez le lien de l'application** dans votre navigateur (votre copie hébergée de
   [`parent-app/`](parent-app/) en HTTPS — p. ex. une URL GitHub Pages gratuite ; configuration détaillée
   dans [`parent-app/README.md`](parent-app/README.md)).
2. **Installez-la :**
   - **iPhone / iPad :** touchez **Partager → Sur l'écran d'accueil**, puis ouvrez-la toujours depuis
     cette nouvelle icône. (Apple n'autorise les notifications pour les web apps que de cette façon —
     iOS 16.4 ou plus récent.)
   - **Windows / Mac / Linux :** cliquez sur **Installer** dans la barre d'adresse du navigateur
     (Chrome/Edge) pour l'avoir comme application de bureau, ou utilisez simplement l'onglet.
3. **Associez-la :** scannez le même QR code affiché sur l'ordinateur de l'enfant (ou collez une fois le
   serveur + le sujet).
4. **Activez les alertes en arrière-plan :** touchez **Enable background notifications** pour qu'une
   demande d'installation sonne même lorsque l'application est fermée. Toucher la notification ouvre
   l'application et affiche la vraie demande vérifiée par signature.

Vous bénéficiez de l'essentiel en toute sécurité : **Autoriser / Refuser signés** qu'un broker piraté ne
peut pas falsifier, votre clé ne quitte jamais l'appareil, et cela fonctionne hors ligne hormis pour le
flux de demandes en direct. **L'application Android reste le client le plus complet** (éditeur d'horaire,
verrouillage instantané, crédits de temps supplémentaire, vue du bilan quotidien et réglages du serveur
IA) ; l'application web se concentre sur **les approbations et les alertes**. Choisissez ce qui correspond
à votre téléphone.

## Statut

✅ **Fonctionnel — en phase de tests actifs, pas encore comme unique protection d'un enfant.** Le cœur est
construit et testé (unit/e2e), tourne sur du vrai matériel (la boucle push-approval, le temps d'écran,
fapolicyd, nftables et la chaîne d'audit se comportent comme prévu), et l'application native Android est
installée et opérationnelle. Deux audits de sécurité externes (2026-10-09, 2026-10-14) et trois tests sur
matériel réel (2026-10-21/23) ont permis de corriger de vrais bogues d'application. **Tant que la base
matérielle complète + fapolicyd en mode enforcing ne sont pas configurés et vérifiés sur votre machine,
considérez cet outil comme une aide solide, et non comme votre seule ligne de défense.** Voir les audits
[`guardian-security-review.md`](guardian-security-review.md) + [`guardian-security-review-2.md`](guardian-security-review-2.md),
les réponses [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) + [`docs/REVIEW-RESPONSE-2.md`](docs/REVIEW-RESPONSE-2.md)
et les rapports de test [`guardian-test-report-2026-10-21.md`](guardian-test-report-2026-10-21.md) +
[`guardian-test-report-2026-10-23.md`](guardian-test-report-2026-10-23.md). Déploiement selon [`docs/TESTING.md`](docs/TESTING.md).

---

## 👤 Auteur

**Créé par [wasyleque](https://github.com/wasyleque).**

## ❤️ Soutenir le projet

Si Omarchy Child Protect Guardian vous est utile, vous pouvez soutenir son développement via **PayPal** :
**[wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)**

Chaque contribution aide à le garder gratuit, privé et ouvert.

## 🤝 Contribuer & partager des idées

Ce projet grandit grâce aux idées de la communauté. **Nous vous invitons chaleureusement à :**
- 💡 **partager des idées** — ouvrez une [Issue](../../issues) avec le label `idea` (un modèle existe),
- 🛠️ **co-créer des fonctionnalités** — choisissez un élément de la [Roadmap](docs/ROADMAP.md) et ouvrez une PR,
- 🌍 **traduire la documentation** dans d'autres langues.

Voir [`CONTRIBUTING.md`](CONTRIBUTING.md). Aucune idée n'est trop petite — construisons ensemble
le meilleur outil de sécurité pour enfants, toutes plateformes confondues.

## Licence

À définir (proposition : GPL-3.0 — un outil de sécurité dont la valeur vient de son ouverture et de son auditabilité).

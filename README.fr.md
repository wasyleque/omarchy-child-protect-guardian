# Omarchy Child Protect Guardian 🛡️

**🌐 Langues :** [English](README.md) · [Polski](README.pl.md) · [Español](README.es.md) · [Deutsch](README.de.md) · **Français** · [中文](README.zh.md)

> Contrôle parental et sécurité des enfants en ligne pour **Omarchy Linux** (Arch + Hyprland),
> avec **approbation à distance des installations d'applis** depuis le téléphone du parent —
> un flux d'approbation push, exactement comme la confirmation d'une connexion 2FA.

**Statut :** 🌱 ébauche conceptuelle (phase 0 — collecte d'idées). Voir [`AGENTS.md`](AGENTS.md).

Objectif ambitieux : être **plus pratique et plus efficace** que Microsoft Family Safety,
Apple Screen Time, Google Family Link et Qustodio — tout en restant **privé** (aucune télémétrie,
aucun envoi des données de l'enfant vers le cloud d'une entreprise).

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
- **Filtre de contenu système** — DNS + eBPF, SafeSearch / mode restreint forcés.
- **Rapports lisibles pour le parent** — ce que l'enfant a fait et demandé (sans espionnage corporatif).
- **Approbations signées cryptographiquement** — même un broker push compromis ne peut falsifier un « Autoriser ».

## Documentation

| Document | Contenu |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) | Concept complet, avantages, cas d'usage, idées non évidentes |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) | Architecture : démon, points d'interception, push-approval, sécurité |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Étapes de construction (MVP → v1), en petites étapes vérifiables |

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

<p align="center">
  <img src="images/EH3D_PHOENIX_PNP_BANNER.svg" alt="EH3D-PHOENIX-PNP banner" width="100%">
</p>

# 📖 EH3D-PHOENIX-PNP

<p align="center"><a href="README.md">🇺🇸 English</a> | <a href="README_spa.md">🇪🇸 Español</a> | 🇫🇷 <b>Français</b> | <a href="README_ita.md">🇮🇹 Italiano</a> | <a href="README_deu.md">🇩🇪 Deutsch</a> | <a href="README_zho.md">🇨🇳 中文</a> | <a href="README_jpn.md">🇯🇵 日本語</a> | <a href="README_por.md">🇵🇹 Português</a></p>

### Assemblage pick and place : positions, chargeurs et ordre de pose

<p align="center">
  <img src="https://img.shields.io/badge/License-GPL%203.0--or--later-blue.svg" alt="GPL 3.0 or later">
  <img src="https://img.shields.io/badge/Stack-Rust-DEA584.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Languages-8-00E5FF.svg" alt="Languages">
  <img src="https://img.shields.io/badge/Version-0.0.4-0f6cbd.svg" alt="Version">
  <img src="https://img.shields.io/badge/Maturity-scaffolding-FFB020.svg" alt="Maturity">
</p>

---

**Vérification d'honnêteté - ce qui fonctionne aujourd'hui :** une bibliothèque Rust et un programme en ligne de commande (`phoenix-pnp`) avec 248 tests qui s'exécutent sur un ordinateur : lit le fichier de positions d'une carte, donne à chaque composant son alimentateur (avec des priorités, des plateaux et des bandes), lit les réglages de la machine dans un fichier, planifie l'ordre de pose pour une tête de jusqu'à six buses (rangs, pointes de buse, limites d'angle, temps des déplacements, panneaux), écrit une feuille de chargement et, sur une machine simulée, exécute le plan pas à pas avec des pannes injectées. Il a aussi une fenêtre (construite avec `--features gui`) dont la logique est testée sans écran ; le dessin lui-même n'est pas couvert par des tests et n'a pas été vérifié sur un écran. Il ne pilote aucune machine : il n'a pas été essayé sur une machine et il n'a pas été vérifié avec une collection de fichiers réels.

---

> **État de sécurité :** ce programme ne commande aucune machine et n'est pas un système de sécurité. Ce qu'il produit doit être vérifié par une personne avant tout essai sur une machine ; rien ici n'a fonctionné sur du matériel réel.


## 📋 En un coup d'œil

| | |
|---|---|
| Version | `0.0.4` |
| Maturité | Échafaudage |
| Tests | 248 |
| Pile technique | Rust |
| Sur du matériel réel | Non essayé : rien ici n'a fonctionné sur une machine |

**Aller à  :** [Présentation](#-présentation) · [Architecture](#-architecture) · [Démarrage rapide](#-démarrage-rapide) · [Tests](#-tests) · [Structure](#-structure-du-dépôt) · [Liés](#-projets-liés)

## 🚦 État du projet

Chaque ligne est une partie de ce dépôt et le feu dit où elle en est aujourd'hui. Elle est écrite d'après ce que le dépôt peut montrer (tests, compilations, exécutions), pas d'après ce qui est prévu.

🟢 Fait et vérifié · 🟡 Fonctionne, avec des limites connues · 🟠 À moitié fait ou pas encore vérifié · 🔴 Pas fait ou ne fonctionne pas

| Domaine | État | Ce que cela veut dire ici |
|---|---|---|
| Cœur | 🟡 | lit le fichier de positions d'une carte, donne à chaque composant son alimentateur (priorité, plateau, bande), planifie l'ordre de pose pour une tête de jusqu'à six buses avec des rangs, des pointes de buse, des limites d'angle et le temps des déplacements, planifie des panneaux et écrit la feuille de chargement ; testé sur un ordinateur avec des entrées normales et abîmées, pas avec une collection de fichiers réels. |
| Ligne de commande | 🟡 | `phoenix-pnp` exécute les mêmes fonctions que la bibliothèque (`parts`, `check`, `plan`, `schedule`, `preflight`, `simulate`, `loadsheet`, `config-check`) et affiche le résultat ; chaque erreur revient comme un message avec son code `PHOENIX-PNP-NNN` et un code de sortie non nul. |
| Tests | 🟡 | 248 tests passent (`cargo test`), avec des cas normaux, des cas limites, des entrées abîmées et de petits cas générés. |
| Configuration de la machine | 🟡 | un fichier `clé = valeur` avec une version, un validateur qui nomme la ligne et les limites de sécurité (course de Z, limites de X et Y, nouvelles tentatives, angles de buse) ; les valeurs n'ont pas été comparées à une machine réelle. |
| Simulateur et travail | 🟡 | une horloge virtuelle, des pannes injectées au hasard (avec une graine) ou à un endroit choisi, des nouvelles tentatives, les politiques alerte et report, des alimentateurs qui s'éteignent après des pannes répétées, pause, pas à pas et un état enregistré pour reprendre ; il ne fait que simuler, rien n'a piloté une machine. |
| Sécurité | 🟡 | Rust sûr et simple (`forbid(unsafe_code)`) : rien ne panique sur les fichiers reçus et une mauvaise valeur est une erreur avec sa raison ; pas de revue indépendante. |
| Fenêtre | 🟠 | Une fenêtre egui (à construire avec `--features gui` ; `phoenix-pnp` sans commande l'ouvre) : onglets de travail, pièces, boîtiers, alimentateurs, feuille de chargement, machine, problèmes et journal, une carte de la table de travail avec le trajet de la tête, des marques de polarité et la pièce suivante, un assistant en cinq étapes, des problèmes avec des corrections qu'on peut annuler, un travail simulé qu'on peut mettre en pause, reprendre et avancer pas à pas, et les huit langues. Sa logique est testée sans écran ; le dessin n'est pas couvert par des tests et n'a pas été vérifié sur un écran. |
| Matériel réel | 🔴 | Non essayé : rien ici n'a été exécuté sur une machine. |
| Documentation | 🟡 | README en huit langues, le journal des modifications et les deux images dessinées depuis `docs/`. |

## 🎯 Présentation

* **Les composants** (`src/parts.rs`) : un fichier `.pos` de KiCad ou un CSV (virgule ou point-virgule, guillemets, en-tête trouvé par nom) avec des positions en mm, mil ou pouces, les angles ramenés à 0-360, la face dessus ou dessous, et chaque ligne absurde est refusée avec son numéro ; une référence répétée est une erreur.
* **Les chargeurs** (`src/feeders.rs`) : un CSV de chargeurs (id, valeur, boîtier, position de prise, pièces restantes) ; chaque composant reçoit le premier chargeur qui contient sa valeur et son boîtier, sans tenir compte de la casse ; un composant sans chargeur et un chargeur qui s'épuise sont signalés, et un second chargeur du même type prend le relais.
* **Le plan** (`src/plan.rs`) : les emplacements sont parcourus au plus proche et coupés en lots d'une à six buses, les chargeurs d'un lot sont visités au plus proche, la face dessous est inversée selon la largeur de la carte, et le plan donne le trajet et une estimation du temps ; il s'écrit comme un travail JSON.
* **Les tables et les réglages** (`src/catalog.rs`, `src/config.rs`, `src/rotation.rs`) : des tables de pièces et de boîtiers dont les valeurs sont héritées (pièce, puis boîtier, puis les valeurs par défaut), un fichier de configuration de la machine au format `clé = valeur` (`examples/machine.cfg` documente chaque clé), davantage de noms pour les colonnes d'un CSV de positions et des corrections de rotation par boîtier.
* **Les alimentateurs avec plus qu'une bande** (`src/feeding.rs`) : priorité, plateaux, bandes et alimentateurs éteints ; l'alimentateur d'un composant est choisi de la même façon quel que soit l'ordre des fichiers.
* **Le planificateur de la machine** (`src/planner.rs`, `src/motion.rs`) : rangs (blocs posés l'un après l'autre), stratégies d'ordre, pointes de buse et leurs changements, limites de l'angle de la buse, temps des déplacements avec accélération et à-coup, parcours plus courts (2-opt) et correction de l'origine d'un composant.
* **Panneaux, feuille de chargement et exports** (`src/panel.rs`, `src/loadsheet.rs`, `src/export.rs`) : cartes et panneaux dans des panneaux, l'emplacement de chaque type de bande, un rapport, la liste de ce qui manque et un G-code qui ne fait que déplacer la tête.
* **Le simulateur et le travail** (`src/sim.rs`, `src/job.rs`, `src/preflight.rs`, `src/model.rs`) : les vérifications avant un travail, une machine à horloge virtuelle et pannes injectables, et un travail qu'on peut mettre en pause, exécuter pas à pas, enregistrer et reprendre.
* **La fenêtre** (`src/gui.rs`, `src/gui/`) : ce que la fenêtre fait en dehors du dessin n'a pas besoin d'écran (les fichiers chargés, le plan et ses problèmes avec corrections, le travail simulé, la géométrie de la carte, les tableaux à sélection partagée, l'assistant en cinq étapes et les textes en huit langues) et est testé ; seul `src/gui/window.rs`, construit avec `--features gui`, le dessine avec egui. La vitesse de la simulation, le thème et la taille dessinée pour une pièce de boîtier inconnu sont des clés de `machine.cfg` (`gui_steps_per_s`, `gui_theme`, `gui_part_size_mm`).

## 🧩 Rôle dans EH3D

EH3D-PHOENIX-PNP est un programme de l'EH3D Fabrication Suite. Aujourd'hui il fonctionne comme bibliothèque, ligne de commande et fenêtre (construite avec `--features gui`) ; la fenêtre n'a pas encore l'aspect commun aux autres programmes de la suite. Il lit et écrit des fichiers simples et peut donc s'utiliser seul ou entre les autres programmes.

## 🏛️ Architecture

<p align="center">
  <img src="images/architecture.svg" alt="Architecture de EH3D-PHOENIX-PNP : qui l'utilise, ce qu'il contient et de quoi il dépend" width="100%">
</p>


Comment les parties de ce dépôt s'articulent : qui l'utilise, ce qu'il contient et de quoi il dépend. L'image est dessinée depuis `docs/architecture.json` et vérifiée avec le code.

## ✨ Fonctionnalités

* **Une bibliothèque et une ligne de commande :** chaque commande de `phoenix-pnp` est une fonction de la bibliothèque ; un autre programme de la suite peut l'appeler sans lancer de processus.
* **Ligne de commande stricte :** une option mal écrite (`--powr`) ou un mot en trop est une erreur qui la nomme, et non quelque chose d'ignoré ; sinon le programme ferait sa sortie avec les valeurs par défaut.
* **Tests avec entrées abîmées :** les tests fournissent aussi des fichiers vides, du bruit, des nombres énormes et des fichiers coupés, et aucun ne doit paniquer.
* **Sans deviner :** une valeur absurde est une erreur qui dit laquelle et pourquoi, jamais un résultat qui n'en a que l'air.
* **Pas de code unsafe :** `#![forbid(unsafe_code)]` dans la bibliothèque et le programme.
* **Un seul lecteur pour les deux tables, et rapide sur une grande carte :** le fichier de positions et celui des alimentateurs sont lus par le même lecteur (virgule ou point-virgule, champs entre guillemets avec un séparateur ou un saut de ligne), cinquante mille composants sont vérifiés, affectés et planifiés en un instant, le plan n'est jamais écrit sur un fichier dont il vient, une option inconnue est une erreur, et `--origin-x` dit où se trouve le bord gauche de la carte quand la face inférieure est en miroir.

## 🔒 Sécurité et protection

| Menace | Défense |
|---|---|
| Un fichier abîmé, coupé ou vide | pas de panique : les tests coupent une entrée valide à chaque position et fournissent du bruit |
| Un nombre absurde (négatif, non fini, démesuré) | refusé avec une erreur qui nomme la valeur ; rien n'est arrondi en quelque chose de plausible |
| Usage non sûr de la mémoire | Rust sûr et simple avec `forbid(unsafe_code)` |

**Ce qui n'est pas défendu :** un résultat produit sans erreur n'est pas un résultat vérifié. Rien ici ne connaît les limites d'une machine et le programme n'a pas eu de revue indépendante. Signalez une vulnérabilité en privé (voir [SECURITY.md](SECURITY.md)).

## 🔌 Interfaces

La bibliothèque est `eh3d_phoenix_pnp` (`use eh3d_phoenix_pnp::...`) ; la ligne de commande est `phoenix-pnp`. Les commandes sont celles-ci ; chacune affiche son résultat sur la sortie standard et signale une erreur sur la sortie d'erreur avec un code de sortie non nul. Il n'y a pas d'accès réseau.

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               les tables de pièces et de boîtiers et l'héritage de leurs propriétés
│   ├── cli_args.rs                              les arguments de la ligne de commande, stricts : une option inconnue est une erreur
│   ├── commands.rs                              les commandes qui utilisent la configuration : schedule, preflight, simulate, loadsheet
│   ├── config.rs                                le fichier de configuration de la machine : clés, plages et limites de sécurité
│   ├── error.rs                                 les codes d'erreur PHOENIX-PNP-NNN et leur sens en huit langues
│   ├── export.rs                                le rapport, la liste de ce qui manque et le G-code des déplacements
│   ├── feeders.rs                               une partie de la logique
│   ├── feeding.rs                               alimentateurs avec priorité, plateaux et bandes, et le choix de l'alimentateur d'un composant
│   ├── gui.rs                                   la fenêtre : des modules sans écran et le dessin dans window.rs
│   ├── gui/app.rs                               l'état de la fenêtre : charger, planifier, démarrer, pause, pas, corriger, enregistrer
│   ├── gui/log.rs                               le journal avec niveaux et le texte du diagnostic à copier
│   ├── gui/problems.rs                          les problèmes d'un travail et les corrections qui les règlent
│   ├── gui/samples.rs                           des exemples, de la petite carte à la grande
│   ├── gui/session.rs                           le travail simulé qu'on démarre, met en pause, reprend et avance pas à pas
│   ├── gui/settings.rs                          ce dont la fenêtre se souvient : langue, thème, couches et dossier de sortie
│   ├── gui/tables.rs                            les lignes, le tri et la sélection partagée par les tableaux
│   ├── gui/texts.rs                             chaque mot de la fenêtre dans les huit langues
│   ├── gui/theme.rs                             les couleurs : vert phénix, clair et contraste élevé
│   ├── gui/view.rs                              la géométrie de la carte : corps, marques de polarité, ajustement, zoom et sélection
│   ├── gui/window.rs                            le dessin avec egui, construit avec la fonctionnalité gui : onglets, carte et tableaux
│   ├── gui/wizard.rs                            l'assistant en cinq étapes, de rien à un travail simulé
│   ├── gui/workspace.rs                         les fichiers chargés, le plan et les modifications, sans fenêtre
│   ├── job.rs                                   le travail : étapes, pause, nouvelles tentatives, politiques, statistiques et état enregistré
│   ├── lib.rs                                   la bibliothèque : ce que chaque commande utilise
│   ├── loadsheet.rs                             la feuille de chargement : l'emplacement de chaque type de bande
│   ├── main.rs                                  la ligne de commande et l'entrée de la fenêtre quand il s'exécute sans commande
│   ├── model.rs                                 les parties d'une machine en traits : machine, buse, alimentateur, actionneur, caméra, pilote
│   ├── motion.rs                                le temps d'un déplacement, avec accélération et à-coup
│   ├── panel.rs                                 panneaux : cartes et panneaux dans des panneaux, chacun à sa place
│   ├── parts.rs                                 une partie de la logique
│   ├── plan.rs                                  une partie de la logique
│   ├── planner.rs                               le planificateur qui connaît la machine : rangs, ordre, pointes, angles et temps
│   ├── preflight.rs                             les vérifications avant un travail, avec une gravité et ce qu'elles désignent
│   ├── rotation.rs                              corrections de rotation par boîtier, et comment tourne une buse limitée
│   ├── sim.rs                                   la machine simulée : horloge virtuelle et pannes injectables
│   └── units.rs                                 longueurs et lieux qui portent leur unité
├── tests/
│   ├── commands.rs                              tests des commandes qui utilisent la configuration
│   ├── feeding.rs                               tests des alimentateurs : priorité, plateaux et bandes
│   ├── gui.rs                                   tests de la fenêtre sans écran : état, plan, travail, carte, tableaux et assistant
│   ├── job.rs                                   tests du travail : étapes, nouvelles tentatives et état enregistré
│   ├── loadsheet.rs                             tests de la feuille de chargement et des exports
│   ├── model.rs                                 tests du modèle de données : codes, longueurs, configuration et tables
│   ├── more.rs                                  d'autres tests de la bibliothèque et de la ligne de commande
│   ├── panel.rs                                 tests des panneaux
│   ├── planner.rs                               tests du temps des déplacements et du planificateur
│   ├── pnp.rs                                   tests de la bibliothèque et de la ligne de commande
│   ├── preflight.rs                             tests des vérifications avant un travail
│   ├── readme.rs                                tests que le README est exécutable et complet
│   ├── sim.rs                                   tests de la machine simulée
│   └── strict.rs                                tests de la ligne de commande : options et mots qu'elle ne connaît pas
├── examples/
│   ├── board.pos                                un exemple pour essayer le programme
│   ├── catalog.csv                              une table de pièces : rang et hauteur
│   ├── feeders.csv                              un exemple pour essayer le programme
│   ├── machine.cfg                              les réglages de la machine, chaque clé expliquée
│   ├── packages.csv                             une table de boîtiers : hauteur, pointe, rang et bande
│   ├── panel.json                               un panneau de deux cartes
│   └── slots.csv                                les emplacements de la machine
├── docs/
│   ├── architecture.json                        données de l'image d'Architecture
│   └── structure.json                           données de l'image de Structure
├── tools/                                       contrôle de la CI et du README et outils d'images
├── images/                                      bannière et images d'Architecture et de Structure du dépôt
├── .github/                                     workflows de CI et de publication, modèles d'issues et de pull requests
├── eh3d.project.json                            manifeste : version, rôle, maturité et commande de compilation
├── Cargo.toml Cargo.lock                        le workspace et ses dépendances verrouillées
├── rust-toolchain.toml                          la toolchain Rust épinglée
├── CHANGELOG.md                                 ce qui a changé dans chaque version
├── SECURITY.md SUPPORT.md                       signaler une vulnérabilité et poser des questions
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   contribuer, code de conduite et licence
```

## ⚙️ Configuration

La machine et le planificateur se configurent avec un fichier texte de lignes `clé = valeur` (`--config FICHIER`), le même format que les autres programmes de la suite ; `examples/machine.cfg` liste chaque clé avec sa valeur par défaut et son effet. Une clé absente garde sa valeur par défaut ; une clé inconnue, écrite deux fois ou hors de sa plage est une erreur qui nomme la ligne. Les options de la ligne de commande (`--nozzles`, `--speed`, `--order`, `--side`) l'emportent sur le fichier.

Les tables de boîtiers et de pièces (`--packages`, `--catalog`) sont des fichiers CSV où une cellule vide hérite ; un panneau (`--panel`) est un fichier JSON ; les emplacements de la machine (`loadsheet`) sont un CSV avec `id,x,y,max_width_mm`. Un fichier plus grand que `max_file_bytes` est refusé. Chaque erreur a un code `PHOENIX-PNP-NNN` ; son message est écrit dans la langue de la clé `language` ou de `--lang` (`en`, `es`, `de`, `fr`, `it`, `ja`, `zh` ou `pt`) :

| Code | Signification |
|---|---|
| `PHOENIX-PNP-001` | Le fichier dépasse la taille autorisée |
| `PHOENIX-PNP-002` | Il y a plus de pièces que la limite ne l'autorise |
| `PHOENIX-PNP-003` | Une ligne de la configuration n'est pas clé = valeur |
| `PHOENIX-PNP-004` | La configuration contient une clé inconnue |
| `PHOENIX-PNP-005` | Une valeur de la configuration n'est pas valide |
| `PHOENIX-PNP-006` | Une valeur de la configuration est hors de sa plage |
| `PHOENIX-PNP-007` | La configuration a été écrite pour une version plus récente du programme |
| `PHOENIX-PNP-008` | Une clé de la configuration est écrite deux fois |
| `PHOENIX-PNP-009` | Un réglage dépasse les limites de sécurité |
| `PHOENIX-PNP-010` | Une ligne de la table des pièces ou des boîtiers n'est pas valide |
| `PHOENIX-PNP-011` | Une pièce ou un boîtier est défini deux fois |
| `PHOENIX-PNP-012` | Une règle de correction de rotation n'est pas valide |
| `PHOENIX-PNP-013` | Une ligne des alimentateurs n'est pas valide |
| `PHOENIX-PNP-014` | Le fichier du panneau n'est pas valide |
| `PHOENIX-PNP-015` | Le panneau est imbriqué trop profondément ou a trop de cartes |
| `PHOENIX-PNP-016` | La table des emplacements n'est pas valide |
| `PHOENIX-PNP-017` | Un réglage du plan n'est pas valide |
| `PHOENIX-PNP-018` | La buse ne peut pas tourner à cet angle dans ses limites |
| `PHOENIX-PNP-019` | La buse tient déjà une pièce : elle ne peut pas en prendre une autre |
| `PHOENIX-PNP-020` | La buse ne tient aucune pièce à poser |
| `PHOENIX-PNP-021` | Aucun alimentateur n'est disponible pour la pièce |
| `PHOENIX-PNP-022` | La pièce n'a pas pu être prise après toutes les tentatives |
| `PHOENIX-PNP-023` | L'alimentateur n'a pas pu avancer après toutes les tentatives |
| `PHOENIX-PNP-024` | La pièce n'a pas pu être posée après toutes les tentatives |
| `PHOENIX-PNP-025` | L'état enregistré appartient à un autre travail |
| `PHOENIX-PNP-026` | L'état enregistré du travail n'est pas valide |
| `PHOENIX-PNP-027` | Le travail est en pause, terminé ou arrêté par une erreur |
| `PHOENIX-PNP-028` | Les vérifications avant le travail ont trouvé des erreurs |
| `PHOENIX-PNP-029` | Une longueur ou un angle n'est pas valide |
| `PHOENIX-PNP-030` | Le pilote n'a pas accepté la commande |
| `PHOENIX-PNP-031` | Le simulateur a injecté une panne |

## 🚀 Démarrage rapide

Il faut une chaîne d'outils Rust stable (1.95 ou plus récente).

```bash
cargo build --release
cargo test
cargo run --release -- plan examples/board.pos examples/feeders.csv examples/plan.json --nozzles 2
cargo run --release -- schedule examples/board.pos examples/feeders.csv examples/schedule.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --state examples/job_state.json --steps 5
cargo run --release -- loadsheet examples/board.pos examples/slots.csv examples/loadsheet.csv --packages examples/packages.csv
cargo run --release --features gui
```

La dernière ligne ouvre la fenêtre ; `phoenix-pnp` sans commande le fait aussi s'il a été construit avec `--features gui` (sous Linux la fenêtre demande les fichiers de développement de X11 ou Wayland et d'OpenGL). Avec une commande, le programme reste une ligne de commande.

## 🧪 Tests

La suite compte 248 tests (`cargo test`, exécutés sous Windows et Linux dans la CI). Ils vérifient les cas normaux, les cas limites et les entrées abîmées : les fichiers vides, le bruit et les fichiers coupés ne provoquent jamais de panique. De petits cas générés (à partir d'une graine fixe, donc avec les mêmes nombres à chaque exécution) vérifient que le plan ne dépend pas de l'ordre des fichiers, qu'un parcours n'est jamais allongé, qu'un travail enregistré puis repris n'importe où se termine comme celui qui ne s'est jamais arrêté, et que les longueurs et les angles se convertissent dans les deux sens. La CI exécute aussi `cargo fmt --check`, `cargo clippy` avec les avertissements en erreurs et `python -I tools/ci_validate.py`.

## 🏗️ Compilation et publication

La version se trouve dans `Cargo.toml` et `eh3d.project.json` ; chaque version a son entrée dans [CHANGELOG.md](CHANGELOG.md). `build.bat` / `build.sh` compilent le programme, `test.bat` / `test.sh` lancent les tests et `run.bat` / `run.sh` l'exécutent avec un exemple. Les versions sont publiées sur GitHub.

## 📂 Structure du dépôt

<p align="center">
  <img src="images/repository-structure.svg" alt="Structure du dépôt EH3D-PHOENIX-PNP" width="100%">
</p>

<details>
<summary>Version texte de l'arborescence</summary>

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               les tables de pièces et de boîtiers et l'héritage de leurs propriétés
│   ├── cli_args.rs                              les arguments de la ligne de commande, stricts : une option inconnue est une erreur
│   ├── commands.rs                              les commandes qui utilisent la configuration : schedule, preflight, simulate, loadsheet
│   ├── config.rs                                le fichier de configuration de la machine : clés, plages et limites de sécurité
│   ├── error.rs                                 les codes d'erreur PHOENIX-PNP-NNN et leur sens en huit langues
│   ├── export.rs                                le rapport, la liste de ce qui manque et le G-code des déplacements
│   ├── feeders.rs                               une partie de la logique
│   ├── feeding.rs                               alimentateurs avec priorité, plateaux et bandes, et le choix de l'alimentateur d'un composant
│   ├── job.rs                                   le travail : étapes, pause, nouvelles tentatives, politiques, statistiques et état enregistré
│   ├── lib.rs                                   la bibliothèque : ce que chaque commande utilise
│   ├── loadsheet.rs                             la feuille de chargement : l'emplacement de chaque type de bande
│   ├── main.rs                                  la ligne de commande
│   ├── model.rs                                 les parties d'une machine en traits : machine, buse, alimentateur, actionneur, caméra, pilote
│   ├── motion.rs                                le temps d'un déplacement, avec accélération et à-coup
│   ├── panel.rs                                 panneaux : cartes et panneaux dans des panneaux, chacun à sa place
│   ├── parts.rs                                 une partie de la logique
│   ├── plan.rs                                  une partie de la logique
│   ├── planner.rs                               le planificateur qui connaît la machine : rangs, ordre, pointes, angles et temps
│   ├── preflight.rs                             les vérifications avant un travail, avec une gravité et ce qu'elles désignent
│   ├── rotation.rs                              corrections de rotation par boîtier, et comment tourne une buse limitée
│   ├── sim.rs                                   la machine simulée : horloge virtuelle et pannes injectables
│   └── units.rs                                 longueurs et lieux qui portent leur unité
├── tests/
│   ├── commands.rs                              tests des commandes qui utilisent la configuration
│   ├── feeding.rs                               tests des alimentateurs : priorité, plateaux et bandes
│   ├── job.rs                                   tests du travail : étapes, nouvelles tentatives et état enregistré
│   ├── loadsheet.rs                             tests de la feuille de chargement et des exports
│   ├── model.rs                                 tests du modèle de données : codes, longueurs, configuration et tables
│   ├── more.rs                                  d'autres tests de la bibliothèque et de la ligne de commande
│   ├── panel.rs                                 tests des panneaux
│   ├── planner.rs                               tests du temps des déplacements et du planificateur
│   ├── pnp.rs                                   tests de la bibliothèque et de la ligne de commande
│   ├── preflight.rs                             tests des vérifications avant un travail
│   ├── readme.rs                                tests que le README est exécutable et complet
│   ├── sim.rs                                   tests de la machine simulée
│   └── strict.rs                                tests de la ligne de commande : options et mots qu'elle ne connaît pas
├── examples/
│   ├── board.pos                                un exemple pour essayer le programme
│   ├── catalog.csv                              une table de pièces : rang et hauteur
│   ├── feeders.csv                              un exemple pour essayer le programme
│   ├── machine.cfg                              les réglages de la machine, chaque clé expliquée
│   ├── packages.csv                             une table de boîtiers : hauteur, pointe, rang et bande
│   ├── panel.json                               un panneau de deux cartes
│   └── slots.csv                                les emplacements de la machine
├── docs/
│   ├── architecture.json                        données de l'image d'Architecture
│   └── structure.json                           données de l'image de Structure
├── tools/                                       contrôle de la CI et du README et outils d'images
├── images/                                      bannière et images d'Architecture et de Structure du dépôt
├── .github/                                     workflows de CI et de publication, modèles d'issues et de pull requests
├── eh3d.project.json                            manifeste : version, rôle, maturité et commande de compilation
├── Cargo.toml Cargo.lock                        le workspace et ses dépendances verrouillées
├── rust-toolchain.toml                          la toolchain Rust épinglée
├── CHANGELOG.md                                 ce qui a changé dans chaque version
├── SECURITY.md SUPPORT.md                       signaler une vulnérabilité et poser des questions
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   contribuer, code de conduite et licence
```

</details>

## 🗺️ Feuille de route et limites connues

**Limites connues aujourd'hui**

* Le dessin de la fenêtre n'a pas été vérifié sur un écran ; ses tableaux ne retiennent pas la largeur des colonnes, et elle n'a ni onglets pour les panneaux et les cartes, ni jog, ni caméra.
* Il n'a pas été essayé sur une machine ni avec une collection de fichiers réels, et il n'a eu aucune revue indépendante ; tout ce qu'il fait avec une machine est une simulation.
* Il ne lit pas les fichiers d'autres programmes de pose, et la règle de l'angle de la face inférieure d'un CSV (`bottom_rotation`) n'a pas été comparée à des fichiers réels d'un programme de CAO.
* Il n'y a pas encore de caméra, de correction de la prise par vision ni de pilote d'une machine réelle ; le plan est un fichier JSON qu'un autre programme doit exécuter.

**Ensuite**

* Amener la fenêtre à l'aspect commun aux programmes de la suite et ajouter ce qui lui manque : onglets pour les panneaux et les cartes, largeurs de colonnes mémorisées et édition des tables de boîtiers.
* Repères de la carte et du panneau, et correction par la caméra d'EH3D-VISION-STUDIO ; n'envoyer un plan à une machine qu'après l'avoir essayé sur une vraie.
* L'essayer avec des fichiers réels et avec les autres programmes de la suite.

## 🌍 Langues

Le README et les textes existent en anglais, espagnol, français, italien, allemand, chinois, japonais et portugais européen.

## 🔗 Projets liés

* [EH3D-VISION-STUDIO](https://github.com/JuanenRac/EH3D-VISION-STUDIO): les caméras, les fiduciaires et l'inspection
* [EH3D-MOTIONCNC](https://github.com/JuanenRac/EH3D-MOTIONCNC): le programme de commande de machine
* [EH3D-GERBERVIEWER](https://github.com/JuanenRac/EH3D-GERBERVIEWER): le visualiseur de couches Gerber
* [EH3D-TOOL-MANAGER](https://github.com/JuanenRac/EH3D-TOOL-MANAGER): le magasin d'outils partagé par les programmes CAM
* [EH3D-SUITE-LAUNCHER](https://github.com/JuanenRac/EH3D-SUITE-LAUNCHER): le centre qui trouve et lance les programmes de la suite

## 📚 Documentation

* [Journal des modifications](CHANGELOG.md)
* [Données d'architecture et de structure](docs/architecture.json) : les données qui servent à dessiner les deux images.

## 🤝 Contribuer et communauté

* Avant une pull request, lancez `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` et `python -I tools/ci_validate.py` : tout doit passer. Voir [CONTRIBUTING.md](CONTRIBUTING.md) et le [code de conduite](CODE_OF_CONDUCT.md).
* Le README existe en huit langues : un changement de texte se fait dans les huit fichiers, et les deux images sont dessinées depuis `docs/architecture.json` et `docs/structure.json` par `tools/make_architecture.py` et `tools/make_structure.py`.
* Signalez une vulnérabilité en privé : [SECURITY.md](SECURITY.md). Questions, idées et rapports : electrohobby3d@gmail.com ([SUPPORT.md](SUPPORT.md)).

## 👤 AUTEUR

**JuanenRac (Electro Hobby 3D)** · electrohobby3d@gmail.com

## 📜 LICENCE

Logiciel : GPL-3.0-or-later. Voir [LICENSE](LICENSE). Les conceptions matérielles de la suite utilisent la CERN OHL-S.

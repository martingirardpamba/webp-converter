# Design — Conversion vidéo dans WebP Converter

> Date : 2026-06-15
> Statut : validé (brainstorming) — à transformer en plan d'implémentation
> Auteur : Martin Girard (MAGIPA Consulting) + Claude Code

## 1. Objectif

Ajouter à l'app `webp-converter` (Tauri v2 desktop) la capacité de **convertir des vidéos lourdes en formats web légers, sans perte de qualité perceptible**, en réutilisant l'ergonomie et la mécanique de l'outil image existant (sélection → scan → conversion par lot → rapport de gain de poids).

« Sans perdre en qualité » = encodage en **qualité constante (CRF)**, visuellement sans perte, et **non** un bitrate plafonné qui dégrade les scènes complexes.

## 2. Architecture existante (baseline — à ne pas casser)

| Couche | Détail |
|---|---|
| Frontend | HTML/JS/CSS vanilla, sans build step (`frontendDist: ../src`), `withGlobalTauri: true` |
| Backend | Rust. `converter.rs` = scan + encodage image **100 % in-process** via crates `image` + `webp` (zéro binaire externe) |
| Commandes Tauri | `scan_folder`, `convert`, `convert_files`, `open_path` |
| Sortie | sous-dossier `webp/` à côté des sources, skip si déjà converti, jamais de modif des originaux |
| Plugins | `tauri-plugin-dialog` |
| Capabilities | `core:default`, `dialog:default`, `dialog:allow-open` |
| Positionnement | README : **« 2 MB installer. No dependencies. »** (argument de vente) |

**Contrainte clé** : l'encodage image ne touche à aucun binaire externe. La vidéo, elle, **exige FFmpeg** (aucun encodeur pur-Rust H.264/VP9 de qualité production). C'est le point de bascule architectural.

## 3. Décisions validées (brainstorming 2026-06-15)

| Axe | Décision | Justification |
|---|---|---|
| Intégration FFmpeg | **Bundle sidecar** (binaire embarqué via `tauri-plugin-shell`) | Zéro config utilisateur, marche hors-ligne, auto-suffisant comme l'app actuelle |
| Formats de sortie | **MP4 (H.264)** + **WebM (VP9)** | H.264 = compat universelle + décodage matériel ; VP9 = ~30 % plus léger à qualité égale pour navigateurs modernes |
| Modèle UX | **Mirror du converter image** | Slider qualité + résolution max ; CRF géré sous le capot ; cohérence avec l'existant |
| Audio | Conservé, ré-encodé (AAC pour MP4, Opus pour WebM) ; option « Silencieux » | Défaut sain pour le web ; `-an` utile pour vidéos de fond |
| Hors périmètre (YAGNI) | AV1, accélération matérielle (NVENC/QSV), trim/découpe, GIF→vidéo, multi-format simultané | Ajoutables plus tard, non nécessaires au cœur du besoin |

## 4. Architecture cible

### 4.1 Intégration FFmpeg — sidecar Tauri v2

API confirmée (docs Tauri v2) :

- **`tauri.conf.json`** → `bundle.externalBin: ["binaries/ffmpeg"]`
- Le binaire doit être nommé avec le **triplet cible** : `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe` (Windows), `ffmpeg-aarch64-apple-darwin` (macOS ARM), etc. Tauri résout le bon suffixe au build.
- **Cargo** : ajouter `tauri-plugin-shell = "2"`
- **`lib.rs`** : `.plugin(tauri_plugin_shell::init())`
- **`capabilities/default.json`** : ajouter
  ```json
  {
    "identifier": "shell:allow-execute",
    "allow": [{ "name": "binaries/ffmpeg", "sidecar": true, "args": true }]
  }
  ```
  `"args": true` car les arguments (chemins, CRF) sont dynamiques. Surface de risque maîtrisée : les arguments sont **construits côté Rust**, jamais passés bruts depuis le frontend (le frontend n'envoie que des params structurés : format, qualité, résolution, chemins).

Côté Rust :
```rust
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;

let cmd = app.shell().sidecar("ffmpeg")?.args(arg_vec);
let (mut rx, mut child) = cmd.spawn()?;
// lire CommandEvent::Stdout (progress) + ::Stderr (Duration, erreurs) + ::Terminated (exit code)
```

### 4.2 Backend Rust — nouveau module `video.rs` (isolé)

`converter.rs` **n'est pas modifié**. Nouveau module miroir :

- `const VIDEO_EXTENSIONS = ["mp4","mov","avi","mkv","webm","m4v","flv","wmv","mpg","mpeg","m2ts","3gp"]`
- `const VIDEO_OUTPUT_DIR = "web"` → sortie dans sous-dossier `web/`
- Structs : `VideoScanResult`, `VideoProgress`, `VideoReport` (équivalents des structs image, avec en plus un champ `file_percent: u8` pour le progrès **intra-fichier** — un encodage vidéo unique est long, contrairement à une image).
- Fonctions :
  - `scan_videos(folder, recursive) -> VideoScanResult` — repère les vidéos non encore converties (output `web/{stem}.{ext}` absent), comme `scan_folder`.
  - `build_ffmpeg_args(input, output, format, crf, max_height, silent) -> Vec<String>` — **pure, testable**, construit la ligne de commande.
  - `quality_to_crf(quality, format) -> u8` — **pure, testable** (voir 4.3).
  - `parse_duration(stderr_line) -> Option<f64>` et `parse_out_time(progress_line) -> Option<f64>` — **pures, testables**, pour le calcul de %.

### 4.3 Pipeline d'encodage

**Mapping qualité (slider 1-100) → CRF** (pures fonctions, clampées) :

| Format | Formule | q=100 | q=80 | q=50 | q=1 |
|---|---|---|---|---|---|
| H.264 (`libx264`) | `clamp(round(34 - q*0.16), 18, 34)` | 18 | 21 | 26 | 34 |
| VP9 (`libvpx-vp9`) | `clamp(round(40 - q*0.16), 24, 40)` | 24 | 27 | 32 | 40 |

Défaut UI : **qualité 80** (cohérent avec l'image), résolution max **1080**.

**Template H.264 → MP4** :
```
ffmpeg -hide_banner -y -i {input}
  -c:v libx264 -crf {crf} -preset medium -pix_fmt yuv420p
  -vf scale=-2:min({maxh}\,ih)
  -c:a aac -b:a 128k
  -movflags +faststart
  -progress pipe:1
  {output}.mp4
```

**Template VP9 → WebM** :
```
ffmpeg -hide_banner -y -i {input}
  -c:v libvpx-vp9 -crf {crf} -b:v 0 -row-mt 1 -deadline good -cpu-used 2 -pix_fmt yuv420p
  -vf scale=-2:min({maxh}\,ih)
  -c:a libopus -b:a 128k
  -progress pipe:1
  {output}.webm
```

Détails techniques importants :
- `-movflags +faststart` (MP4) : déplace le `moov atom` en tête → lecture progressive web (streaming avant téléchargement complet). **Indispensable pour le web.**
- `-pix_fmt yuv420p` : compatibilité maximale (Safari/iOS refusent le 4:2:2/4:4:4).
- `-b:v 0` (VP9) : active le vrai mode CRF qualité constante.
- `scale=-2:min(maxh\,ih)` : plafonne la hauteur à `maxh`, largeur auto-paire (`-2`), **jamais d'upscale** (`min` avec `ih`). La virgule du `min()` est échappée (`\,`) car la vidéo est passée en argv unique mais ffmpeg interprète la virgule comme séparateur de filtres.
- Option silencieux : remplacer le bloc audio par `-an`.
- Conserve le fps source (pas de `-r` forcé).

### 4.4 Suivi de progression

1. Spawn ffmpeg avec `-progress pipe:1`.
2. Au démarrage, ffmpeg écrit sur **stderr** une ligne `Duration: HH:MM:SS.ss, ...` → on parse → `total_seconds`.
3. `-progress pipe:1` émet sur **stdout** des blocs clé=valeur dont `out_time_us=...` (ou `out_time_ms=...`) puis `progress=end` → on parse → `current_seconds`.
4. `file_percent = round(current / total * 100)`, clampé [0,100].
5. Émission d'un événement Tauri **`video-progress`** (distinct de `convert-progress` image) → le frontend met à jour la barre.
6. Si `Duration` non parsable (rare) : progrès **indéterminé** (barre animée + « Encodage… » sans %), pas de crash.
7. Fin : événement **`video-done`** avec `VideoReport`.

### 4.5 Annulation (inclus — fort intérêt pour la vidéo)

Contrairement à l'image (rapide), un encodage vidéo peut durer des minutes. On garde le `child` handle et on expose une commande `cancel_video()` qui tue le process. Bouton **Annuler** visible pendant l'encodage vidéo. Le fichier de sortie partiel est supprimé.

### 4.6 Nouvelles commandes Tauri

```rust
#[tauri::command] fn scan_videos(folder, recursive) -> VideoScanResult
#[tauri::command] async fn convert_videos(app, files|folder, format, quality, max_height, silent) -> VideoReport
#[tauri::command] fn cancel_video(state) -> Result<(), String>
```
`converter.rs` et ses 4 commandes restent intacts.

### 4.7 Intégration UI (mirror, pas de refonte)

- **Toggle en haut de l'app** : `Images (WebP)` / `Vidéos (Web)`. Chaque mode affiche son propre panneau de réglages ; le reste de la mécanique (sélection dossier/fichiers, scan-info, barre de progrès, rapport gain) est partagé.
- Réglages mode Vidéo : **Format** (MP4 / WebM, défaut MP4), **Qualité** (slider 1-100, défaut 80), **Résolution max** (défaut 1080), **Récursif**, **Silencieux** (case à cocher).
- Filtres du dialogue fichiers : extensions vidéo (4.2).
- Rapport : réutilise l'affichage « avant → après (−X %) ». Bouton « Ouvrir le dossier web ».
- Pendant encodage vidéo : bouton **Annuler** + % intra-fichier en plus du compteur N/total.

## 5. Flux de données

```
[UI mode Vidéo] --select--> chemins
   --invoke scan_videos--> [Rust video.rs] --> VideoScanResult --> affichage "N vidéos, X Go"
   --invoke convert_videos--> [Rust] pour chaque fichier:
        build_ffmpeg_args -> app.shell().sidecar("ffmpeg").spawn()
        lire stdout(-progress)/stderr(Duration) -> emit "video-progress" --> [UI] barre + %
        à la fin du fichier: accumuler tailles, emit progress N/total
   fin de lot: emit "video-done" + retour VideoReport --> [UI] rapport
```

## 6. Modules / interfaces (chaque unité : rôle · usage · dépendances)

| Unité | Rôle | Usage | Dépend de |
|---|---|---|---|
| `video.rs` | Scan + construction commandes + parsing progrès vidéo | appelé par `lib.rs` | `tauri-plugin-shell`, std |
| `quality_to_crf` | Mappe slider → CRF | pure fn | — |
| `build_ffmpeg_args` | Construit argv ffmpeg | pure fn | — |
| `parse_duration` / `parse_out_time` | Extraient secondes des flux ffmpeg | pure fn | — |
| Commandes `scan_videos`/`convert_videos`/`cancel_video` | Pont frontend↔backend | `#[tauri::command]` | `video.rs`, `AppHandle`, état du child |
| Frontend `main.js` (mode vidéo) | UI + écoute événements | listeners `video-progress`/`video-done` | API Tauri |

Chaque fonction pure est testable sans ffmpeg ni filesystem → cœur de la stratégie de test.

## 7. Gestion des erreurs

| Cas | Traitement |
|---|---|
| Sidecar ffmpeg introuvable / spawn échoue | statut `error`, message clair, le lot continue sur le fichier suivant |
| ffmpeg exit code ≠ 0 | capturer les dernières lignes stderr comme message d'erreur |
| `Duration` non parsable | progrès indéterminé, pas de crash |
| Écriture sortie refusée (droits) | erreur ffmpeg capturée et remontée |
| Annulation utilisateur | kill du child + suppression du fichier partiel |
| Collision de nom (`clip.mov` + `clip.avi` → `clip.mp4`) | comportement identique à l'image (skip si existe) ; documenté comme limite connue |

## 8. Stratégie de test

**Tests unitaires Rust (sans ffmpeg)** :
- `quality_to_crf` : bornes et valeurs pivots (q=1/50/80/100, par format).
- détection d'extension vidéo (casse, extensions inconnues).
- dérivation du chemin de sortie + logique skip (`web/{stem}.{ext}`).
- `parse_duration` / `parse_out_time` sur des échantillons de stderr/stdout ffmpeg réels.
- `build_ffmpeg_args` : présence des flags critiques (`-crf`, `-movflags +faststart`, `-an` si silencieux, échappement `\,`).

**Test d'intégration (ffmpeg requis)** :
- Générer une vidéo fixture courte (`ffmpeg -f lavfi -i testsrc=duration=2 ...`) → convertir → asserter : sortie existe, plus légère, lisible (re-probe via ffmpeg). Marqué `#[ignore]` si ffmpeg absent en CI.

**QA manuelle (matrice)** : mp4→mp4, mov→webm, portrait, avec/sans audio, 4K→1080, déjà converti (skip), option silencieux, annulation en cours.

## 9. Build / CI

- Script `scripts/fetch-ffmpeg.ps1` (+ `.sh`) télécharge des **builds statiques** ffmpeg par plateforme dans `src-tauri/binaries/ffmpeg-{triplet}{.exe}`, avec **version épinglée + vérification checksum**.
  - Windows : gyan.dev ou BtbN (GPL, statique, x264+vpx+opus).
  - macOS : evermeet.cx / osxexperts.
  - Linux : John Van Sickle (statique).
- `.gitignore` les binaires (ne pas committer ~80 Mo dans git) → fetch au build.
- Mettre à jour la section « Build from source » du README (étape fetch-ffmpeg avant `tauri build`).

## 10. Licence & positionnement (points de vigilance)

- **GPL** : un FFmpeg avec `libx264` est GPLv2+. L'app reste MIT (code), mais on **distribue** un binaire GPL → obligation de joindre la licence FFmpeg (`COPYING.GPLv3`) + offrir la source correspondante (lien vers le build statique). Ajouter un `NOTICE`/`LICENSE-FFmpeg` et une mention README. Alternative LGPL (`libopenh264`) écartée : qualité/contrôle moindres.
- **Positionnement README** : la promesse **« 2 MB installer. No dependencies »** ne tient plus pour la version vidéo (~80 Mo avec ffmpeg bundlé). À mettre à jour. Décision retenue = **un seul installeur combiné**. (Alternative non retenue : deux artefacts — « lean image-only » 2 Mo vs « full » 80 Mo. À reconsidérer seulement si la taille redevient un argument commercial.)
- Incohérence existante détectée (hors périmètre, à noter) : le footer `index.html` pointe `github.com/magipa-consulting/webp-converter` alors que le README pointe `github.com/martingirardpamba/webp-converter`. À aligner lors d'un passage ultérieur.

## 11. Hors périmètre (YAGNI — explicitement exclu)

AV1, accélération matérielle (NVENC/QSV/VideoToolbox), découpe/trim, recadrage, GIF→MP4, export multi-format simultané, presets par cas d'usage avancés, conversion audio seule. Tous ajoutables ultérieurement sans remettre en cause cette architecture.

## 12. Risques & questions ouvertes

- **Perte de l'identité « 2 Mo »** : assumée par le choix bundle. Réversible (option download-at-first-use) si besoin.
- **Lenteur VP9** : encodage VP9 notablement plus lent que H.264 ; acceptable (l'utilisateur choisit), mais à communiquer dans l'UI (« WebM/VP9 = plus léger mais plus lent »).
- **Poids du repo / CI** : la récupération des binaires ffmpeg par plateforme doit être fiable (checksums) — point de fragilité du build à soigner.

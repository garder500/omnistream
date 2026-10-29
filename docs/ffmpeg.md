# FFmpeg embarqué

Omnistream relaie le flux d'OBS vers chaque plateforme avec FFmpeg (`-c copy`,
aucun ré-encodage). Le binaire est livré **dans l'application** : les
utilisateurs n'ont rien à installer.

## Résolution du binaire

Au démarrage d'un direct, Omnistream cherche FFmpeg dans cet ordre :

1. le chemin saisi par l'utilisateur dans l'écran Stream manager (s'il existe) ;
2. le sidecar livré à côté de l'exécutable (`ffmpeg` / `ffmpeg.exe`) ;
3. le `PATH` du système.

Si rien n'est trouvé, l'interface affiche un message clair avec la commande
d'installation adaptée (winget sous Windows, brew sous macOS, apt sous Linux).

## Mise en place pour les développeurs

Tauri exige que le sidecar existe avant toute compilation. `tauri dev` et
`tauri build` lancent donc automatiquement `bun scripts/fetch-ffmpeg.ts`, qui
télécharge le binaire de la cible courante dans `src-tauri/binaries/`
(`ffmpeg-<triple>[.exe]`, ignoré par git). Options :

- `bun scripts/fetch-ffmpeg.ts --target aarch64-apple-darwin` pour une autre cible ;
- `--force` pour retélécharger ;
- `FFMPEG_URL=…` pour utiliser une autre archive (ex. build maison) ;
- `SKIP_FFMPEG_FETCH=1` pour désactiver le téléchargement (CI hors ligne).

## Licences

| Plateforme | Source | Licence |
| --- | --- | --- |
| Windows, Linux | [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) (variante `lgpl`) | LGPL v2.1+ |
| macOS | [eugeneware/ffmpeg-static](https://github.com/eugeneware/ffmpeg-static) | GPL v3 (build statique) |

Omnistream ne lie pas FFmpeg : il l'exécute comme un programme séparé, ce qui
évite toute contamination de la licence de l'application. En revanche, **la
redistribution du binaire impose** de :

- conserver les mentions de licence (ce fichier est embarqué dans l'application,
  dans `licenses/ffmpeg.md`) ;
- fournir (ou indiquer où obtenir) le code source de FFmpeg correspondant :
  <https://ffmpeg.org/download.html> et les dépôts des builds ci-dessus ;
- pour le build GPL macOS, respecter les obligations de la GPL pour ce binaire.

Les builds `lgpl` n'incluent pas x264 : c'est sans importance ici, puisque seul
le mode copie est utilisé. Pour une redistribution plus simple sous macOS,
remplacer la source par un build LGPL (`FFMPEG_URL`) avant publication.

> Ce document est un résumé technique, pas un avis juridique : faites relire les
> obligations de licence avant une diffusion commerciale.

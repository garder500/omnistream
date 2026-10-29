# Omnistream

Diffusez un live sur plusieurs plateformes à la fois, regroupez tous les chats au même endroit et modérez depuis une interface unique.

## Stack

Tauri 2 · Rust · React + TypeScript · Vite · Tailwind CSS · SQLite · FFmpeg

## Structure

```
crates/omnistream-core   Logique métier sans dépendance à Tauri
  domain                 Platform, Channel, ChatMessage, ModerationAction, StreamDestination
  auth                   Trait AuthProvider (OAuth) + une impl par plateforme
  connectors             Trait ChatConnector + ConnectorRegistry (une impl par plateforme)
  moderation             ModerationEngine : dispatch vers le bon connecteur
  streaming              StreamManager : un processus FFmpeg par destination
  db                     SQLite + migrations
src-tauri                Couche Tauri : état de l'app et commandes IPC
src                      UI React : Dashboard, Multi-chat, Moderation, Stream manager, Accounts
```

Ajouter une plateforme : variante dans `Platform` (ou `Other`), un `ChatConnector` (et son module d'authentification dans `auth/`), puis l'enregistrer dans `src-tauri/src/state.rs`.

## Développement

Prérequis : [Bun](https://bun.sh) 1.2+, Rust stable, [dépendances système Tauri](https://tauri.app/start/prerequisites/), FFmpeg dans le `PATH`.

```bash
bun install
bun run tauri dev
cargo test -p omnistream-core   # tests du cœur, sans dépendances système
```

## Connexion Twitch

1. Sur [dev.twitch.tv/console](https://dev.twitch.tv/console), créez une application avec le type de client **Public** (aucun secret n'est utilisé) et copiez son Client ID.
2. Dans Omnistream, page **Comptes** : collez le Client ID, cliquez sur « Se connecter » et saisissez le code affiché sur twitch.tv/activate.
3. Ajoutez des chaînes à suivre : le chat est lu même sans compte, l'écriture et la modération nécessitent la connexion.

Le Client ID peut aussi être fourni à la compilation via la variable `OMNISTREAM_TWITCH_CLIENT_ID`.

## Release

Une release est produite par le workflow `.github/workflows/release.yml` à chaque tag `v*` : il compile les installeurs Windows, macOS (Apple Silicon et Intel) et Linux avec `tauri-action`, après avoir vérifié que le tag correspond à la version de `tauri.conf.json` et que la clé publique n'est plus le placeholder, puis crée une release GitHub en **brouillon** (à relire avant de la publier) contenant aussi `latest.json`, le manifeste lu par la mise à jour automatique.

```bash
# 1. Alignez la version dans package.json, src-tauri/Cargo.toml et src-tauri/tauri.conf.json
# 2. Créez et poussez le tag
git tag v0.2.0 && git push origin v0.2.0
```

### Mise à jour automatique (updater)

Les mises à jour sont signées. Les clés ne sont **jamais** commitées dans le dépôt.

1. Générez une paire de clés, en local : `bun run tauri signer generate -w ~/.tauri/omnistream.key`
2. Remplacez la valeur placeholder `REMPLACER_PAR_LA_CLE_PUBLIQUE_DU_UPDATER` (`plugins.updater.pubkey` dans `src-tauri/tauri.conf.json`) par le contenu de `omnistream.key.pub`. Cette clé publique peut être commitée.
3. Dans GitHub, **Settings → Secrets and variables → Actions**, ajoutez :
   - `TAURI_SIGNING_PRIVATE_KEY` : contenu de `omnistream.key` ;
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` : le mot de passe choisi à l'étape 1.

Conservez la clé privée en lieu sûr : si elle est perdue, les installations existantes ne pourront plus se mettre à jour. Tant que le placeholder n'est pas remplacé, la vérification des mises à jour échoue (sans bloquer l'application). `createUpdaterArtifacts` est activé : un `tauri build` local exige donc `TAURI_SIGNING_PRIVATE_KEY` (utilisez `bun run tauri build --no-bundle` ou `tauri dev` pour développer sans clé).

L'endpoint par défaut est `https://github.com/garder500/omnistream/releases/latest/download/latest.json` ; adaptez-le dans `tauri.conf.json` si le dépôt change de nom. Les installeurs ne sont pas signés au niveau du système (certificat Windows, notarisation Apple) : prévoyez les avertissements SmartScreen/Gatekeeper tant que ce n'est pas configuré.

### Intégration continue

`ci.yml` exécute sur chaque PR : `cargo fmt --all --check`, `cargo clippy -D warnings`, `cargo test -p omnistream-core` et `bun run build`, avec cache cargo et bun.

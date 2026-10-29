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

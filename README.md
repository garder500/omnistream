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

Ajouter une plateforme : variante dans `Platform` (ou `Other`), un `ChatConnector`, un `AuthProvider`, puis l'enregistrer dans `src-tauri/src/state.rs`.

## Développement

Prérequis : Node 20+, Rust stable, [dépendances système Tauri](https://tauri.app/start/prerequisites/), FFmpeg dans le `PATH`.

```bash
npm install
npm run tauri dev
cargo test -p omnistream-core   # tests du cœur, sans dépendances système
```

# Connecter Kick

Omnistream utilise l'API publique officielle de Kick (OAuth 2.1 avec PKCE). Aucun identifiant
n'est embarqué : vous créez votre propre application, ses identifiants restent sur votre machine.

## 1. Créer l'application Kick

1. Sur kick.com, ouvrez **Paramètres > Développeur** (l'authentification à deux facteurs doit être activée).
2. Créez une application et déclarez l'adresse de redirection `http://127.0.0.1:41259/callback`.
3. Copiez le **Client ID** et le **Client Secret**.

## 2. Se connecter dans Omnistream

Écran **Comptes**, carte Kick : collez le Client ID et le Client Secret, puis « Se connecter ».
Ajoutez ensuite votre chaîne avec son nom (slug) dans « Chaînes suivies ».

## Limites

- **Pas de lecture du chat** : l'API officielle ne le fournit que par webhooks vers une URL publique,
  inadaptés à une application de bureau. L'envoi de messages fonctionne.
- La modération (ban, timeout, levée de ban) exige l'identifiant numérique de l'utilisateur Kick.
- Le titre, la catégorie et les tags modifient la chaîne du compte connecté.

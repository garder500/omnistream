# Connecter YouTube

Omnistream lit et modère le chat YouTube avec l'API officielle YouTube Data v3. Il n'embarque
aucun identifiant : vous créez les vôtres (gratuit), ils restent sur votre machine.

## 1. Créer les identifiants Google

1. Ouvrez la [console Google Cloud](https://console.cloud.google.com/) et créez un projet.
2. **API et services > Bibliothèque** : activez « YouTube Data API v3 ».
3. **API et services > Écran de consentement OAuth** : type « Externe », renseignez un nom
   d'application et votre adresse, puis ajoutez votre compte Google dans « Utilisateurs de test ».
   Tant que l'application reste en mode « Test », Google expire le jeton de renouvellement après
   7 jours : passez-la en « Production » (sans validation, avec un avertissement « application non
   vérifiée » à l'écran d'autorisation) pour ne pas avoir à vous reconnecter chaque semaine.
4. **Identifiants > Créer des identifiants > ID client OAuth**, type **Application de bureau**.
5. Copiez l'**ID client** et le **code secret du client**.

## 2. Se connecter dans Omnistream

Écran **Comptes**, carte YouTube : collez l'ID client et le code secret, puis « Se connecter ».
Le navigateur s'ouvre ; après autorisation, Google redirige vers `http://127.0.0.1:41258/callback`
(écoutée par Omnistream le temps de la connexion).

Ajoutez ensuite une chaîne : son handle (`@garder500`) ou son identifiant (`UC…`).
Le chat se connecte quand un direct est en cours, sinon Omnistream réessaie périodiquement.

## Limites

- **Quota** : 10 000 unités par jour et par projet. Lire le chat consomme environ 3 000 unités par
  heure de direct. Chercher le direct d'une chaîne autre que la vôtre coûte 100 unités par essai
  (toutes les 5 minutes hors ligne) ; votre propre chaîne est détectée sans cette recherche.
- Seul le chat des directs en cours est lu ; l'historique déjà présent à la connexion est ignoré.
- Lever un bannissement n'est possible que pour les bans faits depuis Omnistream pendant le direct.
- Le titre et la catégorie du direct ne sont pas encore modifiables depuis Omnistream.

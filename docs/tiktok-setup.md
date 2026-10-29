# Lire le chat TikTok (expérimental)

TikTok n'a pas d'API de chat publique. Omnistream lit le flux « webcast » d'un live à partir du
seul pseudo, en s'inspirant du projet `tiktok-live-connector`. La connexion exige une URL
**signée** : Omnistream l'obtient auprès du service tiers **Euler Stream** avec votre clé d'API.

## À savoir avant d'activer

- **Non officiel** : ce n'est pas autorisé par les conditions d'utilisation de TikTok. Le protocole
  peut changer sans préavis et casser la lecture.
- **Service tiers** : Euler Stream reçoit le pseudo suivi et votre clé d'API. Les offres gratuites
  sont limitées ; certaines fonctions sont payantes.
- **Lecture seule** : pas d'envoi de messages ni de modération.
- **Non testé en conditions réelles** par les développeurs d'Omnistream (pas de clé disponible) :
  les décodages sont couverts par des tests unitaires uniquement.

## Mise en place

1. Créez un compte sur [eulerstream.com](https://www.eulerstream.com/) et générez une clé d'API.
2. Écran **Comptes**, carte TikTok : collez la clé puis « Enregistrer ». Elle est stockée
   localement, dans la base de l'application.
3. Dans « Chaînes suivies », choisissez TikTok et saisissez le pseudo (sans `@`).

Omnistream se connecte dès que le pseudo est en direct et réessaie toutes les deux minutes sinon.

## Ce qui est lu

Messages du chat (avec badges abonné / modérateur), cadeaux (le dernier message d'une série),
follows, abonnements. La fin du direct est détectée.

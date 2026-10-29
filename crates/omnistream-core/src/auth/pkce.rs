//! PKCE (RFC 7636), méthode S256.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Vérificateur PKCE (43 caractères) et son empreinte S256.
pub fn pkce_pair(random: [u8; 32]) -> (String, String) {
    let verifier = URL_SAFE_NO_PAD.encode(random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Nouvelle paire PKCE issue de l'aléa du système.
pub fn new_pkce() -> Result<(String, String)> {
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random)
        .map_err(|e| Error::Other(format!("aléa système indisponible: {e}")))?;
    Ok(pkce_pair(random))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        // Exemple de la RFC 7636, annexe B (octets du vérificateur).
        let octets = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212,
            37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
        ];
        let (verifier, challenge) = pkce_pair(octets);
        assert_eq!(verifier, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
}

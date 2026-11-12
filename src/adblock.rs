//! Reglas del bloqueador integrado. Solo afecta a endpoints publicitarios de
//! Spotify y de redes de anuncios; nunca al audio ni a la API de reproducción.

use std::path::Path;
use std::sync::OnceLock;

/// (subcadena de la URL en minusculas, motivo)
const DEFAULT_RULES: &[(&str, &str)] = &[
    // Decision de anuncios del reproductor web (varios prefijos de host).
    ("spclient.wg.spotify.com/ad-logic/", "ad-logic"),
    ("spclient.spotify.com/ad-logic/", "ad-logic"),
    // Metadatos/creativos de anuncios.
    ("spclient.wg.spotify.com/ads/", "ads"),
    ("spclient.spotify.com/ads/", "ads"),
    // Segmentacion/publicidad de Spotify.
    ("spclient.wg.spotify.com/gabo-receiver-service/", "gabo"),
    ("spclient.spotify.com/gabo-receiver-service/", "gabo"),
    // Beacons de eventos publicitarios.
    ("://adeventtracker.spotify.com/", "ad event tracker"),
    // Servidor de anuncios de Spotify y audio de anuncios.
    ("://ads-fa.spotify.com/", "spotify ads"),
    ("/mp3-ad/", "audio ad"),
    // Redes de anuncios de terceros.
    ("://pubads.g.doubleclick.net/", "doubleclick"),
    ("://securepubads.g.doubleclick.net/", "doubleclick"),
    ("://googleads.g.doubleclick.net/", "google ads"),
    ("://pagead2.googlesyndication.com/", "google ads"),
    ("://amazon-adsystem.com/", "amazon ads"),
    ("://adsystem.amazon.com/", "amazon ads"),
];

static USER_RULES: OnceLock<Vec<String>> = OnceLock::new();

/// Audio publicitario: no se bloquea con 403 porque el reproductor se queda
/// atascado esperando el anuncio. En su lugar se responde con un WAV mudo muy
/// corto, de modo que el anuncio "termine" al instante y vuelva la musica.
pub fn is_ad_audio(uri: &str) -> bool {
    contains_ignore_ascii_case(uri, "://adstudio-assets.scdn.co/")
        || contains_ignore_ascii_case(uri, "/mp3-ad/")
        || contains_ignore_ascii_case(uri, "://mp3-ad.spotify.com/")
}

/// Carga reglas extra desde un fichero de texto (una subcadena por linea).
/// Las lineas vacias y las que empiezan por `#` se ignoran.
pub fn load_user_rules(path: &Path) {
    let mut rules = Vec::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            rules.push(line.to_string());
        }
    }
    let _ = USER_RULES.set(rules);
}

/// Busca `needle` en `haystack` sin asignar memoria ni depender de mayusculas.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Devuelve el motivo de bloqueo si la URL debe bloquearse.
pub fn match_rule(uri: &str) -> Option<&'static str> {
    for (pattern, reason) in DEFAULT_RULES {
        if contains_ignore_ascii_case(uri, pattern) {
            return Some(reason);
        }
    }
    if let Some(rules) = USER_RULES.get() {
        if rules
            .iter()
            .any(|pattern| contains_ignore_ascii_case(uri, pattern))
        {
            return Some("lista personalizada");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_spotify_ads() {
        assert!(match_rule("https://spclient.wg.spotify.com/ad-logic/v1/foo").is_some());
        assert!(match_rule("https://gae2-spclient.spotify.com/ads/v3/foo").is_some());
        assert!(match_rule("https://spclient.wg.spotify.com/gabo-receiver-service/x").is_some());
        assert!(match_rule("https://adeventtracker.spotify.com/track").is_some());
        assert!(match_rule("https://pubads.g.doubleclick.net/x").is_some());
        assert!(match_rule("https://pagead2.googlesyndication.com/x").is_some());
        assert!(match_rule("https://SPCLIENT.WG.SPOTIFY.COM/AD-LOGIC/v1/x").is_some());
        assert!(match_rule("https://audio-ak.spotify.com/audio/abc/mp3-ad/xyz").is_some());
        assert!(match_rule("https://ads-fa.spotify.com/x").is_some());
        assert!(is_ad_audio(
            "https://adstudio-assets.scdn.co/mp3/636f9d63c20ac929d5b6dc03c79a65086460d661.mp3"
        ));
        assert!(is_ad_audio(
            "https://audio-ak.spotify.com/audio/abc/mp3-ad/xyz"
        ));
        assert!(is_ad_audio("https://mp3-ad.spotify.com/ad.mp3"));
        assert!(!is_ad_audio(
            "https://audio-ak.spotify.com/audio/abc/track.mp3"
        ));
        assert!(!is_ad_audio("https://i.scdn.co/image/abc"));
    }

    #[test]
    fn allows_music_and_app() {
        assert!(match_rule("https://open.spotify.com/").is_none());
        assert!(match_rule("https://spclient.wg.spotify.com/playback/v1/foo").is_none());
        assert!(match_rule("https://audio-ak-spotify-com.akamaized.net/audio/track").is_none());
        assert!(match_rule("https://audio-ak.spotify.com/audio/abc/track.mp3").is_none());
        assert!(match_rule("https://i.scdn.co/image/abc").is_none());
        assert!(match_rule("https://api.spotify.com/v1/me").is_none());
    }
}

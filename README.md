# SpotifyRust

Reproductor web de Spotify en una app nativa de Windows, escrita en Rust.
Usa el motor **WebView2 del sistema** (el mismo Chromium de Edge), no Chrome,
no la app de escritorio de Spotify y no automatiza ningún navegador.

## Qué hace

- Carga `https://open.spotify.com` en una ventana propia, sin barra de direcciones.
- Perfil persistente: la sesión de Spotify se mantiene entre arranques
  (`%LOCALAPPDATA%\SpotifyRust\webview2`).
- **Protección máxima de anuncios (por defecto)**: bloquea peticiones de
  anuncios con respuesta `403` y sustituye el audio de anuncio por un WAV mudo
  de 0,3 s, además de saltar al final del archivo de anuncio. Los anuncios no
  se oyen y terminan casi al instante. No requiere extensiones de Chrome ni
  procesos extra.
- Tema oscuro, autoplay permitido, sin menús ni atajos de navegador.
- Los popups (incluido el login OAuth) se cargan dentro de la misma ventana.
- Una sola instancia. Si ya está abierto, avisa en vez de corromper el perfil.
- Binario de ~0,5 MB: no empaqueta Chromium, reutiliza el WebView2 del sistema.

## Reglas de bloqueo

Por defecto (modo `silent`) se bloquean las peticiones de anuncios con
respuesta `403` y el audio de anuncio se sustituye por un WAV mudo de 0,3 s.
Esto es la "protección máxima": Spotify no recibe respuesta de sus servidores
de anuncios y el reproductor no oye nada.

Por defecto se bloquean:

| Patrón | Motivo |
|---|---|
| `spclient*.spotify.com/ad-logic/` | decisión de anuncios |
| `spclient*.spotify.com/ads/` | metadatos/creativos |
| `spclient*.spotify.com/gabo-receiver-service/` | segmentación publicitaria |
| `adeventtracker.spotify.com` | beacons de anuncios |
| `ads-fa.spotify.com` | anuncios de Spotify |
| `adstudio-assets.scdn.co/mp3/`, `/mp3-ad/` | audio de anuncio → se responde con un WAV mudo de 0,3 s (solo en `silent`) |
| `pubads/securepubads/googleads.doubleclick.net` | anuncios de terceros |
| `pagead2.googlesyndication.com`, `amazon-adsystem.com` | anuncios de terceros |

Puedes añadir reglas sin recompilar en:

```
%LOCALAPPDATA%\SpotifyRust\blocklist.txt
```

Una subcadena de URL por línea (sin distinguir mayúsculas). Ejemplo:

```
://megaphone.fm/
```

Las líneas vacías y las que empiezan por `#` se ignoran.

## Modos de anuncios

La variable `SPOTIFYRUST_AD_MODE` elige el comportamiento:

| Modo | Comportamiento |
|---|---|
| `silent` (por defecto) | **Protección máxima**: bloquea reglas de anuncios con 403, sustituye audio por WAV mudo de 0,3 s y salta al final del archivo. Los anuncios no se oyen. **Puede provocar saltos de canciones y audio mudo en las pistas siguientes** (Spotify detecta la manipulación). |
| `fast` | Carga el MP3 real, lo silencia y salta al final. No aplica bloqueos 403. Menos agresivo, pero aún puede causar saltos. |
| `rate` | Silencia y acelera a 16×: ~1,3 s por anuncio. Tampoco aplica 403. |
| `off` | Diagnóstico: no toca los anuncios. Se oyen normalmente. |

```powershell
# Modo por defecto (sin variable): protección máxima
.\target\release\spotifyrust.exe

# Para probar sin bloqueo (si hay saltos o audio mudo):
$env:SPOTIFYRUST_AD_MODE = "off"
.\target\release\spotifyrust.exe
```

## Requisitos

- Windows 10/11 con **WebView2 Runtime** (Windows 11 lo trae; si falta:
  <https://developer.microsoft.com/microsoft-edge/webview2/>).
- Widevine: el runtime moderno de WebView2 incluye `WidevineCdm`, necesario
  para la reproducción DRM de Spotify. Verificado con runtime 157.

## Compilar y ejecutar

```powershell
cargo build --release
.\target\release\spotifyrust.exe
```

En desarrollo: `cargo run` y `cargo test`.

Diagnóstico opcional:

- `SPOTIFYRUST_LOG=<ruta>`: escribe un log de anuncios y bloqueos.
- `SPOTIFYRUST_CDP=9222`: abre el puerto DevTools (solo depuración).

## Atajos

- `Alt+Izquierda`: volver atrás (no hay barra de navegación).
- Los enlaces `target=_blank` y los popups se abren dentro de la app.

## Verificación

Comprobado con una cuenta free real y el reproductor en marcha:

- MP3 publicitario real de `adstudio-assets.scdn.co` (20,06 s): en modo `fast`
  el elemento queda `muted` a 1 ms de `play()`, salta a 19,94 s y emite `ended`
  a los 296 ms con posición 20,062 s. Sin audio y sin acortar la duración.
- Con el elemento conectado a Web Audio (`createMediaElementSource`), `muted`
  deja la señal a 0 (RMS 0) frente a 0,05 sin silenciar.
- Una imagen normal de Spotify (`i.scdn.co`) sigue respondiendo `200 image/jpeg`.

## Limitaciones

**Advertencia importante**: bloquear anuncios en el Web Player de Spotify puede
provocar que el reproductor salte 5 o más canciones o deje el audio mudo en las
pistas siguientes (tras el anuncio). Esto es porque los servidores de Spotify
detectan que el anuncio no se reprodujo realmente y castigan la sesión. Si esto
sucede, usa `SPOTIFYRUST_AD_MODE=off` para desactivar el bloqueo.

- Si Spotify cambia el host del audio publicitario, hay que añadirlo a la regex
  `AD` de `src/main.rs` (scripts de anuncios) y a `is_ad_audio()` en
  `src/adblock.rs` (modo `silent`).
- No es la app oficial ni está afiliado a Spotify.
- El inicio de sesión con email/contraseña funciona dentro de la app. Los
  botones de Google/Facebook/Apple abren un popup OAuth que se redirige a la
  misma ventana; si un flujo concreto falla, usa email y contraseña.
- Solo Windows (WebView2).

## Estructura

```
src/main.rs     ventana, WebView2, perfil, instancia única, interceptor
src/adblock.rs  reglas y tests del bloqueador
```

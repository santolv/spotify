# SpotifyRust

Reproductor web de Spotify en una app nativa de Windows, escrita en Rust.
Usa el motor **WebView2 del sistema** (el mismo Chromium de Edge), no Chrome,
no la app de escritorio de Spotify y no automatiza ningún navegador.

## Qué hace

- Carga `https://open.spotify.com` en una ventana propia, sin barra de direcciones.
- Perfil persistente: la sesión de Spotify se mantiene entre arranques
  (`%LOCALAPPDATA%\SpotifyRust\webview2`).
- **Anuncios inaudibles sin romper la cola**: por defecto el audio real del
  anuncio se carga (con su duración auténtica), pero se silencia al instante y
  se salta al final del archivo. El reproductor recibe un `ended` en su posición
  real, así que la cola no se desajusta y no se oye nada. No requiere
  extensiones de Chrome ni procesos extra.
- Tema oscuro, autoplay permitido, sin menús ni atajos de navegador.
- Los popups (incluido el login OAuth) se cargan dentro de la misma ventana.
- Una sola instancia. Si ya está abierto, avisa en vez de corromper el perfil.
- Binario de ~0,5 MB: no empaqueta Chromium, reutiliza el WebView2 del sistema.

## Reglas de bloqueo

En el modo por defecto (`fast`) las reglas 403 **no se aplican**: bloquear la
decisión de anuncios o sus beacons provoca errores que desajustan la cola. Solo
se actúa sobre el audio. La tabla siguiente describe el modo `silent`.

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
| `fast` (por defecto) | Carga el MP3 real, lo silencia y salta al final: termina en ~0,3 s con la duración auténtica. Sin bloqueos 403. |
| `rate` | Silencia y acelera a 16×: ~1,3 s por anuncio, con la línea de tiempo avanzando. |
| `silent` | Heredado: responde al audio con un WAV mudo de 0,3 s y aplica las reglas 403. Puede desajustar la cola. |
| `off` | Diagnóstico: no toca los anuncios. |

```powershell
$env:SPOTIFYRUST_AD_MODE = "rate"
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

- Si Spotify cambia el host del audio publicitario, hay que añadirlo a la regex
  `AD` de `src/main.rs` (scripts de anuncios) y a `is_ad_audio()` en
  `src/adblock.rs` (log y modo `silent`).
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

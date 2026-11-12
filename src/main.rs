#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;

use std::path::PathBuf;

use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

const START_URL: &str = "https://open.spotify.com/";
const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36";

const BASE_JS: &str = r#"
(() => {
  // Sin barra de navegacion: Alt+Izquierda vuelve atras.
  window.addEventListener('keydown', (e) => {
    if (e.altKey && e.key === 'ArrowLeft') {
      e.preventDefault();
      history.back();
    }
  }, true);
  // Los enlaces target=_blank se abren dentro del reproductor.
  document.addEventListener('click', (e) => {
    const a = e.target && e.target.closest ? e.target.closest('a[target="_blank"]') : null;
    if (a) a.removeAttribute('target');
  }, true);
})();
"#;

/// Silencia el audio real del anuncio y salta al final del archivo. El
/// reproductor recibe un `ended` normal, con la duracion autentica del anuncio.
const AD_SEEK_JS: &str = r#"
(() => {
  const AD = /(?:adstudio-assets\.scdn\.co|\/mp3-ad\/|mp3-ad\.spotify\.com)/i;
  const isAd = (el) => AD.test(el.currentSrc || el.src || '');
  window.__srRate = 0;
  window.__srAd = (el) => {
    if (!isAd(el)) return;
    try {
      el.muted = true;
      el.volume = 0;
      // Idempotente: tras saltar, currentTime ya esta al final. Al reutilizar
      // el elemento para otro anuncio vuelve a saltar.
      if (el.readyState >= 1 && isFinite(el.duration) && el.duration > 0.4) {
        if (el.currentTime < el.duration - 0.5) {
          el.currentTime = el.duration - 0.12;
        }
      }
    } catch (_) {}
  };
})();
"#;

/// Silencia el audio real del anuncio y lo acelera a 16x: termina en ~1,5 s
/// conservando la linea de tiempo natural del reproductor.
const AD_RATE_JS: &str = r#"
(() => {
  const AD = /(?:adstudio-assets\.scdn\.co|\/mp3-ad\/|mp3-ad\.spotify\.com)/i;
  const RATE = 16;
  const isAd = (el) => AD.test(el.currentSrc || el.src || '');
  window.__srRate = RATE;
  window.__srAd = (el) => {
    if (!isAd(el)) return;
    try {
      el.muted = true;
      el.volume = 0;
      if (el.playbackRate !== RATE) el.playbackRate = RATE;
    } catch (_) {}
  };
})();
"#;

/// Red de seguridad: solo actúa sobre elementos identificados como anuncio.
/// Los silencia y, cuando terminan (ended) o se reutilizan para una canción,
/// restaura el volumen para que la música vuelva a oírse.
const AD_GUARD_JS: &str = r#"
(() => {
  const AD = /(?:adstudio-assets\.scdn\.co|\/mp3-ad\/|mp3-ad\.spotify\.com)/i;
  const isAd = (el) => AD.test(el.currentSrc || el.src || '');
  const seen = new Set();
  const release = (el) => {
    try {
      el.muted = false;
      el.volume = 1;
      el.playbackRate = 1;
      delete el.__srPrevVolume;
      delete el.__srAdSeen;
    } catch (_) {}
  };
  const arm = (el) => {
    if (!isAd(el)) {
      release(el);
      return;
    }
    if (!el.__srAdSeen) {
      el.__srAdSeen = true;
      el.__srPrevVolume = el.volume;
    }
    el.muted = true;
    el.volume = 0;
    if (window.__srRate && el.playbackRate !== window.__srRate) {
      el.playbackRate = window.__srRate;
    }
    if (el.readyState >= 1 && isFinite(el.duration) && el.duration > 0.4 && el.currentTime < el.duration - 0.5) {
      el.currentTime = el.duration - 0.12;
    }
  };
  const watch = (el) => {
    if (seen.has(el)) return;
    seen.add(el);
    el.addEventListener('play', () => arm(el));
    el.addEventListener('loadedmetadata', () => arm(el));
    el.addEventListener('timeupdate', () => arm(el));
    el.addEventListener('ended', () => release(el));
  };
  const play = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = function (...args) {
    try { watch(this); arm(this); } catch (_) {}
    return play.apply(this, args);
  };
})();
"#;

#[derive(Clone, Copy, PartialEq)]
enum AdMode {
    Silent,
    Fast,
    Rate,
    Off,
}

impl AdMode {
    fn from_env() -> Self {
        match std::env::var("SPOTIFYRUST_AD_MODE").as_deref() {
            Ok("fast") => Self::Fast,
            Ok("rate") => Self::Rate,
            Ok("off") => Self::Off,
            Ok("silent") => Self::Silent,
            _ => Self::Fast,
        }
    }
}

fn init_script(mode: AdMode) -> String {
    let mut js = String::from(BASE_JS);
    match mode {
        AdMode::Fast => {
            js.push_str(AD_SEEK_JS);
            js.push_str(AD_GUARD_JS);
        }
        AdMode::Rate => {
            js.push_str(AD_RATE_JS);
            js.push_str(AD_GUARD_JS);
        }
        _ => {}
    }
    js
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    if !platform::acquire_single_instance() {
        platform::show_error("SpotifyRust ya se esta ejecutando.");
        return Ok(());
    }

    let data_root = data_root();
    std::fs::create_dir_all(&data_root)?;
    adblock::load_user_rules(&data_root.join("blocklist.txt"));
    let ad_mode = AdMode::from_env();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Spotify")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0))
        .build(&event_loop)?;

    // Perfil persistente: la sesion de Spotify sobrevive al cerrar la app.
    let mut context = WebContext::new(Some(data_root.join("webview2")));

    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url("about:blank")
        .with_user_agent(CHROME_UA)
        .with_background_color((18, 18, 18, 255))
        .with_autoplay(true)
        .with_initialization_script(init_script(ad_mode));

    #[cfg(windows)]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        let builder = builder
            .with_theme(wry::Theme::Dark)
            .with_browser_accelerator_keys(false)
            .with_default_context_menus(false);
        // Depuracion opcional: SPOTIFYRUST_CDP=9222 abre DevTools Protocol.
        match std::env::var("SPOTIFYRUST_CDP") {
            Ok(port) if !port.is_empty() => builder.with_additional_browser_args(format!(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
                 --autoplay-policy=no-user-gesture-required \
                 --remote-debugging-port={port}"
            )),
            _ => builder,
        }
    };

    let webview = builder.build(&window)?;

    #[cfg(windows)]
    if let Err(error) = platform::install_hooks(&webview, ad_mode) {
        eprintln!("[spotifyrust] no se pudieron activar los hooks: {error}");
    }

    webview.load_url(START_URL)?;

    let mut webview = Some(webview);
    let mut window = Some(window);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let _ = webview.take();
                let _ = window.take();
            }
            Event::WindowEvent {
                event: WindowEvent::Destroyed,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

fn data_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("SpotifyRust")
}

#[cfg(windows)]
mod platform {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, ICoreWebView2NewWindowRequestedEventArgs,
        ICoreWebView2WebResourceRequestedEventArgs,
    };
    use webview2_com::{NewWindowRequestedEventHandler, WebResourceRequestedEventHandler};
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Shell::SHCreateMemStream;
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::{HSTRING, PWSTR, w};
    use wry::WebViewExtWindows;

    pub fn acquire_single_instance() -> bool {
        unsafe {
            match CreateMutexW(None, true, w!("Local\\SpotifyRust.SingleInstance")) {
                Ok(handle) => {
                    if GetLastError() == ERROR_ALREADY_EXISTS {
                        false
                    } else {
                        // El handle queda abierto hasta que termine el proceso.
                        let _ = handle;
                        true
                    }
                }
                Err(_) => true,
            }
        }
    }

    pub fn show_error(message: &str) {
        let text = HSTRING::from(message);
        unsafe {
            MessageBoxW(None, &text, w!("SpotifyRust"), MB_OK | MB_ICONERROR);
        }
    }

    /// Bloquea peticiones publicitarias y evita que los popups OAuth se pierdan:
    /// se cargan en la misma ventana en lugar de abrir un navegador externo.
    /// El handler corre en el hilo UI/STA que bombea WebView2: no moverlo.
    pub fn install_hooks(webview: &wry::WebView, mode: super::AdMode) -> windows::core::Result<()> {
        let core = webview.webview();
        let environment = webview.environment();

        unsafe {
            // Popups (login con Google/Facebook/Apple, enlaces externos): misma ventana.
            let popup_webview = core.clone();
            let popup_handler = NewWindowRequestedEventHandler::create(Box::new(
                move |_sender, args: Option<ICoreWebView2NewWindowRequestedEventArgs>| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let mut raw_uri = PWSTR::null();
                    args.Uri(&mut raw_uri)?;
                    let uri = String::from_utf16_lossy(raw_uri.as_wide());
                    CoTaskMemFree(Some(raw_uri.0 as *const _));

                    popup_webview.Navigate(&HSTRING::from(uri))?;
                    args.SetHandled(true)?;
                    Ok(())
                },
            ));
            let mut popup_token = 0i64;
            core.add_NewWindowRequested(&popup_handler, &mut popup_token)?;

            core.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;

            let handler = WebResourceRequestedEventHandler::create(Box::new(
                move |_sender, args: Option<ICoreWebView2WebResourceRequestedEventArgs>| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let request = args.Request()?;
                    let mut raw_uri = PWSTR::null();
                    request.Uri(&mut raw_uri)?;
                    let uri = String::from_utf16_lossy(raw_uri.as_wide());
                    CoTaskMemFree(Some(raw_uri.0 as *const _));

                    if mode == super::AdMode::Off {
                        return Ok(());
                    }

                    if super::adblock::is_ad_audio(&uri) {
                        match mode {
                            super::AdMode::Silent => {
                                // El anuncio se sustituye por 0,3 s de silencio:
                                // el reproductor lo da por terminado.
                                let wav = silent_wav();
                                if let Some(stream) = SHCreateMemStream(Some(wav.as_slice())) {
                                    let headers = HSTRING::from(
                                        "Content-Type: audio/wav\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *",
                                    );
                                    let reason = HSTRING::from("OK");
                                    let response = environment.CreateWebResourceResponse(
                                        Some(&stream),
                                        200,
                                        &reason,
                                        &headers,
                                    )?;
                                    args.SetResponse(&response)?;
                                    log_event("ads", &format!("silencio: {uri}"));
                                }
                                return Ok(());
                            }
                            // El audio real pasa: el script inyectado lo silencia
                            // y lo termina sin romper la cola del reproductor.
                            super::AdMode::Fast | super::AdMode::Rate => {
                                log_event("ads", &format!("audio real: {uri}"));
                                return Ok(());
                            }
                            super::AdMode::Off => {}
                        }
                    }

                    // En fast/rate solo se actua sobre el audio. Bloquear
                    // ad-logic o los beacons provoca errores de anuncio que
                    // desajustan la cola del reproductor.
                    if matches!(mode, super::AdMode::Fast | super::AdMode::Rate) {
                        return Ok(());
                    }

                    if let Some(rule) = super::adblock::match_rule(&uri) {
                        let headers = HSTRING::from("Content-Type: text/plain");
                        let reason = HSTRING::from("Blocked by SpotifyRust");
                        let response =
                            environment.CreateWebResourceResponse(None, 403, &reason, &headers)?;
                        args.SetResponse(&response)?;
                        log_event("adblock", &format!("{rule}: {uri}"));
                    }
                    Ok(())
                },
            ));

            let mut token = 0i64;
            core.add_WebResourceRequested(&handler, &mut token)?;
        }

        Ok(())
    }

    /// Log opcional a fichero (`SPOTIFYRUST_LOG`), util en la build sin consola.
    fn log_event(kind: &str, message: &str) {
        use std::io::Write;
        let Some(path) = std::env::var_os("SPOTIFYRUST_LOG") else {
            return;
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "[{kind}] {message}");
        }
    }

    /// WAV PCM 8 kHz, 8 bits, mono, 0,3 s de silencio. Sustituye al MP3 del
    /// anuncio para que el reproductor lo termine inmediatamente.
    fn silent_wav() -> Vec<u8> {
        const SAMPLE_RATE: u32 = 8000;
        const SAMPLES: u32 = SAMPLE_RATE * 3 / 10;
        let data_len = SAMPLES;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(44 + data_len as usize, 128);
        wav
    }
}

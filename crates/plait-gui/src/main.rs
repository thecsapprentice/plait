//! Plait's desktop app: plait-ui's views over the native backend, in a
//! Dioxus desktop window.

use std::{rc::Rc, sync::Arc};

use dioxus::prelude::*;
use plait_core::store::vault::{OsKeychain, Vault};
use plait_gui::{NativeBackend, native::FILE_ROUTE, settings::Paths};
use plait_ui::{App, STYLE, Ui};

/// The app icon at 256px, for the window's title bar and the taskbar.
/// Packaged builds also carry the installers' icons (assets/brand/app-icon).
const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/app-icon/png/plait-256.png");

/// A label added to the window's title, for development.
const LABEL_ENV: &str = "PLAIT_GUI_LABEL";

fn main() {
    let (dom, config, _runtime) = match setup() {
        Ok(parts) => parts,
        Err(e) => {
            eprintln!("plait-gui: {e:#}");
            std::process::exit(1);
        }
    };
    // Runs until the window closes and never returns; `_runtime` lives
    // until then.
    dioxus_desktop::launch::launch_virtual_dom(dom, config);
}

/// The window's root and settings, and the runtime plait-core runs on.
fn setup() -> anyhow::Result<(VirtualDom, dioxus_desktop::Config, tokio::runtime::Runtime)> {
    // The local time zone, read while this is the only thread: `time`
    // won't read it once others exist (Unix). UTC if it can't tell.
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    // plait-core's work runs here, off the window's thread.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("plait-core")
        .enable_all()
        .build()?;
    // No keychain (some Linux setups): stores take a passphrase instead.
    let vault = OsKeychain::new()
        .ok()
        .map(|k| Arc::new(k) as Arc<dyn Vault>);
    let paths = Paths::default_paths()?;
    let webview = paths.webview.clone();
    let backend =
        NativeBackend::new(paths, vault, runtime.handle().clone())?.with_utc_offset(offset);

    let backend = Rc::new(backend);
    let dom = VirtualDom::new(Root);
    dom.provide_root_context(Ui(backend.clone()));
    dom.provide_root_context(backend);
    // For development: tells apart windows started side by side
    // (`scripts/dev-gui.py --guis 2`).
    let title = match std::env::var(LABEL_ENV) {
        Ok(label) if !label.trim().is_empty() => format!("Plait ({})", label.trim()),
        _ => "Plait".to_owned(),
    };
    let config = dioxus_desktop::Config::new()
        .with_menu(None)
        .with_data_directory(webview)
        .with_custom_head(format!("<style>{STYLE}</style>"))
        .with_background_color((238, 241, 244, 255))
        .with_window(
            dioxus_desktop::WindowBuilder::new()
                .with_title(title)
                .with_inner_size(dioxus_desktop::LogicalSize::new(1180.0, 780.0))
                .with_min_inner_size(dioxus_desktop::LogicalSize::new(360.0, 480.0)),
        );
    // After `with_window`, which replaces the window builder the icon goes in.
    let config = match window_icon() {
        Some(icon) => config.with_icon(icon),
        None => config,
    };
    Ok((dom, config, runtime))
}

/// The window's icon, decoded from the embedded PNG. Without it the window
/// would carry Dioxus's own.
fn window_icon() -> Option<dioxus_desktop::tao::window::Icon> {
    dioxus_desktop::icon_from_memory(ICON_PNG).ok()
}

/// The platform's part of the root: spellcheck, the images notes show,
/// then plait-ui's app.
#[component]
fn Root() -> Element {
    use_hook(spellcheck);
    let backend = use_context::<Rc<NativeBackend>>();
    dioxus_desktop::use_asset_handler(FILE_ROUTE, move |request, responder| {
        responder.respond(note_image(&backend, request.uri().path()));
    });
    rsx! { App {} }
}

/// Answers the webview's request for an image on a note (3g-2): from the
/// open server's store, as the type it was sent as, never sniffed as
/// anything else, and never cached. Anything else is not found.
fn note_image(backend: &NativeBackend, path: &str) -> dioxus_desktop::wry::http::Response<Vec<u8>> {
    use dioxus_desktop::wry::http::{Response, StatusCode, header};
    let rest = path
        .trim_start_matches('/')
        .strip_prefix(FILE_ROUTE)
        .and_then(|p| p.strip_prefix('/'))
        .unwrap_or("");
    let built = match backend.image(rest) {
        Some((bytes, kind)) => Response::builder()
            .header(header::CONTENT_TYPE, kind)
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::CACHE_CONTROL, "no-store")
            .body(bytes.to_vec()),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new()),
    };
    built.unwrap_or_default()
}

/// WebKitGTK checks spelling only when asked, unlike WebView2 and
/// WKWebView (3a's findings).
#[cfg(target_os = "linux")]
fn spellcheck() {
    use dioxus_desktop::wry::WebViewExtUnix;
    use webkit2gtk::{WebContextExt, WebViewExt};
    if let Some(context) = dioxus_desktop::window().webview.webview().context() {
        context.set_spell_checking_enabled(true);
        let lang = std::env::var("LANG").unwrap_or_default();
        let lang = lang
            .split('.')
            .next()
            .filter(|l| !l.is_empty() && *l != "C" && *l != "POSIX")
            .unwrap_or("en_US");
        context.set_spell_checking_languages(&[lang]);
    }
}

#[cfg(not(target_os = "linux"))]
fn spellcheck() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_icon_is_the_app_icon_and_decodes() {
        assert!(ICON_PNG.starts_with(b"\x89PNG"));
        assert!(window_icon().is_some());
    }
}

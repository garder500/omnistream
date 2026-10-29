//! Confort d'usage : mode dock (petite fenêtre toujours au premier plan) et raccourcis globaux.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const DOCK_LABEL: &str = "dock";

/// Ouvre le dock s'il est fermé, le ferme sinon. Renvoie `true` si le dock est ouvert ensuite.
pub fn toggle_dock_window(app: &AppHandle) -> Result<bool, String> {
    if let Some(win) = app.get_webview_window(DOCK_LABEL) {
        win.close().map_err(|e| e.to_string())?;
        return Ok(false);
    }
    WebviewWindowBuilder::new(app, DOCK_LABEL, WebviewUrl::App("index.html?dock=1".into()))
        .title("Omnistream · dock")
        .inner_size(360.0, 520.0)
        .min_inner_size(260.0, 200.0)
        .always_on_top(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(true)
}

// `async` : la création de fenêtre ne doit pas se faire sur le thread principal (blocage sous Windows).
#[tauri::command]
pub async fn toggle_dock(app: AppHandle) -> Result<bool, String> {
    toggle_dock_window(&app)
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Enregistre les raccourcis globaux (échec toléré : un autre logiciel peut déjà les occuper).
/// - Ctrl/Cmd+Maj+O : ramener Omnistream au premier plan
/// - Ctrl/Cmd+Maj+K : ouvrir la palette de commandes
/// - Ctrl/Cmd+Maj+D : ouvrir / fermer le dock
pub fn register_shortcuts(app: &AppHandle) {
    let primary = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    let mods = Modifiers::SHIFT | primary;
    let open = Shortcut::new(Some(mods), Code::KeyO);
    let palette = Shortcut::new(Some(mods), Code::KeyK);
    let dock = Shortcut::new(Some(mods), Code::KeyD);

    for (sc, name) in [(open, "ouvrir"), (palette, "palette"), (dock, "dock")] {
        let res = app
            .global_shortcut()
            .on_shortcut(sc, move |app, _shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                match name {
                    "dock" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = toggle_dock_window(&app);
                        });
                    }
                    "palette" => {
                        show_main(app);
                        let _ = app.emit_to("main", "shortcut://palette", ());
                    }
                    _ => show_main(app),
                }
            });
        if let Err(e) = res {
            eprintln!("raccourci global « {name} » indisponible : {e}");
        }
    }
}

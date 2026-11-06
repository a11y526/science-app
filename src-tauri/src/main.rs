// 과학 관리 (윈도우) — 선생님 포털의 관리 화면을 앱 창으로 연다.
// 처음 켤 때 포털 주소를 받고(dist/index.html), 켤 때마다 새 버전을 확인한다.
// 포털 화면에는 앱 기능(IPC)을 열어 주지 않는다(capabilities 없음).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gh;

use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater::UpdaterExt;

// 앱 안 페이지(dist/index.html) 주소. 윈도우와 맥이 다르다.
#[cfg(windows)]
const LOCAL: &str = "http://tauri.localhost/index.html";
#[cfg(not(windows))]
const LOCAL: &str = "tauri://localhost/index.html";

fn go(app: &AppHandle, query: &str) {
    if let (Some(win), Ok(url)) = (app.get_webview_window("main"), Url::parse(&format!("{LOCAL}{query}"))) {
        let _ = win.navigate(url);
    }
}

fn say(app: &AppHandle, text: &str) {
    app.dialog().message(text).title("과학 관리").show(|_| {});
}

async fn check_update(app: AppHandle, manual: bool) {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(_) => return,
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let a = app.clone();
            app.dialog()
                .message(format!("새 버전 {}이 있어요. 지금 업데이트할까요?\n(받은 뒤 앱이 다시 켜져요)", update.version))
                .title("업데이트")
                .buttons(MessageDialogButtons::OkCancelCustom("업데이트".into(), "나중에".into()))
                .show(move |ok| {
                    if !ok {
                        return;
                    }
                    tauri::async_runtime::spawn(async move {
                        match update.download_and_install(|_, _| {}, || {}).await {
                            Ok(_) => a.restart(),
                            Err(_) => say(&a, "업데이트를 받지 못했어요. 인터넷 연결을 확인하고 메뉴의 [업데이트 확인]을 눌러 주세요."),
                        }
                    });
                });
        }
        Ok(None) => {
            if manual {
                say(&app, "지금이 가장 새 버전이에요.");
            }
        }
        Err(_) => {
            if manual {
                say(&app, "업데이트를 확인하지 못했어요. 인터넷 연결을 확인해 주세요.");
            }
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(gh::Gh::default())
        .invoke_handler(tauri::generate_handler![
            gh::gh_ready,
            gh::gh_start,
            gh::gh_poll,
            gh::gh_find,
            gh::gh_create,
            gh::gh_open
        ])
        .setup(|app| {
            let home = MenuItem::with_id(app, "home", "관리 첫 화면", true, None::<&str>)?;
            let reload = MenuItem::with_id(app, "reload", "새로고침", true, Some("F5"))?;
            let addr = MenuItem::with_id(app, "addr", "포털 주소 바꾸기", true, None::<&str>)?;
            let upd = MenuItem::with_id(app, "update", "업데이트 확인", true, None::<&str>)?;
            let ver = MenuItem::with_id(
                app,
                "ver",
                format!("버전 {}", app.package_info().version),
                false,
                None::<&str>,
            )?;
            let sub = Submenu::with_items(app, "메뉴", true, &[&home, &reload, &addr, &upd, &ver])?;
            let menu = Menu::with_items(app, &[&sub])?;

            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("과학 관리")
                .inner_size(1280.0, 820.0)
                .min_inner_size(900.0, 600.0)
                .menu(menu)
                .build()?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(check_update(handle, false));
            Ok(())
        })
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "home" => go(app, ""),
            "addr" => go(app, "?setup=1"),
            "reload" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.eval("location.reload()");
                }
            }
            "update" => {
                tauri::async_runtime::spawn(check_update(app.clone(), true));
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("과학 관리를 켜지 못했어요");
}

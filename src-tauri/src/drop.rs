//! Files dropped on the window, on Windows.
//!
//! wry's own drop handling registers on the WebView2 windows in our process, but the
//! window under the cursor belongs to WebView2's browser process, which then refuses
//! the drop. So WebView2 takes the drop itself, the page passes the dropped files to
//! us with postMessageWithAdditionalObjects, and we read their paths here.

use tauri::{Emitter, WebviewWindow};
use webview2_com::{
    take_pwstr,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs2},
    WebMessageReceivedEventHandler,
};
use windows::core::{Interface, PWSTR};

/// Message the page sends with the dropped files attached
const DROP_MESSAGE: &str = "flowbit-drop";

pub fn install(window: &WebviewWindow) {
    let app = window.clone();
    let _ = window.with_webview(move |webview| unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else {
            return;
        };
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            // Tauri's own messages aren't plain strings, or are other strings: skip them
            let mut message = PWSTR::null();
            if args.TryGetWebMessageAsString(&mut message).is_err() {
                return Ok(());
            }
            if take_pwstr(message) != DROP_MESSAGE {
                return Ok(());
            }
            let Ok(objects) = args
                .cast::<ICoreWebView2WebMessageReceivedEventArgs2>()
                .and_then(|a| a.AdditionalObjects())
            else {
                return Ok(());
            };
            let mut count = 0u32;
            objects.Count(&mut count)?;
            let mut paths = Vec::new();
            for i in 0..count {
                let Ok(file) = objects
                    .GetValueAtIndex(i)
                    .and_then(|o| o.cast::<ICoreWebView2File>())
                else {
                    continue;
                };
                let mut path = PWSTR::null();
                if file.Path(&mut path).is_ok() {
                    paths.push(take_pwstr(path));
                }
            }
            if !paths.is_empty() {
                app.emit("files-dropped", paths).ok();
            }
            Ok(())
        }));
        let mut token = 0i64;
        let _ = core.add_WebMessageReceived(&handler, &mut token);
    });
}

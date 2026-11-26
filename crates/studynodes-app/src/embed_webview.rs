#![allow(unsafe_code)]

//! Embedded HTML iframe media player backed by native WebView2 and a localhost origin host.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::OnceLock;

use studynodes_graph::model::NodeId;

use crate::media_helper::{embed_iframe_html, percent_decode};

#[derive(Clone, Debug, PartialEq)]
pub struct DesiredEmbed {
    pub node_id: NodeId,
    pub iframe_src: String,
    pub title: String,
    pub rect: [f32; 4],
    pub visible: bool,
    pub seek_nonce: u64,
}

fn is_allowed_embed_src(src: &str) -> bool {
    let lower = src.trim().to_ascii_lowercase();
    lower.starts_with("https://www.youtube.com/embed/")
        || lower.starts_with("https://www.youtube-nocookie.com/embed/")
        || lower.starts_with("https://player.vimeo.com/video/")
        || lower.starts_with("https://open.spotify.com/embed/")
}

fn handle_http_request(stream: &mut std::net::TcpStream) {
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let path_and_query = first_line.split_whitespace().nth(1).unwrap_or("/");
    let query = path_and_query.split_once('?').map(|(_, q)| q).unwrap_or("");

    let mut src = String::new();
    let mut title = String::from("Embedded Media");
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "src" => src = percent_decode(v),
                "title" => title = percent_decode(v),
                _ => {}
            }
        }
    }

    let (status, body) = if is_allowed_embed_src(&src) {
        ("200 OK", embed_iframe_html(&src, &title))
    } else {
        (
            "400 Bad Request",
            "<!doctype html><html><body style='background:#0f1115;color:#ccc;font-family:sans-serif;padding:16px'>Unsupported embed URL</body></html>".to_string(),
        )
    };

    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nReferrer-Policy: strict-origin-when-cross-origin\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

static EMBED_SERVER_PORT: OnceLock<u16> = OnceLock::new();

pub fn local_embed_url(iframe_src: &str, title: &str) -> String {
    let port = *EMBED_SERVER_PORT.get_or_init(|| {
        let Ok(listener) = TcpListener::bind(("127.0.0.1", 0)) else {
            return 0;
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        std::thread::Builder::new()
            .name("studynodes-embed-httpd".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut s = stream;
                    std::thread::spawn(move || {
                        handle_http_request(&mut s);
                    });
                }
            })
            .ok();
        port
    });

    if port == 0 {
        return iframe_src.to_string();
    }
    format!(
        "http://127.0.0.1:{}/embed?src={}&title={}",
        port,
        crate::media_helper::percent_encode(iframe_src),
        crate::media_helper::percent_encode(title)
    )
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use std::num::NonZeroIsize;
    use std::sync::Mutex;
    use wry::{
        Rect, WebContext, WebView, WebViewBuilder,
        dpi::{LogicalPosition, LogicalSize},
    };

    static PARENT_HWND: Mutex<Option<isize>> = Mutex::new(None);

    struct ParentWindowHandle(NonZeroIsize);

    impl raw_window_handle::HasWindowHandle for ParentWindowHandle {
        fn window_handle(
            &self,
        ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
            let h = raw_window_handle::Win32WindowHandle::new(self.0);
            let raw = raw_window_handle::RawWindowHandle::Win32(h);
            Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
        }
    }

    struct ActiveWebEmbed {
        webview: WebView,
        iframe_src: String,
        seek_nonce: u64,
        last_rect: [f32; 4],
        visible: bool,
    }

    thread_local! {
        static WEB_CTX: RefCell<Option<WebContext>> = const { RefCell::new(None) };
        static ACTIVE_EMBEDS: RefCell<HashMap<NodeId, ActiveWebEmbed>> = RefCell::new(HashMap::new());
    }

    pub fn attach(window: &rune::WindowHandle) {
        let _ = window.with_raw_handle(|handle| {
            if let rune::raw_window_handle::RawWindowHandle::Win32(w) = handle.as_raw() {
                if let Ok(mut guard) = PARENT_HWND.lock() {
                    *guard = Some(w.hwnd.get());
                }
            }
        });
    }

    fn make_rect(r: [f32; 4]) -> Rect {
        Rect {
            position: LogicalPosition::new(r[0].max(0.0) as f64, r[1].max(0.0) as f64).into(),
            size: LogicalSize::new(r[2].max(1.0) as f64, r[3].max(1.0) as f64).into(),
        }
    }

    pub fn has_active_embeds() -> bool {
        ACTIVE_EMBEDS.with(|views_cell| !views_cell.borrow().is_empty())
    }

    pub fn sync_embeds(desired: &[DesiredEmbed]) {
        if rune::is_headless() {
            return;
        }
        let Some(hwnd_raw) = PARENT_HWND.lock().ok().and_then(|g| *g) else {
            return;
        };
        let Some(nz) = NonZeroIsize::new(hwnd_raw) else {
            return;
        };
        let parent = ParentWindowHandle(nz);

        ACTIVE_EMBEDS.with(|views_cell| {
            let mut views = views_cell.borrow_mut();

            // Tear down removed / closed embeds immediately so audio/video stops.
            views.retain(|id, _| desired.iter().any(|d| &d.node_id == id));

            WEB_CTX.with(|ctx_cell| {
                let mut ctx_opt = ctx_cell.borrow_mut();

                for item in desired {
                    let target_url = local_embed_url(&item.iframe_src, &item.title);
                    if let Some(existing) = views.get_mut(&item.node_id) {
                        if existing.iframe_src != item.iframe_src
                            || existing.seek_nonce != item.seek_nonce
                        {
                            let _ = existing.webview.load_url(&target_url);
                            existing.iframe_src = item.iframe_src.clone();
                            existing.seek_nonce = item.seek_nonce;
                        }
                        if existing.visible != item.visible {
                            let _ = existing.webview.set_visible(item.visible);
                            existing.visible = item.visible;
                        }
                        if item.visible {
                            let moved = (existing.last_rect[0] - item.rect[0]).abs() > 0.5
                                || (existing.last_rect[1] - item.rect[1]).abs() > 0.5
                                || (existing.last_rect[2] - item.rect[2]).abs() > 0.5
                                || (existing.last_rect[3] - item.rect[3]).abs() > 0.5;
                            if moved {
                                let _ = existing.webview.set_bounds(make_rect(item.rect));
                                existing.last_rect = item.rect;
                            }
                        }
                    } else if item.visible {
                        if ctx_opt.is_none() {
                            let data_dir = std::env::temp_dir().join("StudyNodesWebView2");
                            let _ = std::fs::create_dir_all(&data_dir);
                            *ctx_opt = Some(WebContext::new(Some(data_dir)));
                        }
                        let Some(web_ctx) = ctx_opt.as_mut() else {
                            continue;
                        };
                        let builder = WebViewBuilder::new_with_web_context(web_ctx)
                            .with_url(&target_url)
                            .with_bounds(make_rect(item.rect))
                            .with_background_color((15, 17, 21, 255));
                        if let Ok(webview) = builder.build_as_child(&parent) {
                            let _ = webview.set_visible(item.visible);
                            views.insert(
                                item.node_id.clone(),
                                ActiveWebEmbed {
                                    webview,
                                    iframe_src: item.iframe_src.clone(),
                                    seek_nonce: item.seek_nonce,
                                    last_rect: item.rect,
                                    visible: item.visible,
                                },
                            );
                        }
                    }
                }
            });
        });
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;

    pub fn attach(_window: &rune::WindowHandle) {}
    pub fn has_active_embeds() -> bool {
        false
    }
    pub fn sync_embeds(_desired: &[DesiredEmbed]) {}
}

pub use imp::{attach, has_active_embeds, sync_embeds};

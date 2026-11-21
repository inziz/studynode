#![allow(unsafe_code)]

//! Native Windows Precision Touchpad 2D simultaneous Pan & Pinch via DirectManipulation.
//!
//! Without DirectManipulation, Windows synthesizes legacy `WM_MOUSEWHEEL` / `WM_MOUSEHWHEEL`
//! messages that lock each two-finger trackpad gesture to a single axis (X or Y). By attaching a
//! DirectManipulation viewport with no axis rails to the canvas window and forwarding contacts
//! only when a `PT_TOUCHPAD` pointer hits the canvas region, trackpad panning becomes genuinely
//! simultaneous across X and Y with zero-lag 2D diagonals and inertia.

#[derive(Clone, Copy, Debug)]
pub enum TouchpadEvent {
    Pan { dx: f32, dy: f32 },
    Zoom { factor: f32, x: f32, y: f32 },
}

#[cfg(target_os = "windows")]
mod imp {
    use super::TouchpadEvent;
    use std::cell::{Cell, RefCell};
    use std::sync::Mutex;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::DirectManipulation::{
        DirectManipulationManager, DIRECTMANIPULATION_CONFIGURATION_INTERACTION,
        DIRECTMANIPULATION_CONFIGURATION_SCALING,
        DIRECTMANIPULATION_CONFIGURATION_SCALING_INERTIA,
        DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_INERTIA,
        DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_X,
        DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_Y, DIRECTMANIPULATION_DISABLED,
        DIRECTMANIPULATION_INERTIA, DIRECTMANIPULATION_READY, DIRECTMANIPULATION_RUNNING,
        DIRECTMANIPULATION_STATUS, DIRECTMANIPULATION_VIEWPORT_OPTIONS_DISABLEPIXELSNAPPING,
        DIRECTMANIPULATION_VIEWPORT_OPTIONS_MANUALUPDATE, IDirectManipulationContent,
        IDirectManipulationFrameInfoProvider, IDirectManipulationManager,
        IDirectManipulationPrimaryContent, IDirectManipulationUpdateManager,
        IDirectManipulationViewport, IDirectManipulationViewportEventHandler,
        IDirectManipulationViewportEventHandler_Impl,
    };
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Input::Pointer::GetPointerType;
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        DM_POINTERHITTEST, GetCursorPos, KillTimer, POINTER_INPUT_TYPE, PT_TOUCHPAD, SetTimer,
        WM_NCDESTROY, WM_TIMER,
    };
    use windows_core::{Ref, Result as WinResult, implement};

    const SUBCLASS_ID: usize = 0x5354_4E44; // "STND"
    const TIMER_ID: usize = 0x5354_4E45;

    static EVENTS: Mutex<Vec<TouchpadEvent>> = Mutex::new(Vec::new());
    static WAKER: Mutex<Option<rune::Waker>> = Mutex::new(None);
    /// Laid-out canvas bounds `[x, y, w, h]` in logical window points.
    static CANVAS_BOUNDS: Mutex<Option<[f32; 4]>> = Mutex::new(None);

    struct DmController {
        hwnd: isize,
        manager: IDirectManipulationManager,
        update_manager: IDirectManipulationUpdateManager,
        viewport: IDirectManipulationViewport,
        cookie: u32,
    }

    thread_local! {
        static DM_STATE: RefCell<Option<DmController>> = const { RefCell::new(None) };
        static IDLE_TICKS: Cell<u32> = const { Cell::new(0) };
    }

    fn wake_app() {
        if let Ok(guard) = WAKER.lock() {
            if let Some(waker) = guard.as_ref() {
                (waker)();
            }
        }
    }

    fn push_event(ev: TouchpadEvent) {
        if let Ok(mut q) = EVENTS.lock() {
            q.push(ev);
        }
        wake_app();
    }

    fn dpi_scale_for(hwnd: HWND) -> f32 {
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi == 0 {
            1.0
        } else {
            (dpi as f32 / 96.0).max(1.0)
        }
    }

    fn client_logical_point(hwnd: HWND, screen_pt: POINT) -> (f32, f32) {
        let mut pt = screen_pt;
        unsafe {
            let _ = ScreenToClient(hwnd, &mut pt);
        }
        let scale = dpi_scale_for(hwnd);
        (pt.x as f32 / scale, pt.y as f32 / scale)
    }

    fn point_in_canvas(hwnd: HWND) -> bool {
        let mut pt = POINT::default();
        unsafe {
            if GetCursorPos(&mut pt).is_err() {
                return false;
            }
        }
        let (lx, ly) = client_logical_point(hwnd, pt);

        if let Ok(bounds) = CANVAS_BOUNDS.lock() {
            if let Some([bx, by, bw, bh]) = *bounds {
                return lx >= bx && lx <= bx + bw && ly >= by && ly <= by + bh;
            }
        }
        false
    }

    #[implement(IDirectManipulationViewportEventHandler)]
    struct ViewportHandler {
        hwnd: isize,
        active: Cell<bool>,
        resetting: Cell<bool>,
        last: Cell<Option<(f32, f32, f32)>>,
    }

    impl IDirectManipulationViewportEventHandler_Impl for ViewportHandler_Impl {
        fn OnViewportStatusChanged(
            &self,
            viewport: Ref<IDirectManipulationViewport>,
            current: DIRECTMANIPULATION_STATUS,
            previous: DIRECTMANIPULATION_STATUS,
        ) -> WinResult<()> {
            if current == DIRECTMANIPULATION_RUNNING || current == DIRECTMANIPULATION_INERTIA {
                self.active.set(true);
                if current == DIRECTMANIPULATION_RUNNING && previous != DIRECTMANIPULATION_INERTIA {
                    self.last.set(None);
                }
            } else if current == DIRECTMANIPULATION_READY || current == DIRECTMANIPULATION_DISABLED {
                self.active.set(false);
                self.last.set(None);
                if current == DIRECTMANIPULATION_READY
                    && (previous == DIRECTMANIPULATION_RUNNING
                        || previous == DIRECTMANIPULATION_INERTIA)
                {
                    if let Some(vp) = viewport.as_ref() {
                        self.resetting.set(true);
                        unsafe {
                            let _ = vp.ZoomToRect(500_000.0, 500_000.0, 501_000.0, 501_000.0, false);
                        }
                        self.last.set(None);
                        self.resetting.set(false);
                    }
                }
            }
            Ok(())
        }

        fn OnViewportUpdated(&self, _viewport: Ref<IDirectManipulationViewport>) -> WinResult<()> {
            Ok(())
        }

        fn OnContentUpdated(
            &self,
            _viewport: Ref<IDirectManipulationViewport>,
            content: Ref<IDirectManipulationContent>,
        ) -> WinResult<()> {
            if self.resetting.get() || !self.active.get() {
                return Ok(());
            }
            let Some(c) = content.as_ref() else {
                return Ok(());
            };
            let mut m = [0.0f32; 6];
            unsafe {
                if c.GetContentTransform(&mut m).is_err() {
                    return Ok(());
                }
            }
            let scale = m[0];
            let tx = m[4];
            let ty = m[5];

            let hwnd = HWND(self.hwnd as *mut core::ffi::c_void);
            let dpi_scale = dpi_scale_for(hwnd);

            if let Some((prev_scale, prev_tx, prev_ty)) = self.last.get() {
                let scale_ratio = if prev_scale > 1e-4 {
                    scale / prev_scale
                } else {
                    1.0
                };
                if scale_ratio.is_finite()
                    && (scale_ratio - 1.0).abs() > 0.0005
                    && (0.5..=2.0).contains(&scale_ratio)
                {
                    let mut pt = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut pt);
                    }
                    let (cx, cy) = client_logical_point(hwnd, pt);
                    push_event(TouchpadEvent::Zoom {
                        factor: scale_ratio,
                        x: cx,
                        y: cy,
                    });
                } else {
                    let dx = (tx - prev_tx) / dpi_scale;
                    let dy = (ty - prev_ty) / dpi_scale;
                    if dx.is_finite()
                        && dy.is_finite()
                        && dx.abs() < 400.0
                        && dy.abs() < 400.0
                        && (dx.abs() > 1e-4 || dy.abs() > 1e-4)
                    {
                        push_event(TouchpadEvent::Pan { dx, dy });
                    }
                }
            }

            self.last.set(Some((scale, tx, ty)));
            Ok(())
        }
    }

    fn init_dm(hwnd: HWND) -> WinResult<DmController> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let manager: IDirectManipulationManager =
                CoCreateInstance(&DirectManipulationManager, None, CLSCTX_INPROC_SERVER)?;
            manager.Activate(hwnd)?;
            let update_manager: IDirectManipulationUpdateManager = manager.GetUpdateManager()?;
            let viewport: IDirectManipulationViewport =
                manager.CreateViewport(Option::<&IDirectManipulationFrameInfoProvider>::None, hwnd)?;

            // Deliberately omit RAILS_X and RAILS_Y so X and Y glide freely at the same time.
            let cfg = DIRECTMANIPULATION_CONFIGURATION_INTERACTION
                | DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_X
                | DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_Y
                | DIRECTMANIPULATION_CONFIGURATION_TRANSLATION_INERTIA
                | DIRECTMANIPULATION_CONFIGURATION_SCALING
                | DIRECTMANIPULATION_CONFIGURATION_SCALING_INERTIA;
            viewport.ActivateConfiguration(cfg)?;
            viewport.SetViewportOptions(
                DIRECTMANIPULATION_VIEWPORT_OPTIONS_MANUALUPDATE
                    | DIRECTMANIPULATION_VIEWPORT_OPTIONS_DISABLEPIXELSNAPPING,
            )?;

            let vp_rect = RECT {
                left: 0,
                top: 0,
                right: 1000,
                bottom: 1000,
            };
            viewport.SetViewportRect(&vp_rect)?;

            let content: IDirectManipulationContent = viewport.GetPrimaryContent()?;
            let content_rect = RECT {
                left: 0,
                top: 0,
                right: 1_000_000,
                bottom: 1_000_000,
            };
            content.SetContentRect(&content_rect)?;

            let primary: IDirectManipulationPrimaryContent = viewport.GetPrimaryContent()?;
            primary.SetZoomBoundaries(0.1, 10.0)?;

            viewport.ZoomToRect(500_000.0, 500_000.0, 501_000.0, 501_000.0, false)?;

            let handler: IDirectManipulationViewportEventHandler = ViewportHandler {
                hwnd: hwnd.0 as isize,
                active: Cell::new(false),
                resetting: Cell::new(false),
                last: Cell::new(None),
            }
            .into();
            let cookie = viewport.AddEventHandler(Some(hwnd), &handler)?;
            viewport.Enable()?;

            Ok(DmController {
                hwnd: hwnd.0 as isize,
                manager,
                update_manager,
                viewport,
                cookie,
            })
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _uid_subclass: usize,
        _ref_data: usize,
    ) -> LRESULT {
        match msg {
            DM_POINTERHITTEST => {
                let pointer_id = (wparam.0 & 0xFFFF) as u32;
                let mut ptype = POINTER_INPUT_TYPE::default();
                let is_touchpad =
                    unsafe { GetPointerType(pointer_id, &mut ptype).is_ok() } && ptype == PT_TOUCHPAD;

                if is_touchpad && point_in_canvas(hwnd) {
                    let vp = DM_STATE.with(|cell| {
                        cell.borrow()
                            .as_ref()
                            .filter(|c| c.hwnd == hwnd.0 as isize)
                            .map(|c| c.viewport.clone())
                    });
                    if let Some(vp) = vp {
                        IDLE_TICKS.with(|t| t.set(0));
                        unsafe {
                            let _ = vp.SetContact(pointer_id);
                            SetTimer(Some(hwnd), TIMER_ID, 8, None);
                        }
                        wake_app();
                        return LRESULT(0);
                    }
                }
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_TIMER if wparam.0 == TIMER_ID => {
                let pair = DM_STATE.with(|cell| {
                    cell.borrow()
                        .as_ref()
                        .filter(|c| c.hwnd == hwnd.0 as isize)
                        .map(|c| (c.update_manager.clone(), c.viewport.clone()))
                });
                if let Some((um, vp)) = pair {
                    unsafe {
                        let _ = um.Update(Option::<&IDirectManipulationFrameInfoProvider>::None);
                        if let Ok(status) = vp.GetStatus() {
                            if status == DIRECTMANIPULATION_RUNNING
                                || status == DIRECTMANIPULATION_INERTIA
                            {
                                IDLE_TICKS.with(|t| t.set(0));
                            } else if status == DIRECTMANIPULATION_DISABLED {
                                let _ = KillTimer(Some(hwnd), TIMER_ID);
                            } else {
                                let ticks = IDLE_TICKS.with(|t| {
                                    let next = t.get().saturating_add(1);
                                    t.set(next);
                                    next
                                });
                                if ticks >= 40 {
                                    let _ = KillTimer(Some(hwnd), TIMER_ID);
                                }
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_NCDESTROY => {
                let ctrl = DM_STATE.with(|cell| {
                    let mut b = cell.borrow_mut();
                    if b.as_ref().is_some_and(|c| c.hwnd == hwnd.0 as isize) {
                        b.take()
                    } else {
                        None
                    }
                });
                unsafe {
                    let _ = KillTimer(Some(hwnd), TIMER_ID);
                    if let Some(c) = ctrl {
                        let _ = c.viewport.RemoveEventHandler(c.cookie);
                        let _ = c.viewport.Abandon();
                        let _ = c.manager.Deactivate(hwnd);
                    }
                    let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }

    pub fn set_waker(waker: rune::Waker) {
        if let Ok(mut g) = WAKER.lock() {
            *g = Some(waker);
        }
    }

    pub fn set_canvas_bounds(bounds: Option<[f32; 4]>) {
        if let Ok(mut g) = CANVAS_BOUNDS.lock() {
            *g = bounds;
        }
    }

    pub fn attach(window: &rune::WindowHandle) {
        let _ = window.with_raw_handle(|handle| {
            if let rune::raw_window_handle::RawWindowHandle::Win32(w) = handle.as_raw() {
                let hwnd_raw = w.hwnd.get() as isize;
                let already = DM_STATE.with(|c| {
                    c.borrow().as_ref().is_some_and(|ctrl| ctrl.hwnd == hwnd_raw)
                });
                if already {
                    return;
                }
                let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
                if let Ok(ctrl) = init_dm(hwnd) {
                    DM_STATE.with(|c| {
                        *c.borrow_mut() = Some(ctrl);
                    });
                    unsafe {
                        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
                    }
                }
            }
        });
    }

    pub fn pump_updates() {
        let pair = DM_STATE.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|c| (c.update_manager.clone(), c.viewport.clone()))
        });
        if let Some((um, vp)) = pair {
            unsafe {
                if let Ok(status) = vp.GetStatus() {
                    if status == DIRECTMANIPULATION_RUNNING || status == DIRECTMANIPULATION_INERTIA {
                        let _ = um.Update(Option::<&IDirectManipulationFrameInfoProvider>::None);
                    }
                }
            }
        }
    }

    pub fn take_events() -> Vec<TouchpadEvent> {
        if let Ok(mut q) = EVENTS.lock() {
            std::mem::take(&mut *q)
        } else {
            Vec::new()
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::TouchpadEvent;

    pub fn set_waker(_waker: rune::Waker) {}
    pub fn set_canvas_bounds(_bounds: Option<[f32; 4]>) {}
    pub fn attach(_window: &rune::WindowHandle) {}
    pub fn pump_updates() {}
    pub fn take_events() -> Vec<TouchpadEvent> {
        Vec::new()
    }
}

pub use imp::{attach, pump_updates, set_canvas_bounds, set_waker, take_events};

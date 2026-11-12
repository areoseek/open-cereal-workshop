//! A VST3 plug-in's own editor in a window of ours. The plug-in draws inside a window we make and own;
//! we answer its requests to resize, and on Linux lend it our frame loop for its timers and watched
//! files, which is how plug-ins there keep their drawing alive.

use std::ffi::{c_char, c_void};
use std::time::{Duration, Instant};

use super::abi3::*;

pub const IPLUGFRAME: Tuid = iid(0x367FAF01, 0xAFA94693, 0x8D4DA2A0, 0xED0882A3);
pub const IRUNLOOP: Tuid = iid(0x18C35366, 0x97764F1A, 0x9C5B8385, 0x7A871389);

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct ViewVt {
    pub base: Unknown,
    pub supported: unsafe extern "system" fn(Obj, *const c_char) -> i32,
    pub attached: unsafe extern "system" fn(Obj, *mut c_void, *const c_char) -> i32,
    pub removed: unsafe extern "system" fn(Obj) -> i32,
    pub on_wheel: unsafe extern "system" fn(Obj, f32) -> i32,
    pub on_key_down: unsafe extern "system" fn(Obj, u16, i16, i16) -> i32,
    pub on_key_up: unsafe extern "system" fn(Obj, u16, i16, i16) -> i32,
    pub get_size: unsafe extern "system" fn(Obj, *mut Rect) -> i32,
    pub on_size: unsafe extern "system" fn(Obj, *mut Rect) -> i32,
    pub on_focus: unsafe extern "system" fn(Obj, u8) -> i32,
    pub set_frame: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub can_resize: unsafe extern "system" fn(Obj) -> i32,
    pub check_size: unsafe extern "system" fn(Obj, *mut Rect) -> i32,
}

#[repr(C)]
pub struct FrameVt {
    pub base: Unknown,
    pub resize_view: unsafe extern "system" fn(Obj, Obj, *mut Rect) -> i32,
}

#[repr(C)]
pub struct RunLoopVt {
    pub base: Unknown,
    pub add_fd: unsafe extern "system" fn(Obj, Obj, i32) -> i32,
    pub remove_fd: unsafe extern "system" fn(Obj, Obj) -> i32,
    pub add_timer: unsafe extern "system" fn(Obj, Obj, u64) -> i32,
    pub remove_timer: unsafe extern "system" fn(Obj, Obj) -> i32,
}

/// The one function of a timer or watched-file handler, after the common three. A timer takes no
/// argument and ignores the one passed.
#[repr(C)]
struct CallVt {
    base: Unknown,
    call: unsafe extern "system" fn(Obj, i32),
}

unsafe fn vt<T>(o: Obj) -> &'static T {
    &**(o as *const *const T)
}

/// What the plug-in holds of us while its editor may be open: the frame it asks to resize, and the loop
/// it hands its timers to. One per plug-in, kept until the plug-in itself goes, since some keep asking
/// after their editor has closed.
#[repr(C)]
pub struct Frame {
    vt: &'static FrameVt,
    run: RunLoop,
    open: Option<Open>,
}

#[repr(C)]
struct RunLoop {
    vt: &'static RunLoopVt,
    frame: *mut Frame,
    fds: Vec<(Obj, i32)>,
    timers: Vec<(Obj, u64, Instant)>,
}

struct Open {
    view: Obj,
    win: os::Win,
    size: (i32, i32),
    resizable: bool,
}

unsafe extern "system" fn one(_: Obj) -> u32 {
    1
}

unsafe fn frame_answer(f: *mut Frame, want: *const Tuid, out: *mut Obj) -> i32 {
    if out.is_null() || want.is_null() {
        return BAD_ARG;
    }
    if *want == IPLUGFRAME || *want == FUNKNOWN {
        *out = f as Obj;
        OK
    } else if *want == IRUNLOOP && cfg!(all(unix, not(target_os = "macos"))) {
        *out = &mut (*f).run as *mut RunLoop as Obj;
        OK
    } else {
        *out = std::ptr::null_mut();
        NO_INTERFACE
    }
}

unsafe extern "system" fn frame_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    frame_answer(this as *mut Frame, want, out)
}

unsafe extern "system" fn loop_query(this: Obj, want: *const Tuid, out: *mut Obj) -> i32 {
    frame_answer((*(this as *mut RunLoop)).frame, want, out)
}

unsafe extern "system" fn frame_resize(this: Obj, view: Obj, rect: *mut Rect) -> i32 {
    let f = this as *mut Frame;
    if rect.is_null() {
        return BAD_ARG;
    }
    let r = *rect;
    let want = ((r.right - r.left).max(1), (r.bottom - r.top).max(1));
    let Some(o) = (*f).open.as_mut() else { return FALSE };
    if o.size != want {
        o.size = want;
        o.win.resize(want.0, want.1, o.resizable);
        let v = if view.is_null() { o.view } else { view };
        let mut r = Rect { left: 0, top: 0, right: want.0, bottom: want.1 };
        (vt::<ViewVt>(v).on_size)(v, &mut r);
    }
    OK
}

unsafe extern "system" fn loop_add_fd(this: Obj, handler: Obj, fd: i32) -> i32 {
    if handler.is_null() {
        return BAD_ARG;
    }
    (vt::<Unknown>(handler).keep)(handler);
    (*(this as *mut RunLoop)).fds.push((handler, fd));
    OK
}

unsafe extern "system" fn loop_remove_fd(this: Obj, handler: Obj) -> i32 {
    let l = &mut *(this as *mut RunLoop);
    let before = l.fds.len();
    l.fds.retain(|x| x.0 != handler);
    let gone = before - l.fds.len();
    for _ in 0..gone {
        (vt::<Unknown>(handler).drop)(handler);
    }
    if gone > 0 {
        OK
    } else {
        FALSE
    }
}

unsafe extern "system" fn loop_add_timer(this: Obj, handler: Obj, ms: u64) -> i32 {
    if handler.is_null() || ms == 0 {
        return BAD_ARG;
    }
    (vt::<Unknown>(handler).keep)(handler);
    (*(this as *mut RunLoop)).timers.push((handler, ms, Instant::now() + Duration::from_millis(ms)));
    OK
}

unsafe extern "system" fn loop_remove_timer(this: Obj, handler: Obj) -> i32 {
    let l = &mut *(this as *mut RunLoop);
    let before = l.timers.len();
    l.timers.retain(|x| x.0 != handler);
    let gone = before - l.timers.len();
    for _ in 0..gone {
        (vt::<Unknown>(handler).drop)(handler);
    }
    if gone > 0 {
        OK
    } else {
        FALSE
    }
}

static FRAME_VT: FrameVt = FrameVt { base: Unknown { query: frame_query, keep: one, drop: one }, resize_view: frame_resize };
static LOOP_VT: RunLoopVt =
    RunLoopVt { base: Unknown { query: loop_query, keep: one, drop: one }, add_fd: loop_add_fd, remove_fd: loop_remove_fd, add_timer: loop_add_timer, remove_timer: loop_remove_timer };

impl Frame {
    pub fn make() -> *mut Frame {
        let f = Box::into_raw(Box::new(Frame {
            vt: &FRAME_VT,
            run: RunLoop { vt: &LOOP_VT, frame: std::ptr::null_mut(), fds: Vec::new(), timers: Vec::new() },
            open: None,
        }));
        // SAFETY: just made, nobody else has it.
        unsafe { (*f).run.frame = f };
        f
    }

    /// Whether the plug-in's settings half offers an editor for the kind of window we have.
    ///
    /// # Safety
    /// Window thread, live controller.
    pub unsafe fn offered(controller: Obj) -> bool {
        let Some(kind) = os::KIND else { return false };
        if controller.is_null() {
            return false;
        }
        let view = (vt::<ControllerVt>(controller).create_view)(controller, c"editor".as_ptr());
        if view.is_null() {
            return false;
        }
        let ok = (vt::<ViewVt>(view).supported)(view, kind.as_ptr() as *const c_char) == OK;
        (vt::<Unknown>(view).drop)(view);
        ok
    }

    /// Open the editor in a new window. False when the plug-in has none or it would not go in.
    ///
    /// # Safety
    /// Window thread, live controller, `f` from `make` and not yet freed.
    pub unsafe fn show(f: *mut Frame, controller: Obj, title: &str) -> bool {
        if (*f).open.is_some() {
            return true;
        }
        let Some(kind) = os::KIND else { return false };
        if controller.is_null() {
            return false;
        }
        let view = (vt::<ControllerVt>(controller).create_view)(controller, c"editor".as_ptr());
        if view.is_null() {
            return false;
        }
        let v = vt::<ViewVt>(view);
        let kind = kind.as_ptr() as *const c_char;
        if (v.supported)(view, kind) != OK {
            (v.base.drop)(view);
            return false;
        }
        (v.set_frame)(view, f as Obj);
        let mut r = Rect::default();
        (v.get_size)(view, &mut r);
        let mut size = (r.right - r.left, r.bottom - r.top);
        if size.0 < 16 || size.1 < 16 {
            size = (640, 400);
        }
        let resizable = (v.can_resize)(view) == OK;
        let Some(win) = os::Win::open(title, size.0, size.1, resizable) else {
            (v.set_frame)(view, std::ptr::null_mut());
            (v.base.drop)(view);
            return false;
        };
        let handle = win.handle();
        // in place before the plug-in is let in: it may ask for another size while it attaches
        (*f).open = Some(Open { view, win, size, resizable });
        if (v.attached)(view, handle, kind) != OK {
            Frame::hide(f);
            return false;
        }
        // some only learn their real size once attached
        let mut r = Rect::default();
        if (v.get_size)(view, &mut r) == OK {
            let now = (r.right - r.left, r.bottom - r.top);
            if let Some(o) = (*f).open.as_mut() {
                if now.0 >= 16 && now.1 >= 16 && now != o.size {
                    o.size = now;
                    o.win.resize(now.0, now.1, o.resizable);
                }
            }
        }
        true
    }

    /// # Safety
    /// As `show`.
    pub unsafe fn hide(f: *mut Frame) {
        if let Some(o) = (*f).open.take() {
            let v = vt::<ViewVt>(o.view);
            (v.removed)(o.view);
            (v.set_frame)(o.view, std::ptr::null_mut());
            (v.base.drop)(o.view);
            drop(o.win);
        }
    }

    /// # Safety
    /// As `show`.
    pub unsafe fn is_open(f: *mut Frame) -> bool {
        (*f).open.is_some()
    }

    /// Once a frame: the window's own news, then the timers and watched files the plug-in left with us.
    ///
    /// # Safety
    /// As `show`.
    pub unsafe fn idle(f: *mut Frame) {
        if let Some(o) = (*f).open.as_mut() {
            let (closed, sized) = o.win.poll();
            if closed {
                Frame::hide(f);
            } else if let Some(now) = sized.filter(|s| *s != o.size && o.resizable) {
                let view = o.view;
                let mut r = Rect { left: 0, top: 0, right: now.0, bottom: now.1 };
                (vt::<ViewVt>(view).check_size)(view, &mut r);
                let fit = ((r.right - r.left).max(1), (r.bottom - r.top).max(1));
                o.size = fit;
                if fit != now {
                    o.win.resize(fit.0, fit.1, true);
                }
                let mut r = Rect { left: 0, top: 0, right: fit.0, bottom: fit.1 };
                (vt::<ViewVt>(view).on_size)(view, &mut r);
            }
        }
        // a handler may add or take away others while it runs, so each is looked up again before its call
        let now = Instant::now();
        let due: Vec<Obj> = (*f).run.timers.iter().filter(|t| now >= t.2).map(|t| t.0).collect();
        for h in due {
            let Some(t) = (*f).run.timers.iter_mut().find(|t| t.0 == h) else { continue };
            t.2 = now + Duration::from_millis(t.1);
            (vt::<CallVt>(h).call)(h, 0);
        }
        #[cfg(unix)]
        {
            let watched = (*f).run.fds.clone();
            for (h, fd) in watched {
                if !(*f).run.fds.iter().any(|x| x.0 == h && x.1 == fd) {
                    continue;
                }
                let mut p = super::PollFd { fd, events: 1, revents: 0 };
                if super::poll(&mut p, 1, 0) > 0 && p.revents != 0 {
                    (vt::<CallVt>(h).call)(h, fd);
                }
            }
        }
    }

    /// # Safety
    /// As `show`; `f` is gone afterwards. Call once the plug-in itself has ended.
    pub unsafe fn free(f: *mut Frame) {
        Frame::hide(f);
        drop(Box::from_raw(f));
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod os {
    use std::ffi::c_void;
    use x11rb::connection::Connection;
    use x11rb::properties::WmSizeHints;
    use x11rb::protocol::xproto::*;
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    pub const KIND: Option<&'static [u8]> = Some(b"X11EmbedWindowID\0");

    pub struct Win {
        conn: RustConnection,
        id: Window,
        delete: Atom,
    }

    impl Win {
        pub fn open(title: &str, w: i32, h: i32, resizable: bool) -> Option<Win> {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let s = conn.setup().roots.get(screen)?;
            let id = conn.generate_id().ok()?;
            let aux = CreateWindowAux::new().background_pixel(s.black_pixel).event_mask(EventMask::STRUCTURE_NOTIFY);
            conn.create_window(x11rb::COPY_DEPTH_FROM_PARENT, id, s.root, 0, 0, w as u16, h as u16, 0, WindowClass::INPUT_OUTPUT, 0, &aux).ok()?;
            let protocols = conn.intern_atom(false, b"WM_PROTOCOLS").ok()?.reply().ok()?.atom;
            let delete = conn.intern_atom(false, b"WM_DELETE_WINDOW").ok()?.reply().ok()?.atom;
            let utf8 = conn.intern_atom(false, b"UTF8_STRING").ok()?.reply().ok()?.atom;
            let net_name = conn.intern_atom(false, b"_NET_WM_NAME").ok()?.reply().ok()?.atom;
            conn.change_property32(PropMode::REPLACE, id, protocols, AtomEnum::ATOM, &[delete]).ok()?;
            conn.change_property8(PropMode::REPLACE, id, AtomEnum::WM_NAME, AtomEnum::STRING, title.as_bytes()).ok()?;
            conn.change_property8(PropMode::REPLACE, id, net_name, utf8, title.as_bytes()).ok()?;
            conn.change_property8(PropMode::REPLACE, id, AtomEnum::WM_CLASS, AtomEnum::STRING, b"open-cereal-workshop\0Open Cereal Workshop\0").ok()?;
            // our main window may live on another display system, where this one cannot be tied to it:
            // it is kept in front instead, as an editor floating over the program
            let state = conn.intern_atom(false, b"_NET_WM_STATE").ok()?.reply().ok()?.atom;
            let above = conn.intern_atom(false, b"_NET_WM_STATE_ABOVE").ok()?.reply().ok()?.atom;
            conn.change_property32(PropMode::REPLACE, id, state, AtomEnum::ATOM, &[above]).ok()?;
            let win = Win { conn, id, delete };
            win.hints(w, h, resizable);
            win.conn.map_window(id).ok()?;
            // the plug-in talks to the display on a line of its own: the window has to be there first
            win.conn.sync().ok()?;
            Some(win)
        }

        fn hints(&self, w: i32, h: i32, resizable: bool) {
            if !resizable {
                let mut hints = WmSizeHints::new();
                hints.min_size = Some((w, h));
                hints.max_size = Some((w, h));
                let _ = hints.set_normal_hints(&self.conn, self.id);
            }
        }

        pub fn handle(&self) -> *mut c_void {
            self.id as usize as *mut c_void
        }

        pub fn resize(&self, w: i32, h: i32, resizable: bool) {
            self.hints(w, h, resizable);
            let _ = self.conn.configure_window(self.id, &ConfigureWindowAux::new().width(w as u32).height(h as u32));
            let _ = self.conn.flush();
        }

        /// Whether its close button was pressed, and its size when that was reported.
        pub fn poll(&mut self) -> (bool, Option<(i32, i32)>) {
            let (mut closed, mut size) = (false, None);
            let _ = self.conn.flush();
            loop {
                match self.conn.poll_for_event() {
                    Ok(Some(Event::ClientMessage(m))) => {
                        if m.format == 32 && m.data.as_data32()[0] == self.delete {
                            closed = true;
                        }
                    }
                    Ok(Some(Event::ConfigureNotify(c))) => {
                        if c.window == self.id {
                            size = Some((c.width as i32, c.height as i32));
                        }
                    }
                    Ok(Some(Event::DestroyNotify(d))) => {
                        if d.window == self.id {
                            closed = true;
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(_) => {
                        closed = true;
                        break;
                    }
                }
            }
            (closed, size)
        }
    }

    impl Drop for Win {
        fn drop(&mut self) {
            let _ = self.conn.destroy_window(self.id);
            let _ = self.conn.flush();
        }
    }
}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

    pub const KIND: Option<&'static [u8]> = Some(b"HWND\0");

    type Hwnd = *mut c_void;

    #[repr(C)]
    struct WndClass {
        style: u32,
        wnd_proc: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
        cls_extra: i32,
        wnd_extra: i32,
        instance: *mut c_void,
        icon: *mut c_void,
        cursor: *mut c_void,
        background: *mut c_void,
        menu_name: *const u16,
        class_name: *const u16,
    }

    #[repr(C)]
    #[derive(Default)]
    struct WinRect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(class: *const WndClass) -> u16;
        fn CreateWindowExW(ex: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: Hwnd, menu: *mut c_void, instance: *mut c_void, param: *mut c_void) -> Hwnd;
        fn DefWindowProcW(w: Hwnd, msg: u32, wp: usize, lp: isize) -> isize;
        fn DestroyWindow(w: Hwnd) -> i32;
        fn ShowWindow(w: Hwnd, how: i32) -> i32;
        fn AdjustWindowRectEx(rect: *mut WinRect, style: u32, menu: i32, ex: u32) -> i32;
        fn SetWindowPos(w: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        fn GetClientRect(w: Hwnd, rect: *mut WinRect) -> i32;
        fn SetWindowLongPtrW(w: Hwnd, index: i32, value: isize) -> isize;
        fn GetWindowLongPtrW(w: Hwnd, index: i32) -> isize;
        fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    }

    const WM_CLOSE: u32 = 0x10;
    const USERDATA: i32 = -21;
    const CAPTION: u32 = 0x00C0_0000;
    const SYSMENU: u32 = 0x0008_0000;
    const MINBOX: u32 = 0x0002_0000;
    const THICK: u32 = 0x0004_0000;
    const MAXBOX: u32 = 0x0001_0000;
    const CLIP_CHILDREN: u32 = 0x0200_0000;
    const CW_DEFAULT: i32 = 0x8000_0000u32 as i32;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn style(resizable: bool) -> u32 {
        CAPTION | SYSMENU | MINBOX | CLIP_CHILDREN | if resizable { THICK | MAXBOX } else { 0 }
    }

    /// Our program's own loop hands this window its messages; all it adds is to note the close button.
    unsafe extern "system" fn proc(w: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
        if msg == WM_CLOSE {
            let flag = GetWindowLongPtrW(w, USERDATA) as *const AtomicBool;
            if !flag.is_null() {
                (*flag).store(true, Relaxed);
            }
            return 0;
        }
        DefWindowProcW(w, msg, wp, lp)
    }

    pub struct Win {
        id: Hwnd,
        closed: Box<AtomicBool>,
    }

    impl Win {
        pub fn open(title: &str, w: i32, h: i32, resizable: bool) -> Option<Win> {
            static CLASS: std::sync::Once = std::sync::Once::new();
            let class = wide("OpenCerealWorkshopPlugView");
            // SAFETY: the documented window calls, on the window thread, with strings that outlive them.
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                CLASS.call_once(|| {
                    let c = WndClass {
                        style: 0,
                        wnd_proc: proc,
                        cls_extra: 0,
                        wnd_extra: 0,
                        instance,
                        icon: std::ptr::null_mut(),
                        cursor: LoadCursorW(std::ptr::null_mut(), 32512 as *const u16),
                        background: std::ptr::null_mut(),
                        menu_name: std::ptr::null(),
                        class_name: class.as_ptr(),
                    };
                    RegisterClassW(&c);
                });
                let st = style(resizable);
                let mut r = WinRect { left: 0, top: 0, right: w, bottom: h };
                AdjustWindowRectEx(&mut r, st, 0, 0);
                let name = wide(title);
                let id = CreateWindowExW(0, class.as_ptr(), name.as_ptr(), st, CW_DEFAULT, CW_DEFAULT, r.right - r.left, r.bottom - r.top, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null_mut());
                if id.is_null() {
                    return None;
                }
                let closed = Box::new(AtomicBool::new(false));
                SetWindowLongPtrW(id, USERDATA, &*closed as *const AtomicBool as isize);
                ShowWindow(id, 5);
                Some(Win { id, closed })
            }
        }

        pub fn handle(&self) -> *mut c_void {
            self.id
        }

        pub fn resize(&self, w: i32, h: i32, resizable: bool) {
            // SAFETY: as in `open`.
            unsafe {
                let mut r = WinRect { left: 0, top: 0, right: w, bottom: h };
                AdjustWindowRectEx(&mut r, style(resizable), 0, 0);
                SetWindowPos(self.id, std::ptr::null_mut(), 0, 0, r.right - r.left, r.bottom - r.top, 0x0002 | 0x0004 | 0x0010);
            }
        }

        pub fn poll(&mut self) -> (bool, Option<(i32, i32)>) {
            let mut r = WinRect::default();
            // SAFETY: as in `open`.
            let ok = unsafe { GetClientRect(self.id, &mut r) } != 0;
            (self.closed.load(Relaxed), (ok && r.right > 0 && r.bottom > 0).then_some((r.right, r.bottom)))
        }
    }

    impl Drop for Win {
        fn drop(&mut self) {
            // SAFETY: as in `open`; the flag outlives the window it is tied to.
            unsafe {
                SetWindowLongPtrW(self.id, USERDATA, 0);
                DestroyWindow(self.id);
            }
        }
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
mod os {
    use std::ffi::c_void;

    pub const KIND: Option<&'static [u8]> = None;

    pub struct Win;

    impl Win {
        pub fn open(_title: &str, _w: i32, _h: i32, _resizable: bool) -> Option<Win> {
            None
        }
        pub fn handle(&self) -> *mut c_void {
            std::ptr::null_mut()
        }
        pub fn resize(&self, _w: i32, _h: i32, _resizable: bool) {}
        pub fn poll(&mut self) -> (bool, Option<(i32, i32)>) {
            (true, None)
        }
    }
}

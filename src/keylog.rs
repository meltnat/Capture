use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8,
    VK_F9, VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
    VK_NEXT, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

const DISPLAY_DURATION: Duration = Duration::from_secs(2);

pub struct KeyEntry {
    pub name: String,
    pub pressed_at: Instant,
}

pub struct KeyLog {
    entries: VecDeque<KeyEntry>,
}

impl KeyLog {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, name: String) {
        let now = Instant::now();
        self.entries.retain(|e| now.duration_since(e.pressed_at) < DISPLAY_DURATION);
        self.entries.push_back(KeyEntry { name, pressed_at: now });
        // 最大10キーまで保持
        while self.entries.len() > 10 {
            self.entries.pop_front();
        }
    }

    /// 表示期限内のキー名一覧を返す
    pub fn current_keys(&self) -> Vec<&str> {
        let now = Instant::now();
        self.entries
            .iter()
            .filter(|e| now.duration_since(e.pressed_at) < DISPLAY_DURATION)
            .map(|e| e.name.as_str())
            .collect()
    }
}

fn vk_to_name(vk: VIRTUAL_KEY, scancode: u32) -> String {
    match vk {
        VK_RETURN => "Enter".to_string(),
        VK_BACK => "BS".to_string(),
        VK_TAB => "Tab".to_string(),
        VK_ESCAPE => "Esc".to_string(),
        VK_SPACE => "Space".to_string(),
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => "Shift".to_string(),
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => "Ctrl".to_string(),
        VK_MENU | VK_LMENU | VK_RMENU => "Alt".to_string(),
        VK_CAPITAL => "Caps".to_string(),
        VK_LWIN | VK_RWIN => "Win".to_string(),
        VK_DELETE => "Del".to_string(),
        VK_INSERT => "Ins".to_string(),
        VK_HOME => "Home".to_string(),
        VK_END => "End".to_string(),
        VK_PRIOR => "PgUp".to_string(),
        VK_NEXT => "PgDn".to_string(),
        VK_UP => "↑".to_string(),
        VK_DOWN => "↓".to_string(),
        VK_LEFT => "←".to_string(),
        VK_RIGHT => "→".to_string(),
        VK_F1 => "F1".to_string(),
        VK_F2 => "F2".to_string(),
        VK_F3 => "F3".to_string(),
        VK_F4 => "F4".to_string(),
        VK_F5 => "F5".to_string(),
        VK_F6 => "F6".to_string(),
        VK_F7 => "F7".to_string(),
        VK_F8 => "F8".to_string(),
        VK_F9 => "F9".to_string(),
        VK_F10 => "F10".to_string(),
        VK_F11 => "F11".to_string(),
        VK_F12 => "F12".to_string(),
        _ => {
            // GetKeyNameTextW でキー名を取得
            let lparam = ((scancode & 0x1FF) << 16) as i32;
            let mut buf = [0u16; 32];
            let len = unsafe { GetKeyNameTextW(lparam, &mut buf) };
            if len > 0 {
                String::from_utf16_lossy(&buf[..len as usize])
            } else {
                format!("#{}", vk.0)
            }
        }
    }
}

// フック用のグローバル共有状態
static HOOK_LOG: std::sync::OnceLock<Arc<Mutex<KeyLog>>> = std::sync::OnceLock::new();

unsafe extern "system" fn keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let msg = w_param.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
            let vk = VIRTUAL_KEY(kb.vkCode as u16);
            let name = vk_to_name(vk, kb.scanCode);
            if let Some(log) = HOOK_LOG.get() {
                if let Ok(mut guard) = log.lock() {
                    guard.push(name);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

pub struct KeyLogger {
    pub log: Arc<Mutex<KeyLog>>,
    thread: Option<std::thread::JoinHandle<()>>,
    thread_id: Arc<Mutex<Option<u32>>>,
}

impl KeyLogger {
    pub fn new() -> Self {
        let log = Arc::new(Mutex::new(KeyLog::new()));
        HOOK_LOG.set(Arc::clone(&log)).ok();

        let thread_id = Arc::new(Mutex::new(None::<u32>));
        let thread_id_clone = Arc::clone(&thread_id);

        let thread = std::thread::spawn(move || {
            // スレッド ID を記録
            let tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
            *thread_id_clone.lock().unwrap() = Some(tid);

            let hook = unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
                    .expect("Failed to set keyboard hook")
            };

            // メッセージループ（フックが動作するために必要）
            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                UnhookWindowsHookEx(hook).ok();
            }
        });

        Self {
            log,
            thread: Some(thread),
            thread_id,
        }
    }
}

impl Drop for KeyLogger {
    fn drop(&mut self) {
        // WM_QUIT をフックスレッドに送って GetMessage ループを終了させる
        if let Ok(guard) = self.thread_id.lock() {
            if let Some(tid) = *guard {
                unsafe { PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0)).ok() };
            }
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}


use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, RECT};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use crate::dialogs::Dialogs;
use crate::editor::EditorState;

const CLASS_NAME: &str = "AetherEditor";
const WINDOW_TITLE: &str = "Aether";

/// 设置 DPI 感知模式
fn set_dpi_awareness() {
    unsafe {
        // 尝试设置 Per-Monitor V2 DPI 感知（Windows 10 1607+）
        use windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
        use windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
        use windows::Win32::UI::HiDpi::{SetProcessDpiAwareness, PROCESS_PER_MONITOR_DPI_AWARE};

        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            // V2 失败时回退到 Per-Monitor DPI 感知（Windows 8.1+）
            let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
        }
    }
}

/// 获取 DPI 缩放比例和缩放后的窗口大小
fn get_dpi_scaled_size(base_width: i32, base_height: i32) -> (f32, i32, i32) {
    unsafe {
        use windows::Win32::UI::HiDpi::GetDpiForSystem;

        let dpi = GetDpiForSystem();
        let scale = dpi as f32 / 96.0;
        let scaled_width = (base_width as f32 * scale) as i32;
        let scaled_height = (base_height as f32 * scale) as i32;
        (scale, scaled_width, scaled_height)
    }
}

thread_local! {
    static EDITOR_STATE: RefCell<Option<Rc<RefCell<EditorState>>>> = RefCell::new(None);
}

pub fn run() {
    unsafe {
        // 设置 DPI 感知模式（Per-Monitor V2）
        set_dpi_awareness();

        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();

        let class_name: Vec<u16> = CLASS_NAME.encode_utf16().chain(Some(0)).collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(std::ptr::null_mut()), // NULL_BRUSH，避免系统绘制默认白色背景
            ..Default::default()
        };

        RegisterClassW(&wc);

        // 获取主显示器 DPI 并计算缩放后的窗口大小
        let (_dpi_scale, scaled_width, scaled_height) = get_dpi_scaled_size(1280, 800);

        let title: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(Some(0)).collect();
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW, // 显示在任务栏
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            WS_POPUP | WS_VISIBLE | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scaled_width,
            scaled_height,
            None,
            None,
            instance,
            None,
        ).unwrap();

        let state = EditorState::new(hwnd).unwrap();
        let state_rc = Rc::new(RefCell::new(state));
        EDITOR_STATE.with(|s| {
            *s.borrow_mut() = Some(state_rc.clone());
        });

        // 获取窗口实际 DPI 并计算缩放因子
        {
            use windows::Win32::UI::HiDpi::GetDpiForWindow;
            let dpi = GetDpiForWindow(hwnd);
            let scale = dpi as f32 / 96.0;
            state_rc.borrow_mut().dpi_scale = scale;
        }

        // 获取实际客户区物理像素尺寸，resize 内部会转换为逻辑像素
        let mut client_rect = RECT::default();
        if GetClientRect(hwnd, &mut client_rect).is_ok() {
            let w = (client_rect.right - client_rect.left) as u32;
            let h = (client_rect.bottom - client_rect.top) as u32;
            if w > 0 && h > 0 {
                state_rc.borrow_mut().resize(w, h);
            }
        }

        state_rc.borrow_mut().init_render_target().unwrap();
        state_rc.borrow_mut().render();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_LBUTTONDOWN => {
                let raw_x = (lparam.0 & 0xFFFF) as i16 as f32;
                let raw_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        let mut st = state.borrow_mut();
                        // 将物理像素转换为逻辑像素(DIP)
                        let mouse_x = raw_x / st.dpi_scale;
                        let mouse_y = raw_y / st.dpi_scale;
                        let layout = st.layout.clone();

                        // 0. 检测标题栏区域点击（包含菜单项和窗口控制按钮）
                        let titlebar_region = layout.title_bar_region();
                        if titlebar_region.contains(mouse_x, mouse_y) {
                            let btn_width = 46.0;
                            let close_x = titlebar_region.x + titlebar_region.width - btn_width;
                            let maximize_x = close_x - btn_width;
                            let minimize_x = maximize_x - btn_width;
                            
                            // 先检测是否点击了窗口控制按钮区域
                            if mouse_x >= minimize_x {
                                if mouse_x >= close_x {
                                    // 关闭窗口
                                    drop(st);
                                    let _ = DestroyWindow(hwnd);
                                    return;
                                } else if mouse_x >= maximize_x {
                                    // 最大化/还原
                                    let is_max = st.is_maximized;
                                    drop(st);
                                    if is_max {
                                        let _ = ShowWindow(hwnd, SW_RESTORE);
                                    } else {
                                        let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                                    }
                                    return;
                                } else {
                                    // 最小化
                                    drop(st);
                                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                                    return;
                                }
                            }
                            
                            // 检测是否点击了菜单项
                            if let Some(idx) = st.menu_bar.hit_test(mouse_x, mouse_y - titlebar_region.y, titlebar_region.height) {
                                let was_active = st.menu_bar.active_index == Some(idx);
                                st.menu_bar.close_all();
                                if !was_active {
                                    st.menu_bar.expand(idx);
                                }
                                drop(st);
                                state.borrow_mut().render();
                                return;
                            }
                            
                            // 标题栏拖动开始（点击了标题栏但非按钮/菜单区域）
                            st.menu_bar.close_all();
                            drop(st);
                            let _ = ReleaseCapture();
                            let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
                            return;
                        }

                        // 1. 检测子菜单点击（子菜单在标题栏下方弹出）
                        if let Some(active_idx) = st.menu_bar.active_index {
                            if let Some(&submenu_x) = st.menu_bar.item_x_positions.get(active_idx) {
                                let submenu_y = titlebar_region.y + titlebar_region.height;
                                if let Some(sub_idx) = st.menu_bar.hit_test_submenu(active_idx, mouse_x, mouse_y, submenu_x, submenu_y) {
                                    if let Some(item) = st.menu_bar.items.get(active_idx) {
                                        if let Some(menu_item) = item.items.get(sub_idx) {
                                            if menu_item.enabled && menu_item.command_id != crate::menu_bar::CommandId::None {
                                                let cmd = menu_item.command_id;
                                                st.menu_bar.close_all();
                                                drop(st);
                                                state.borrow_mut().execute_command(cmd, hwnd);
                                                state.borrow_mut().render();
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            st.menu_bar.close_all();
                            drop(st);
                            state.borrow_mut().render();
                            return;
                        }

                        // 2. 检测活动栏点击
                        let activity_region = layout.activity_bar_region();
                        if activity_region.contains(mouse_x, mouse_y) {
                            if let Some(idx) = st.activity_bar.hit_test(mouse_x, mouse_y, activity_region.y) {
                                let view = st.activity_bar.items[idx].view;
                                st.activity_bar.switch_to(idx);
                                st.activity_view = view;
                                st.sidebar_content = crate::layout::SidebarContent::from_view(view);
                                drop(st);
                                state.borrow_mut().render();
                                return;
                            }
                        }

                        // 3. 检测侧边栏点击
                        let sidebar_region = layout.sidebar_region();
                        if sidebar_region.contains(mouse_x, mouse_y) {
                            if st.handle_sidebar_click(mouse_x - sidebar_region.x, mouse_y - sidebar_region.y) {
                                drop(st);
                                state.borrow_mut().render();
                                return;
                            }
                        }

                        // 4. 检测标签栏点击
                        let has_multiple_tabs = st.tab_count() > 1;
                        let tab_region = layout.tab_bar_region(has_multiple_tabs);
                        if tab_region.contains(mouse_x, mouse_y) {
                            if st.handle_tab_bar_click(mouse_x, mouse_y, tab_region.x) {
                                drop(st);
                                state.borrow_mut().render();
                                return;
                            }
                        }

                        // 5. 欢迎页/编辑器区域点击
                        let welcome_x = if layout.activity_bar_visible { layout.activity_bar_width } else { 0.0 };
                        let welcome_width = st.window_width as f32 - welcome_x;
                        let welcome_y = layout.menu_bar_height;
                        let welcome_height = st.window_height as f32 - welcome_y
                            - if layout.status_bar_visible { layout.status_bar_height } else { 0.0 };
                        let welcome_region = crate::layout::Region::new(welcome_x, welcome_y, welcome_width, welcome_height);

                        if welcome_region.contains(mouse_x, mouse_y) {
                            if st.show_welcome() {
                                let action = st.handle_welcome_click(mouse_x, mouse_y, welcome_x, welcome_y, welcome_width, welcome_height);
                                if let Some(action) = action {
                                    drop(st);
                                    match action {
                                        crate::welcome::WelcomeAction::OpenFolder => {
                                            if let Some(path) = Dialogs::open_folder_dialog(hwnd, "打开文件夹") {
                                                state.borrow_mut().open_folder(path);
                                                state.borrow_mut().render();
                                            }
                                        }
                                        crate::welcome::WelcomeAction::NewFile => {
                                            state.borrow_mut().new_file();
                                            state.borrow_mut().render();
                                        }
                                        crate::welcome::WelcomeAction::CloneRepo => {
                                            state.borrow_mut().status_message = "克隆仓库功能即将推出".to_string();
                                            state.borrow_mut().render();
                                        }
                                        crate::welcome::WelcomeAction::OpenRemote => {
                                            state.borrow_mut().status_message = "SSH 连接功能即将推出".to_string();
                                            state.borrow_mut().render();
                                        }
                                        crate::welcome::WelcomeAction::OpenRecentProject(path_str) => {
                                            let path = PathBuf::from(path_str);
                                            state.borrow_mut().open_folder(path);
                                            state.borrow_mut().render();
                                        }
                                    }
                                    return;
                                }
                            } else {
                                let editor_content = layout.editor_content_region(has_multiple_tabs);
                                st.set_cursor_from_mouse(mouse_x, mouse_y, editor_content.x, editor_content.y);
                                st.clear_selection();
                                st.start_selection();
                                drop(st);
                                state.borrow_mut().render();
                                return;
                            }
                        }

                        // 6. 状态栏点击
                        let _status_region = layout.status_bar_region();
                    }
                });
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let raw_x = (lparam.0 & 0xFFFF) as i16 as f32;
                let raw_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                let is_dragging = wparam.0 & 0x0001 != 0; // MK_LBUTTON

                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        let mut st = state.borrow_mut();
                        // 将物理像素转换为逻辑像素(DIP)
                        let mouse_x = raw_x / st.dpi_scale;
                        let mouse_y = raw_y / st.dpi_scale;
                        let layout = st.layout.clone();

                        // 更新标题栏区域悬停（包含菜单项和窗口控制按钮）
                        let old_titlebar_hover = st.titlebar_hover_button;
                        let titlebar_region = layout.title_bar_region();
                        if titlebar_region.contains(mouse_x, mouse_y) {
                            let btn_width = 46.0;
                            let close_x = titlebar_region.x + titlebar_region.width - btn_width;
                            let maximize_x = close_x - btn_width;
                            let minimize_x = maximize_x - btn_width;
                            
                            // 检测窗口控制按钮悬停
                            if mouse_x >= minimize_x {
                                if mouse_x >= close_x {
                                    st.titlebar_hover_button = Some(2);
                                } else if mouse_x >= maximize_x {
                                    st.titlebar_hover_button = Some(1);
                                } else {
                                    st.titlebar_hover_button = Some(0);
                                }
                            } else {
                                st.titlebar_hover_button = None;
                            }
                        } else {
                            st.titlebar_hover_button = None;
                        }
                        let new_titlebar_hover = st.titlebar_hover_button;

                        // 更新菜单栏悬停（菜单项现在在标题栏内）
                        let old_menu_hover = st.menu_bar.hover_index;
                        if titlebar_region.contains(mouse_x, mouse_y) {
                            let btn_width = 46.0;
                            let minimize_x = titlebar_region.x + titlebar_region.width - btn_width * 3.0;
                            // 只有在非按钮区域才检测菜单悬停
                            if mouse_x < minimize_x {
                                st.menu_bar.hover_index = st.menu_bar.hit_test(mouse_x, mouse_y - titlebar_region.y, titlebar_region.height);
                            } else {
                                st.menu_bar.hover_index = None;
                            }
                        } else {
                            st.menu_bar.hover_index = None;
                        }
                        let new_menu_hover = st.menu_bar.hover_index;

                        // 更新活动栏悬停
                        let activity_region = layout.activity_bar_region();
                        st.activity_bar.hover_index = st.activity_bar.hit_test(mouse_x, mouse_y, activity_region.y);

                        // 更新标签栏悬停状态
                        let editor_content = layout.editor_content_region(st.tab_count() > 1);
                        let old_hover = st.hover_tab;
                        st.update_hover_tab(mouse_x, mouse_y, editor_content.x);
                        let new_hover = st.hover_tab;

                        // 更新文件树悬停状态
                        let sidebar_region = layout.sidebar_region();
                        let _old_tree_hover = st.hover_file_node;
                        let tree_hover_changed = if sidebar_region.contains(mouse_x, mouse_y) {
                            st.update_file_tree_hover(mouse_x - sidebar_region.x, mouse_y - sidebar_region.y)
                        } else {
                            let old = st.hover_file_node.take();
                            old.is_some()
                        };

                        if old_menu_hover != new_menu_hover || old_hover != new_hover || old_titlebar_hover != new_titlebar_hover || tree_hover_changed {
                            drop(st);
                            state.borrow_mut().render();
                        } else if is_dragging {
                            st.set_cursor_from_mouse(mouse_x, mouse_y, editor_content.x, editor_content.y);
                            st.update_selection();
                            drop(st);
                            state.borrow_mut().render();
                        }
                    }
                });
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        state.borrow_mut().end_selection();
                    }
                });
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_SIZE => {
                let mut client_rect = RECT::default();
                if GetClientRect(hwnd, &mut client_rect).is_ok() {
                    let width = (client_rect.right - client_rect.left) as u32;
                    let height = (client_rect.bottom - client_rect.top) as u32;
                    let is_max = wparam.0 == SIZE_MAXIMIZED as usize;
                    let is_min = wparam.0 == SIZE_MINIMIZED as usize;
                    EDITOR_STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            let mut st = state.borrow_mut();
                            st.is_maximized = is_max;
                            if !is_min {
                                st.resize(width, height);
                            }
                            drop(st);
                            if !is_min {
                                state.borrow_mut().render();
                            }
                        }
                    });
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let new_dpi = (wparam.0 & 0xFFFF) as f32;
                let new_scale = new_dpi / 96.0;

                if lparam.0 != 0 {
                    let suggested_rect: *const RECT = lparam.0 as *const RECT;
                    let rect = &*suggested_rect;
                    let _ = SetWindowPos(
                        hwnd, None,
                        rect.left, rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }

                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        let mut st = state.borrow_mut();
                        st.dpi_scale = new_scale;
                        if let Some(rt) = &mut st.render_target {
                            rt.set_dpi(new_dpi);
                        }
                        st.status_message = format!("DPI: {} ({}%)", new_dpi as u32, (new_scale * 100.0) as u32);
                        drop(st);
                        state.borrow_mut().render();
                    }
                });
                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                // 移除系统非客户区边框，避免白色边框线
                // 返回 0 表示客户区覆盖整个窗口，不绘制系统边框
                LRESULT(0)
            }
            WM_NCHITTEST => {
                // 自定义命中测试，实现无边框窗口的调整大小和拖动
                let x = ((lparam.0 & 0xFFFF) as i16) as i32;
                let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as i32;
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    let border_size = 8; // 边框调整大小区域
                    let left = x - rect.left;
                    let top = y - rect.top;
                    let right = rect.right - x;
                    let bottom = rect.bottom - y;

                    let mut result = HTCLIENT;
                    if top < border_size {
                        if left < border_size { result = HTTOPLEFT; }
                        else if right < border_size { result = HTTOPRIGHT; }
                        else { result = HTTOP; }
                    } else if bottom < border_size {
                        if left < border_size { result = HTBOTTOMLEFT; }
                        else if right < border_size { result = HTBOTTOMRIGHT; }
                        else { result = HTBOTTOM; }
                    } else if left < border_size {
                        result = HTLEFT;
                    } else if right < border_size {
                        result = HTRIGHT;
                    } else {
                        // 标题栏区域全部返回 HTCLIENT，由 WM_LBUTTONDOWN 统一处理菜单/按钮点击和拖动
                        // 不返回 HTCAPTION/HTCLOSE 等系统码，因为 WS_POPUP 窗口系统不会正确处理它们
                    }
                    return LRESULT(result as isize);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_ERASEBKGND => {
                // 阻止系统擦除背景，避免白色闪烁
                LRESULT(1)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let _hdc = BeginPaint(hwnd, &mut ps);
                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        state.borrow_mut().render();
                    }
                });
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CHAR => {
                let ch = (wparam.0 & 0xFFFF) as u16;
                if ch >= 32 && ch != 127 {
                    if let Some(c) = char::from_u32(ch as u32) {
                        // 命令面板激活时，输入字符进入搜索框
                        let command_palette_active = EDITOR_STATE.with(|s| {
                            s.borrow().as_ref().map(|state| state.borrow().command_palette.visible).unwrap_or(false)
                        });
                        // 终端面板激活时，输入字符进入终端
                        let terminal_active = EDITOR_STATE.with(|s| {
                            s.borrow().as_ref().map(|state| {
                                state.borrow().sidebar_content == crate::layout::SidebarContent::TerminalPanel
                            }).unwrap_or(false)
                        });
                        if command_palette_active {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().command_palette.append_query(c);
                                    state.borrow_mut().render();
                                }
                            });
                        } else if terminal_active {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().terminal_panel.input_line.push(c);
                                    state.borrow_mut().terminal_panel.cursor_pos += 1;
                                    state.borrow_mut().render();
                                }
                            });
                        } else {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().insert_char(c);
                                    state.borrow_mut().render();
                                }
                            });
                        }
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let vk = VIRTUAL_KEY(wparam.0 as u16);
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;

                // 命令面板激活时优先处理键盘导航
                let command_palette_active = EDITOR_STATE.with(|s| {
                    s.borrow().as_ref().map(|state| state.borrow().command_palette.visible).unwrap_or(false)
                });

                if command_palette_active {
                    match vk {
                        VK_ESCAPE => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().command_palette.hide();
                                    state.borrow_mut().render();
                                }
                            });
                            return LRESULT(0);
                        }
                        VK_RETURN => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    if let Some(cmd) = state.borrow().command_palette.selected_command() {
                                        let hwnd = state.borrow().hwnd;
                                        state.borrow_mut().execute_command(cmd, hwnd);
                                    }
                                    state.borrow_mut().command_palette.hide();
                                    state.borrow_mut().render();
                                }
                            });
                            return LRESULT(0);
                        }
                        VK_UP => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().command_palette.select_prev();
                                    state.borrow_mut().render();
                                }
                            });
                            return LRESULT(0);
                        }
                        VK_DOWN => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().command_palette.select_next();
                                    state.borrow_mut().render();
                                }
                            });
                            return LRESULT(0);
                        }
                        VK_BACK => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().command_palette.backspace_query();
                                    state.borrow_mut().render();
                                }
                            });
                            return LRESULT(0);
                        }
                        _ => {}
                    }
                }

                if ctrl {
                    match vk {
                        VK_O => {
                            if let Some(path) = Dialogs::open_file_dialog(hwnd, "打开文件", &[]) {
                                EDITOR_STATE.with(|s| {
                                    if let Some(state) = s.borrow().as_ref() {
                                        state.borrow_mut().load_file(path);
                                        state.borrow_mut().render();
                                    }
                                });
                            }
                        }
                        VK_K => {
                            if let Some(path) = Dialogs::open_folder_dialog(hwnd, "打开文件夹") {
                                EDITOR_STATE.with(|s| {
                                    if let Some(state) = s.borrow().as_ref() {
                                        state.borrow_mut().open_folder(path);
                                        state.borrow_mut().render();
                                    }
                                });
                            }
                        }
                        VK_S => {
                            if shift {
                                if let Some(path) = Dialogs::save_file_dialog(hwnd, "另存为", "untitled.txt") {
                                    EDITOR_STATE.with(|s| {
                                        if let Some(state) = s.borrow().as_ref() {
                                            state.borrow_mut().save_as(path);
                                            state.borrow_mut().render();
                                        }
                                    });
                                }
                            } else {
                                let need_dialog = EDITOR_STATE.with(|s| {
                                    s.borrow().as_ref().map(|state| state.borrow().file_path.is_none()).unwrap_or(true)
                                });
                                if need_dialog {
                                    if let Some(path) = Dialogs::save_file_dialog(hwnd, "保存文件", "untitled.txt") {
                                        EDITOR_STATE.with(|s| {
                                            if let Some(state) = s.borrow().as_ref() {
                                                state.borrow_mut().save_as(path);
                                                state.borrow_mut().render();
                                            }
                                        });
                                    }
                                } else {
                                    EDITOR_STATE.with(|s| {
                                        if let Some(state) = s.borrow().as_ref() {
                                            state.borrow_mut().save_file();
                                            state.borrow_mut().render();
                                        }
                                    });
                                }
                            }
                        }
                        VK_N => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().new_file();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_B => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().layout.toggle_sidebar();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_P => {
                            if shift {
                                EDITOR_STATE.with(|s| {
                                    if let Some(state) = s.borrow().as_ref() {
                                        state.borrow_mut().command_palette.toggle();
                                        state.borrow_mut().render();
                                    }
                                });
                            }
                        }
                        VK_G => {
                            if shift {
                                EDITOR_STATE.with(|s| {
                                    if let Some(state) = s.borrow().as_ref() {
                                        state.borrow_mut().command_palette.show();
                                        state.borrow_mut().command_palette.update_query(">");
                                        state.borrow_mut().render();
                                    }
                                });
                            } else {
                                EDITOR_STATE.with(|s| {
                                    if let Some(state) = s.borrow().as_ref() {
                                        state.borrow_mut().command_palette.show();
                                        state.borrow_mut().command_palette.update_query(":");
                                        state.borrow_mut().render();
                                    }
                                });
                            }
                        }
                        VK_C => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().copy();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_X => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().cut();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_V => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().paste();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_A => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().select_all();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_Z => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    if shift {
                                        state.borrow_mut().redo();
                                    } else {
                                        state.borrow_mut().undo();
                                    }
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_Y => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().redo();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_TAB => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    if shift {
                                        state.borrow_mut().prev_tab();
                                    } else {
                                        state.borrow_mut().next_tab();
                                    }
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_W | VK_F4 => {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    state.borrow_mut().close_current_tab();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                        VK_1 | VK_NUMPAD1 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(1); state.borrow_mut().render(); } }); }
                        VK_2 | VK_NUMPAD2 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(2); state.borrow_mut().render(); } }); }
                        VK_3 | VK_NUMPAD3 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(3); state.borrow_mut().render(); } }); }
                        VK_4 | VK_NUMPAD4 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(4); state.borrow_mut().render(); } }); }
                        VK_5 | VK_NUMPAD5 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(5); state.borrow_mut().render(); } }); }
                        VK_6 | VK_NUMPAD6 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(6); state.borrow_mut().render(); } }); }
                        VK_7 | VK_NUMPAD7 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(7); state.borrow_mut().render(); } }); }
                        VK_8 | VK_NUMPAD8 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { state.borrow_mut().goto_tab(8); state.borrow_mut().render(); } }); }
                        VK_9 | VK_NUMPAD9 => { EDITOR_STATE.with(|s| { if let Some(state) = s.borrow().as_ref() { let last = state.borrow().tab_count(); state.borrow_mut().goto_tab(last); state.borrow_mut().render(); } }); }
                        _ => {}
                    }
                    return LRESULT(0);
                }

                // 非Ctrl按键
                let terminal_active = EDITOR_STATE.with(|s| {
                    s.borrow().as_ref().map(|state| {
                        state.borrow().sidebar_content == crate::layout::SidebarContent::TerminalPanel
                    }).unwrap_or(false)
                });
                let has_selection = |st: &EditorState| {
                    st.selection_start.is_some() && st.selection_end.is_some()
                };
                match vk {
                    VK_RETURN => {
                        if terminal_active {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    let input = state.borrow().terminal_panel.input_line.clone();
                                    state.borrow_mut().terminal_panel.push_output(&format!("> {}", input));
                                    state.borrow_mut().terminal_panel.send_enter();
                                    state.borrow_mut().render();
                                }
                            });
                        } else {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    let has_sel = has_selection(&state.borrow());
                                    if has_sel { state.borrow_mut().delete_selection(); }
                                    state.borrow_mut().insert_newline();
                                    state.borrow_mut().render();
                                }
                            });
                        }
                    }
                    VK_BACK => {
                        if terminal_active {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    let mut st = state.borrow_mut();
                                    if !st.terminal_panel.input_line.is_empty() {
                                        st.terminal_panel.input_line.pop();
                                        st.terminal_panel.cursor_pos = st.terminal_panel.cursor_pos.saturating_sub(1);
                                    }
                                    st.render();
                                }
                            });
                        } else {
                            EDITOR_STATE.with(|s| {
                                if let Some(state) = s.borrow().as_ref() {
                                    let has_sel = has_selection(&state.borrow());
                                    if has_sel {
                                        state.borrow_mut().delete_selection();
                                    } else {
                                        state.borrow_mut().delete_char();
                                    }
                                    state.borrow_mut().render();
                                }
                            });
                        }
                    }
                    VK_DELETE => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let has_sel = has_selection(&state.borrow());
                                if has_sel {
                                    state.borrow_mut().delete_selection();
                                } else {
                                    state.borrow_mut().delete_forward();
                                }
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_LEFT => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let mut st = state.borrow_mut();
                                if shift {
                                    if st.selection_start.is_none() { st.start_selection(); }
                                    st.move_cursor_left();
                                    st.update_selection();
                                } else {
                                    if st.selection_start.is_some() { st.clear_selection(); }
                                    st.move_cursor_left();
                                }
                                drop(st);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_RIGHT => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let mut st = state.borrow_mut();
                                if shift {
                                    if st.selection_start.is_none() { st.start_selection(); }
                                    st.move_cursor_right();
                                    st.update_selection();
                                } else {
                                    if st.selection_start.is_some() { st.clear_selection(); }
                                    st.move_cursor_right();
                                }
                                drop(st);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_UP => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let mut st = state.borrow_mut();
                                if shift {
                                    if st.selection_start.is_none() { st.start_selection(); }
                                    st.move_cursor_up();
                                    st.update_selection();
                                } else {
                                    if st.selection_start.is_some() { st.clear_selection(); }
                                    st.move_cursor_up();
                                }
                                drop(st);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_DOWN => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let mut st = state.borrow_mut();
                                if shift {
                                    if st.selection_start.is_none() { st.start_selection(); }
                                    st.move_cursor_down();
                                    st.update_selection();
                                } else {
                                    if st.selection_start.is_some() { st.clear_selection(); }
                                    st.move_cursor_down();
                                }
                                drop(st);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_HOME => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let mut st = state.borrow_mut();
                                if shift {
                                    if st.selection_start.is_none() { st.start_selection(); }
                                    st.move_cursor_home();
                                    st.update_selection();
                                } else {
                                    if st.selection_start.is_some() { st.clear_selection(); }
                                    st.move_cursor_home();
                                }
                                drop(st);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_END => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let mut st = state.borrow_mut();
                                if shift {
                                    if st.selection_start.is_none() { st.start_selection(); }
                                    st.move_cursor_end();
                                    st.update_selection();
                                } else {
                                    if st.selection_start.is_some() { st.clear_selection(); }
                                    st.move_cursor_end();
                                }
                                drop(st);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_PRIOR => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let page = state.borrow().window_height as f32 - 24.0;
                                state.borrow_mut().scroll(-page);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_NEXT => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let page = state.borrow().window_height as f32 - 24.0;
                                state.borrow_mut().scroll(page);
                                state.borrow_mut().render();
                            }
                        });
                    }
                    VK_TAB => {
                        EDITOR_STATE.with(|s| {
                            if let Some(state) = s.borrow().as_ref() {
                                let has_sel = has_selection(&state.borrow());
                                if has_sel { state.borrow_mut().delete_selection(); }
                                state.borrow_mut().insert_char('\t');
                                state.borrow_mut().render();
                            }
                        });
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f32;
                // 提取光标屏幕坐标
                let cursor_x = (lparam.0 & 0xFFFF) as i16 as f32;
                let cursor_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        let mut state = state.borrow_mut();
                        // 检查光标是否在侧边栏区域内
                        let sidebar = state.layout.sidebar_region();
                        if state.layout.sidebar_visible
                            && cursor_x >= sidebar.x
                            && cursor_x < sidebar.x + sidebar.width
                            && cursor_y >= sidebar.y
                            && cursor_y < sidebar.y + sidebar.height
                        {
                            state.scroll_sidebar(-delta);
                        } else {
                            state.scroll(-delta);
                        }
                        state.render();
                    }
                });
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

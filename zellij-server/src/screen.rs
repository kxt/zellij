//! Things related to [`Screen`]s.

use std::collections::BTreeMap;
use std::os::unix::io::RawFd;
use std::str;
use std::sync::{Arc, RwLock};

use zellij_utils::zellij_tile;

use crate::{
    panes::PaneId,
    pty::{PtyInstruction, VteBytes},
    tab::Tab,
    thread_bus::Bus,
    ui::layout::Layout,
    wasm_vm::PluginInstruction,
    ServerInstruction, SessionState,
};
use zellij_tile::data::{Event, InputMode, ModeInfo, Palette, PluginCapabilities, TabInfo};
use zellij_utils::{
    errors::{ContextType, ScreenContext},
    input::options::Options,
    ipc::ClientAttributes,
    pane_size::PositionAndSize,
};

/// Instructions that can be sent to the [`Screen`].
#[derive(Debug, Clone)]
pub(crate) enum ScreenInstruction {
    PtyBytes(RawFd, VteBytes),
    Render,
    NewPane(PaneId),
    HorizontalSplit(PaneId),
    VerticalSplit(PaneId),
    WriteCharacter(Vec<u8>),
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    SwitchFocus,
    FocusNextPane,
    FocusPreviousPane,
    MoveFocusLeft,
    MoveFocusLeftOrPreviousTab,
    MoveFocusDown,
    MoveFocusUp,
    MoveFocusRight,
    MoveFocusRightOrNextTab,
    Exit,
    ScrollUp,
    ScrollDown,
    PageScrollUp,
    PageScrollDown,
    ClearScroll,
    CloseFocusedPane,
    ToggleActiveTerminalFullscreen,
    SetSelectable(PaneId, bool),
    SetMaxHeight(PaneId, usize),
    SetInvisibleBorders(PaneId, bool),
    ClosePane(PaneId),
    ApplyLayout(Layout, Vec<RawFd>),
    NewTab(RawFd),
    SwitchTabNext,
    SwitchTabPrev,
    ToggleActiveSyncTab,
    CloseTab,
    GoToTab(u32),
    UpdateTabName(Vec<u8>),
    TerminalResize(PositionAndSize),
    ChangeMode(ModeInfo),
}

impl From<&ScreenInstruction> for ScreenContext {
    fn from(screen_instruction: &ScreenInstruction) -> Self {
        match *screen_instruction {
            ScreenInstruction::PtyBytes(..) => ScreenContext::HandlePtyBytes,
            ScreenInstruction::Render => ScreenContext::Render,
            ScreenInstruction::NewPane(_) => ScreenContext::NewPane,
            ScreenInstruction::HorizontalSplit(_) => ScreenContext::HorizontalSplit,
            ScreenInstruction::VerticalSplit(_) => ScreenContext::VerticalSplit,
            ScreenInstruction::WriteCharacter(_) => ScreenContext::WriteCharacter,
            ScreenInstruction::ResizeLeft => ScreenContext::ResizeLeft,
            ScreenInstruction::ResizeRight => ScreenContext::ResizeRight,
            ScreenInstruction::ResizeDown => ScreenContext::ResizeDown,
            ScreenInstruction::ResizeUp => ScreenContext::ResizeUp,
            ScreenInstruction::SwitchFocus => ScreenContext::SwitchFocus,
            ScreenInstruction::FocusNextPane => ScreenContext::FocusNextPane,
            ScreenInstruction::FocusPreviousPane => ScreenContext::FocusPreviousPane,
            ScreenInstruction::MoveFocusLeft => ScreenContext::MoveFocusLeft,
            ScreenInstruction::MoveFocusLeftOrPreviousTab => {
                ScreenContext::MoveFocusLeftOrPreviousTab
            }
            ScreenInstruction::MoveFocusDown => ScreenContext::MoveFocusDown,
            ScreenInstruction::MoveFocusUp => ScreenContext::MoveFocusUp,
            ScreenInstruction::MoveFocusRight => ScreenContext::MoveFocusRight,
            ScreenInstruction::MoveFocusRightOrNextTab => ScreenContext::MoveFocusRightOrNextTab,
            ScreenInstruction::Exit => ScreenContext::Exit,
            ScreenInstruction::ScrollUp => ScreenContext::ScrollUp,
            ScreenInstruction::ScrollDown => ScreenContext::ScrollDown,
            ScreenInstruction::PageScrollUp => ScreenContext::PageScrollUp,
            ScreenInstruction::PageScrollDown => ScreenContext::PageScrollDown,
            ScreenInstruction::ClearScroll => ScreenContext::ClearScroll,
            ScreenInstruction::CloseFocusedPane => ScreenContext::CloseFocusedPane,
            ScreenInstruction::ToggleActiveTerminalFullscreen => {
                ScreenContext::ToggleActiveTerminalFullscreen
            }
            ScreenInstruction::SetSelectable(..) => ScreenContext::SetSelectable,
            ScreenInstruction::SetInvisibleBorders(..) => ScreenContext::SetInvisibleBorders,
            ScreenInstruction::SetMaxHeight(..) => ScreenContext::SetMaxHeight,
            ScreenInstruction::ClosePane(_) => ScreenContext::ClosePane,
            ScreenInstruction::ApplyLayout(..) => ScreenContext::ApplyLayout,
            ScreenInstruction::NewTab(_) => ScreenContext::NewTab,
            ScreenInstruction::SwitchTabNext => ScreenContext::SwitchTabNext,
            ScreenInstruction::SwitchTabPrev => ScreenContext::SwitchTabPrev,
            ScreenInstruction::CloseTab => ScreenContext::CloseTab,
            ScreenInstruction::GoToTab(_) => ScreenContext::GoToTab,
            ScreenInstruction::UpdateTabName(_) => ScreenContext::UpdateTabName,
            ScreenInstruction::TerminalResize(_) => ScreenContext::TerminalResize,
            ScreenInstruction::ChangeMode(_) => ScreenContext::ChangeMode,
            ScreenInstruction::ToggleActiveSyncTab => ScreenContext::ToggleActiveSyncTab,
        }
    }
}

/// A [`Screen`] holds multiple [`Tab`]s, each one holding multiple [`panes`](crate::client::panes).
/// It only directly controls which tab is active, delegating the rest to the individual `Tab`.
pub(crate) struct Screen {
    /// A Bus for sending and receiving messages with the other threads.
    pub bus: Bus<ScreenInstruction>,
    /// An optional maximal amount of panes allowed per [`Tab`] in this [`Screen`] instance.
    max_panes: Option<usize>,
    /// A map between this [`Screen`]'s tabs and their ID/key.
    tabs: BTreeMap<usize, Tab>,
    /// The full size of this [`Screen`].
    position_and_size: PositionAndSize,
    /// The index of this [`Screen`]'s active [`Tab`].
    active_tab_index: Option<usize>,
    mode_info: ModeInfo,
    input_mode: InputMode,
    colors: Palette,
    session_state: Arc<RwLock<SessionState>>,
}

impl Screen {
    /// Creates and returns a new [`Screen`].
    pub fn new(
        bus: Bus<ScreenInstruction>,
        client_attributes: &ClientAttributes,
        max_panes: Option<usize>,
        mode_info: ModeInfo,
        input_mode: InputMode,
        session_state: Arc<RwLock<SessionState>>,
    ) -> Self {
        Screen {
            bus,
            max_panes,
            position_and_size: client_attributes.position_and_size,
            colors: client_attributes.palette,
            active_tab_index: None,
            tabs: BTreeMap::new(),
            mode_info,
            input_mode,
            session_state,
        }
    }

    /// Creates a new [`Tab`] in this [`Screen`], containing a single
    /// [pane](crate::client::panes) with PTY file descriptor `pane_id`.
    pub fn new_tab(&mut self, pane_id: RawFd) {
        let tab_index = self.get_new_tab_index();
        let position = self.tabs.len();
        let tab = Tab::new(
            tab_index,
            position,
            String::new(),
            &self.position_and_size,
            self.bus.os_input.as_ref().unwrap().clone(),
            self.bus.senders.clone(),
            self.max_panes,
            Some(PaneId::Terminal(pane_id)),
            self.mode_info.clone(),
            self.input_mode,
            self.colors,
            self.session_state.clone(),
        );
        self.active_tab_index = Some(tab_index);
        self.tabs.insert(tab_index, tab);
        self.update_tabs();
        self.render();
    }

    /// Returns the index where a new [`Tab`] should be created in this [`Screen`].
    /// Currently, this is right after the last currently existing tab, or `0` if
    /// no tabs exist in this screen yet.
    fn get_new_tab_index(&self) -> usize {
        if let Some(index) = self.tabs.keys().last() {
            *index + 1
        } else {
            0
        }
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the next tab.
    pub fn switch_tab_next(&mut self) {
        let active_tab_pos = self.get_active_tab().unwrap().position;
        let new_tab_pos = (active_tab_pos + 1) % self.tabs.len();

        for tab in self.tabs.values_mut() {
            if tab.position == new_tab_pos {
                tab.set_force_render();
                self.active_tab_index = Some(tab.index);
                break;
            }
        }
        self.update_tabs();
        self.render();
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the previous tab.
    pub fn switch_tab_prev(&mut self) {
        let active_tab_pos = self.get_active_tab().unwrap().position;
        let new_tab_pos = if active_tab_pos == 0 {
            self.tabs.len() - 1
        } else {
            active_tab_pos - 1
        };
        for tab in self.tabs.values_mut() {
            if tab.position == new_tab_pos {
                tab.set_force_render();
                self.active_tab_index = Some(tab.index);
                break;
            }
        }
        self.update_tabs();
        self.render();
    }

    pub fn go_to_tab(&mut self, mut tab_index: usize) {
        tab_index -= 1;
        let active_tab_index = self.get_active_tab().unwrap().index;
        if let Some(t) = self.tabs.values_mut().find(|t| t.position == tab_index) {
            if t.index != active_tab_index {
                t.set_force_render();
                self.active_tab_index = Some(t.index);
                self.update_tabs();
                self.render();
            }
        }
    }

    /// Closes this [`Screen`]'s active [`Tab`], exiting the application if it happens
    /// to be the last tab.
    pub fn close_tab(&mut self) {
        let active_tab_index = self.active_tab_index.unwrap();
        if self.tabs.len() > 1 {
            self.switch_tab_prev();
        }
        let active_tab = self.tabs.remove(&active_tab_index).unwrap();
        let pane_ids = active_tab.get_pane_ids();
        // below we don't check the result of sending the CloseTab instruction to the pty thread
        // because this might be happening when the app is closing, at which point the pty thread
        // has already closed and this would result in an error
        self.bus
            .senders
            .send_to_pty(PtyInstruction::CloseTab(pane_ids))
            .unwrap();
        if self.tabs.is_empty() {
            self.active_tab_index = None;
            if *self.session_state.read().unwrap() == SessionState::Attached {
                self.bus
                    .senders
                    .send_to_server(ServerInstruction::Render(None))
                    .unwrap();
            }
        } else {
            for t in self.tabs.values_mut() {
                if t.position > active_tab.position {
                    t.position -= 1;
                }
            }
            self.update_tabs();
        }
    }

    pub fn resize_to_screen(&mut self, new_screen_size: PositionAndSize) {
        self.position_and_size = new_screen_size;
        for (_, tab) in self.tabs.iter_mut() {
            tab.resize_whole_tab(new_screen_size);
        }
        let _ = self.get_active_tab_mut().map(|t| t.set_force_render());
        self.render();
    }

    /// Renders this [`Screen`], which amounts to rendering its active [`Tab`].
    pub fn render(&mut self) {
        if *self.session_state.read().unwrap() != SessionState::Attached {
            return;
        }
        if let Some(active_tab) = self.get_active_tab_mut() {
            if active_tab.get_active_pane().is_some() {
                active_tab.render();
            } else {
                self.close_tab();
            }
        };
    }

    /// Returns a mutable reference to this [`Screen`]'s tabs.
    pub fn get_tabs_mut(&mut self) -> &mut BTreeMap<usize, Tab> {
        &mut self.tabs
    }

    /// Returns an immutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab(&self) -> Option<&Tab> {
        match self.active_tab_index {
            Some(tab) => self.tabs.get(&tab),
            None => None,
        }
    }

    /// Returns a mutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab_mut(&mut self) -> Option<&mut Tab> {
        match self.active_tab_index {
            Some(tab) => self.get_tabs_mut().get_mut(&tab),
            None => None,
        }
    }

    /// Creates a new [`Tab`] in this [`Screen`], applying the specified [`Layout`]
    /// and switching to it.
    pub fn apply_layout(&mut self, layout: Layout, new_pids: Vec<RawFd>) {
        let tab_index = self.get_new_tab_index();
        let position = self.tabs.len();
        let mut tab = Tab::new(
            tab_index,
            position,
            String::new(),
            &self.position_and_size,
            self.bus.os_input.as_ref().unwrap().clone(),
            self.bus.senders.clone(),
            self.max_panes,
            None,
            self.mode_info.clone(),
            self.input_mode,
            self.colors,
            self.session_state.clone(),
        );
        tab.apply_layout(layout, new_pids);
        self.active_tab_index = Some(tab_index);
        self.tabs.insert(tab_index, tab);
        self.update_tabs();
    }

    pub fn update_tabs(&self) {
        let mut tab_data = vec![];
        let active_tab_index = self.active_tab_index.unwrap();
        for tab in self.tabs.values() {
            tab_data.push(TabInfo {
                position: tab.position,
                name: tab.name.clone(),
                active: active_tab_index == tab.index,
                is_sync_panes_active: tab.is_sync_panes_active(),
            });
        }
        self.bus
            .senders
            .send_to_plugin(PluginInstruction::Update(None, Event::TabUpdate(tab_data)))
            .unwrap();
    }

    pub fn update_active_tab_name(&mut self, buf: Vec<u8>) {
        let s = str::from_utf8(&buf).unwrap();
        let active_tab = self.get_active_tab_mut().unwrap();
        match s {
            "\0" => {
                active_tab.name = String::new();
            }
            "\u{007F}" | "\u{0008}" => {
                //delete and backspace keys
                active_tab.name.pop();
            }
            c => {
                active_tab.name.push_str(c);
            }
        }
        self.update_tabs();
    }
    pub fn change_mode(&mut self, mode_info: ModeInfo) {
        self.colors = mode_info.palette;
        self.mode_info = mode_info;
        for tab in self.tabs.values_mut() {
            tab.mode_info = self.mode_info.clone();
        }
    }
}

// The box is here in order to make the
// NewClient enum smaller
#[allow(clippy::boxed_local)]
pub(crate) fn screen_thread_main(
    bus: Bus<ScreenInstruction>,
    max_panes: Option<usize>,
    client_attributes: ClientAttributes,
    config_options: Box<Options>,
    session_state: Arc<RwLock<SessionState>>,
) {
    let capabilities = config_options.simplified_ui;

    let mut screen = Screen::new(
        bus,
        &client_attributes,
        max_panes,
        ModeInfo {
            palette: client_attributes.palette,
            capabilities: PluginCapabilities {
                arrow_fonts: capabilities,
            },
            ..ModeInfo::default()
        },
        InputMode::Normal,
        session_state,
    );
    loop {
        let (event, mut err_ctx) = screen
            .bus
            .recv()
            .expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Screen((&event).into()));
        match event {
            ScreenInstruction::PtyBytes(pid, vte_bytes) => {
                let active_tab = screen.get_active_tab_mut().unwrap();
                if active_tab.has_terminal_pid(pid) {
                    // it's most likely that this event is directed at the active tab
                    // look there first
                    active_tab.handle_pty_bytes(pid, vte_bytes);
                } else {
                    // if this event wasn't directed at the active tab, start looking
                    // in other tabs
                    let all_tabs = screen.get_tabs_mut();
                    for tab in all_tabs.values_mut() {
                        if tab.has_terminal_pid(pid) {
                            tab.handle_pty_bytes(pid, vte_bytes);
                            break;
                        }
                    }
                }
            }
            ScreenInstruction::Render => {
                screen.render();
            }
            ScreenInstruction::NewPane(pid) => {
                screen.get_active_tab_mut().unwrap().new_pane(pid);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::HorizontalSplit(pid) => {
                screen.get_active_tab_mut().unwrap().horizontal_split(pid);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::VerticalSplit(pid) => {
                screen.get_active_tab_mut().unwrap().vertical_split(pid);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::WriteCharacter(bytes) => {
                let active_tab = screen.get_active_tab_mut().unwrap();
                match active_tab.is_sync_panes_active() {
                    true => active_tab.write_to_terminals_on_current_tab(bytes),
                    false => active_tab.write_to_active_terminal(bytes),
                }
            }
            ScreenInstruction::ResizeLeft => {
                screen.get_active_tab_mut().unwrap().resize_left();
            }
            ScreenInstruction::ResizeRight => {
                screen.get_active_tab_mut().unwrap().resize_right();
            }
            ScreenInstruction::ResizeDown => {
                screen.get_active_tab_mut().unwrap().resize_down();
            }
            ScreenInstruction::ResizeUp => {
                screen.get_active_tab_mut().unwrap().resize_up();
            }
            ScreenInstruction::SwitchFocus => {
                screen.get_active_tab_mut().unwrap().move_focus();
            }
            ScreenInstruction::FocusNextPane => {
                screen.get_active_tab_mut().unwrap().focus_next_pane();
            }
            ScreenInstruction::FocusPreviousPane => {
                screen.get_active_tab_mut().unwrap().focus_previous_pane();
            }
            ScreenInstruction::MoveFocusLeft => {
                screen.get_active_tab_mut().unwrap().move_focus_left();
            }
            ScreenInstruction::MoveFocusLeftOrPreviousTab => {
                if !screen.get_active_tab_mut().unwrap().move_focus_left() {
                    screen.switch_tab_prev();
                }
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::MoveFocusDown => {
                screen.get_active_tab_mut().unwrap().move_focus_down();
            }
            ScreenInstruction::MoveFocusRight => {
                screen.get_active_tab_mut().unwrap().move_focus_right();
            }
            ScreenInstruction::MoveFocusRightOrNextTab => {
                if !screen.get_active_tab_mut().unwrap().move_focus_right() {
                    screen.switch_tab_next();
                }
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::MoveFocusUp => {
                screen.get_active_tab_mut().unwrap().move_focus_up();
            }
            ScreenInstruction::ScrollUp => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .scroll_active_terminal_up();
            }
            ScreenInstruction::ScrollDown => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .scroll_active_terminal_down();
            }
            ScreenInstruction::PageScrollUp => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .scroll_active_terminal_up_page();
            }
            ScreenInstruction::PageScrollDown => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .scroll_active_terminal_down_page();
            }
            ScreenInstruction::ClearScroll => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .clear_active_terminal_scroll();
            }
            ScreenInstruction::CloseFocusedPane => {
                screen.get_active_tab_mut().unwrap().close_focused_pane();
                screen.render();
            }
            ScreenInstruction::SetSelectable(id, selectable) => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .set_pane_selectable(id, selectable);
            }
            ScreenInstruction::SetMaxHeight(id, max_height) => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .set_pane_max_height(id, max_height);
            }
            ScreenInstruction::SetInvisibleBorders(id, invisible_borders) => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .set_pane_invisible_borders(id, invisible_borders);
                screen.render();
            }
            ScreenInstruction::ClosePane(id) => {
                screen.get_active_tab_mut().unwrap().close_pane(id);
                screen.render();
            }
            ScreenInstruction::ToggleActiveTerminalFullscreen => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .toggle_active_pane_fullscreen();
            }
            ScreenInstruction::NewTab(pane_id) => {
                screen.new_tab(pane_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::SwitchTabNext => {
                screen.switch_tab_next();
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::SwitchTabPrev => {
                screen.switch_tab_prev();
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::CloseTab => {
                screen.close_tab();
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::ApplyLayout(layout, new_pane_pids) => {
                screen.apply_layout(layout, new_pane_pids);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::GoToTab(tab_index) => {
                screen.go_to_tab(tab_index as usize);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            ScreenInstruction::UpdateTabName(c) => {
                screen.update_active_tab_name(c);
            }
            ScreenInstruction::TerminalResize(new_size) => {
                screen.resize_to_screen(new_size);
            }
            ScreenInstruction::ChangeMode(mode_info) => {
                screen.change_mode(mode_info);
            }
            ScreenInstruction::ToggleActiveSyncTab => {
                screen
                    .get_active_tab_mut()
                    .unwrap()
                    .toggle_sync_panes_is_active();
                screen.update_tabs();
            }
            ScreenInstruction::Exit => {
                break;
            }
        }
    }
}

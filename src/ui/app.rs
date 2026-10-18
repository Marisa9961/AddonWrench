mod columns;
mod commands;
mod context_menu;
mod details;
mod filters;
mod group_info;
mod icons;
mod layout;
mod notifications;
mod overlays;
mod pages;
mod preferences;
mod runtime;
mod settings;
mod tasks;
mod timestamps;
mod widgets;
mod workspace;

use super::search_input;
use super::search_input::TextInput;
use crate::steam::workshop;
use addon_wrench::{
    addons::{self, Addon},
    browser::{Browser, Sort, SortColumn, Status},
    discovery, games, groups,
};
use columns::{Column, ColumnResize};
use context_menu::{ContextMenu, ContextTarget};
use gpui::{
    App, Application, AvailableSpace, Bounds, ClipboardItem, Context, Corner, Div, ElementId,
    Entity, FocusHandle, Focusable, FontWeight, Image, ImageFormat, IntoElement, MouseButton,
    ObjectFit, PathPromptOptions, Pixels, Point, Render, SharedString, Stateful, TitlebarOptions,
    Window, WindowBounds, WindowOptions, anchored, canvas, deferred, div, img, point, prelude::*,
    px, rgb, rgba, size, uniform_list,
};
use overlays::OverlayKind;
use preferences::{OptionalColumn, Preferences};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

const BG: u32 = 0xf5f6f7;
const PANEL: u32 = 0xffffff;
const LINE: u32 = 0xe7e9eb;
const TEXT: u32 = 0x30353b;
const MUTED: u32 = 0x777f87;
const ACCENT: u32 = 0x398568;
const TINT: u32 = 0xedf5f0;

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Status,
    Games,
    Tags,
    Groups(usize),
    AddToGroup,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Addons,
    Settings,
}

#[derive(Clone, Copy)]
enum ButtonStyle {
    Standard,
    Primary,
    Soft,
}

#[derive(Clone)]
enum Command {
    Navigate(Page),
    SwitchGame(PathBuf),
    TechnicalDetails,
    EditWorkshopLink,
    MissingGroup(usize),
    ClearMissingGroup,
    ConfirmGroupToggle,
    Choose,
    LaunchGame,
    ToggleColumn(OptionalColumn),
    ResetColumnWidths,
    Import,
    Refresh,
    Folder,
    DataFolder,
    Toggle(String, bool),
    Bulk(bool),
    Browse(Option<usize>, Status),
    CreateEmptyGroup,
    EditGroup(Option<usize>),
    SaveGroup,
    DeleteGroup(usize),
    RemoveFromGroup,
    GroupToggle(usize, bool),
    AddKeysToGroup(usize, Vec<String>),
    RemoveKeysFromGroup(usize, Vec<String>),
    CreateGroupWith(Vec<String>),
    ContextAddToGroup,
    ContextBack,
    Status(Status),
    Sort(Sort),
    Check(String),
    CheckAll,
    ClearSelection,
    ClearSearch,
    ResetFilters,
    Detail(String),
    Close,
    Menu(Menu),
    Reveal(PathBuf),
    CopyPath(PathBuf),
    DismissMessage,
    Tag(Option<String>),
    FetchWorkshop(String),
    AssociateWorkshop(String),
    WorkshopBrowser(String),
    WorkshopPage(String),
}

impl Command {
    fn allowed_in_details(&self, detail: Option<&str>) -> bool {
        matches!(
            self,
            Self::Close
                | Self::TechnicalDetails
                | Self::EditWorkshopLink
                | Self::Reveal(_)
                | Self::CopyPath(_)
                | Self::WorkshopPage(_)
                | Self::WorkshopBrowser(_)
        ) || matches!(self, Self::Toggle(key, _) | Self::FetchWorkshop(key) | Self::AssociateWorkshop(key) if detail == Some(key.as_str()))
    }

    fn allowed_in_settings(&self) -> bool {
        matches!(
            self,
            Self::Close
                | Self::Navigate(Page::Settings)
                | Self::DataFolder
                | Self::ToggleColumn(_)
                | Self::ResetColumnWidths
                | Self::Refresh
        )
    }

    fn available_while_busy(&self) -> bool {
        matches!(
            self,
            Self::TechnicalDetails
                | Self::EditWorkshopLink
                | Self::Navigate(_)
                | Self::Browse(_, _)
                | Self::Status(_)
                | Self::Sort(_)
                | Self::Check(_)
                | Self::CheckAll
                | Self::ClearSelection
                | Self::ClearSearch
                | Self::ResetFilters
                | Self::Detail(_)
                | Self::Close
                | Self::Menu(_)
                | Self::ContextAddToGroup
                | Self::ContextBack
                | Self::Reveal(_)
                | Self::CopyPath(_)
                | Self::Folder
                | Self::DataFolder
                | Self::ToggleColumn(_)
                | Self::ResetColumnWidths
                | Self::DismissMessage
                | Self::Tag(_)
                | Self::WorkshopPage(_)
                | Self::WorkshopBrowser(_)
        )
    }
}

struct Snapshot {
    scan_error: Option<String>,
    catalog: Option<games::Library>,
    profile: Option<games::Game>,
    game: Option<PathBuf>,
    addons: Vec<Addon>,
    message: String,
}
struct Manager {
    column_resize: Option<ColumnResize>,
    table_header_width: Option<f32>,
    preferences: Preferences,
    preferences_error: Option<String>,
    catalog: games::Library,
    profiles: Vec<games::Game>,
    profile: Option<games::Game>,
    scan_error: Option<String>,
    technical_details: bool,
    runtime: runtime::Monitor,
    workshop_link_editing: bool,
    group_toggle: Option<(usize, bool)>,
    missing_group: Option<usize>,
    game: Option<PathBuf>,
    addons: Vec<Addon>,
    browser: Browser,
    covers: HashMap<String, Arc<Image>>,
    metadata_loading: bool,
    metadata_keys: HashSet<String>,
    metadata_status: HashMap<String, String>,
    workshop_link: Entity<TextInput>,
    metadata_attempted: HashSet<String>,
    groups: Vec<groups::Group>,
    active_group: Option<usize>,
    groups_ready: bool,
    group_editor: Option<Option<usize>>,
    group_error: String,
    group_name: Entity<TextInput>,
    search: Entity<TextInput>,
    focus: FocusHandle,
    menu: Option<Menu>,
    context_menu: Option<ContextMenu>,
    new_group_members: Vec<String>,
    new_group_stay: bool,
    detail: Option<String>,
    detail_hover: Option<String>,
    page: Page,
    busy: bool,
    error: bool,
    message: String,
    message_revision: u64,
    notice: notifications::NoticeState,
}

impl Manager {
    fn new(cx: &mut Context<Self>) -> Self {
        let search = cx.new(TextInput::new);
        cx.observe(&search, |this, input, cx| {
            this.browser.query = input.read(cx).value().to_owned();
            this.reconcile();
            cx.notify();
        })
        .detach();
        let preferences = Preferences::load(&discovery::data_dir().join("preferences.json"));
        let mut manager = Self {
            column_resize: None,
            table_header_width: None,
            preferences: preferences.clone().unwrap_or_default(),
            preferences_error: preferences.err(),
            catalog: games::Library::default(),
            profiles: Vec::new(),
            profile: None,
            scan_error: None,
            technical_details: false,
            runtime: runtime::Monitor::default(),
            workshop_link_editing: false,
            group_toggle: None,
            missing_group: None,
            game: None,
            addons: vec![],
            browser: Browser::default(),
            covers: HashMap::new(),
            metadata_loading: false,
            metadata_keys: HashSet::new(),
            metadata_status: HashMap::new(),
            workshop_link: cx.new(|cx| TextInput::with_placeholder("粘贴工坊链接或 ID", cx)),
            metadata_attempted: HashSet::new(),
            groups: Vec::new(),
            active_group: None,
            groups_ready: true,
            group_editor: None,
            group_error: String::new(),
            group_name: cx.new(|cx| TextInput::with_placeholder("输入分组名称", cx)),
            search,
            focus: cx.focus_handle(),
            menu: None,
            context_menu: None,
            new_group_members: Vec::new(),
            new_group_stay: false,
            detail: None,
            detail_hover: None,
            page: Page::Addons,
            busy: false,
            error: false,
            message: String::new(),
            message_revision: 0,
            notice: notifications::NoticeState::default(),
        };
        manager.job(cx, || {
            let catalog = games::catalog()?;
            let game = catalog.selected.clone();
            let (addons, scan_error) =
                match game.as_ref().map(|path| games::scan_path(path)).transpose() {
                    Ok(addons) => (addons.unwrap_or_default(), None),
                    Err(error) => (Vec::new(), Some(error)),
                };
            Ok(Snapshot {
                scan_error,
                catalog: Some(catalog),
                profile: None,
                message: if game.is_some() {
                    String::new()
                } else {
                    "未找到游戏，请点击左上角添加游戏目录。".into()
                },
                game,
                addons,
            })
        });
        manager.watch_game(cx);
        manager
    }

    fn can_manage(&self) -> bool {
        self.profile.as_ref().is_some_and(games::Game::can_manage)
    }

    fn can_write(&self) -> bool {
        self.can_manage() && self.scan_error.is_none() && !self.busy && self.runtime.can_write()
    }

    fn rebuild_covers(&mut self) {
        self.covers = self
            .addons
            .iter()
            .filter_map(|a| {
                a.metadata.cover.as_ref().map(|bytes| {
                    (
                        a.key.clone(),
                        Arc::new(Image::from_bytes(ImageFormat::Png, bytes.as_ref().clone())),
                    )
                })
            })
            .collect();
    }

    fn reconcile(&mut self) {
        self.browser.members = self
            .active_group
            .and_then(|i| self.groups.get(i))
            .map(|g| g.members.iter().cloned().collect());
        self.browser.reconcile(&self.addons);
        if !self
            .addons
            .iter()
            .any(|a| Some(&a.key) == self.detail.as_ref())
        {
            self.detail = None;
        }
    }
}

impl Render for Manager {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_notice(cx);
        div()
            .id("manager")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Microsoft YaHei")
            .text_sm()
            .on_click(cx.listener(|this, _, _, cx| {
                this.menu = None;
                this.context_menu = None;
                cx.notify();
            }))
            .track_focus(&self.focus)
            .child(self.workspace_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(self.library_panel(window, cx))
                            .when(!self.browser.checked.is_empty(), |el| {
                                el.child(self.batch_bar(cx))
                            }),
                    ),
            )
            .when(!self.message.is_empty(), |el| {
                el.child(self.task_message(cx))
            })
            .when_some(self.context_menu.as_ref(), |el, menu| {
                el.child(self.context_menu_overlay(menu, window, cx))
            })
            .when(self.page != Page::Addons, |el| {
                el.child(self.settings_overlay(window, cx))
            })
            .when(self.group_editor.is_some(), |el| {
                el.child(self.group_dialog(window, cx))
            })
            .when(self.group_toggle.is_some(), |el| {
                el.child(self.group_impact_dialog(window, cx))
            })
            .when(self.missing_group.is_some(), |el| {
                el.child(self.missing_group_dialog(window, cx))
            })
            .when(self.detail.is_some(), |el| {
                el.child(self.dialog(window, cx))
            })
            .when(self.column_resize.is_some(), |el| {
                el.child(self.column_resize_overlay(cx))
            })
    }
}

pub fn run() {
    Application::new().run(|cx: &mut App| {
        search_input::bind_keys(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(820.), px(520.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Addon Wrench".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let manager = cx.new(Manager::new);
                manager.update(cx, |_, cx| {
                    cx.observe_window_bounds(window, |_, _, cx| cx.notify())
                        .detach();
                });
                window.focus(&manager.read(cx).focus);
                manager
            },
        )
        .expect("无法创建 GPUI 窗口");
        cx.activate(true);
    });
}

use super::*;

pub(super) const CHECKBOX_WIDTH: f32 = 36.;

struct HoverHint(SharedString);

impl Render for HoverHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .max_w(px(300.))
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .shadow_md()
            .font_family("Microsoft YaHei")
            .text_xs()
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

pub(super) fn hover_hint(text: impl Into<SharedString>, cx: &mut App) -> gpui::AnyView {
    cx.new(|_| HoverHint(text.into())).into()
}

pub(super) fn tag_labels(addon: &Addon) -> Vec<&str> {
    let mut seen = HashSet::new();
    addon
        .metadata
        .tags()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_lowercase()))
        .collect()
}

impl Manager {
    pub(super) fn cover(&self, addon: &Addon, edge: f32) -> Div {
        div()
            .size(px(edge))
            .flex_shrink_0()
            .rounded(px(4.))
            .overflow_hidden()
            .bg(rgb(BG))
            .when_some(self.covers.get(&addon.key), |el, cover| {
                el.child(
                    img(cover.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
            })
            .when(!self.covers.contains_key(&addon.key), |el| {
                el.flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("无封面")
            })
    }

    pub(super) fn button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        command: Command,
        primary: bool,
        enabled: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        self.styled_button(
            id,
            label,
            command,
            if primary {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Standard
            },
            enabled,
            cx,
        )
    }

    pub(super) fn styled_button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        command: Command,
        style: ButtonStyle,
        enabled: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let label: SharedString = label.into();
        let writes_game = matches!(
            command,
            Command::Import
                | Command::Toggle(_, _)
                | Command::Bulk(_)
                | Command::GroupToggle(_, _)
                | Command::ConfirmGroupToggle
        );
        let enabled = enabled && (!writes_game || self.can_write());
        let protection_hint = if writes_game {
            self.runtime.message().map(str::to_owned)
        } else {
            None
        };
        let active = enabled && (!self.busy || command.available_while_busy());
        let (background, foreground, hover_background) = match style {
            ButtonStyle::Primary => (ACCENT, PANEL, 0x2e7056),
            ButtonStyle::Standard => (PANEL, TEXT, 0xf0f2f3),
            ButtonStyle::Soft if active => (TINT, ACCENT, 0xddeee3),
            ButtonStyle::Soft => (BG, MUTED, BG),
        };
        let icon = command.icon().filter(|_| !label.as_str().is_empty());
        let icon_color = match style {
            ButtonStyle::Primary => PANEL,
            ButtonStyle::Soft if active => ACCENT,
            _ => MUTED,
        };
        let click_command = command.clone();
        let open_menu = match command {
            Command::Menu(menu) if self.menu == Some(menu) => Some(menu),
            _ => None,
        };
        div()
            .id(id)
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .h(px(34.))
            .px_3()
            .rounded(px(4.))
            .text_sm()
            .bg(rgb(background))
            .text_color(rgb(foreground))
            .opacity(if active { 1. } else { 0.4 })
            .when(matches!(style, ButtonStyle::Soft), |el| {
                el.border_1()
                    .border_color(rgb(if active { 0xc7dfd1 } else { LINE }))
            })
            .when(active, |el| {
                el.cursor_pointer().hover(move |s| {
                    let s = s.bg(rgb(hover_background));
                    if matches!(style, ButtonStyle::Soft) {
                        s.border_color(rgb(0x9fc8b2))
                    } else {
                        s
                    }
                })
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if !event.standard_click() {
                        return;
                    }
                    cx.stop_propagation();
                    if active {
                        window.focus(&this.focus);
                        let had_modal = this.modal_open();
                        this.command(click_command.clone(), cx);
                        if this.group_editor.is_some() {
                            window.focus(&this.group_name.focus_handle(cx));
                        } else if had_modal && !this.modal_open() {
                            window.focus(&this.focus);
                        }
                    }
                }),
            )
            .when_some(icon, |el, icon| {
                el.child(
                    icons::leading(icon)
                        .mr(px(6.))
                        .text_color(rgb(icon_color)),
                )
            })
            .child(label)
            .when_some(protection_hint, |el, hint| {
                el.tooltip(move |_, cx| hover_hint(hint.clone(), cx))
            })
            .when_some(open_menu, |el, menu| {
                let popup = self.popup(menu, cx);
                let backdrop = self.overlay_backdrop("popup-shield", OverlayKind::Menu, cx);
                el.child(
                    canvas(
                        move |bounds, window, cx| {
                            let viewport = window.viewport_size();
                            let above = menu == Menu::AddToGroup;
                            let position = point(
                                bounds.left(),
                                if above {
                                    bounds.top() - px(6.)
                                } else {
                                    bounds.bottom() + px(6.)
                                },
                            );
                            let popup = popup.max_w((viewport.width - px(16.)).max(px(0.))).max_h(
                                (viewport.height - px(16.))
                                    .max(px(0.))
                                    .min(px(if menu == Menu::Tags { 380. } else { 260. })),
                            );
                            let mut overlay = backdrop
                                .w(viewport.width)
                                .h(viewport.height)
                                .child(
                                    anchored()
                                        .position(position)
                                        .anchor(if above {
                                            Corner::BottomLeft
                                        } else {
                                            Corner::TopLeft
                                        })
                                        .snap_to_window_with_margin(px(8.))
                                        .child(popup),
                                )
                                .into_any_element();
                            overlay.layout_as_root(
                                size(
                                    AvailableSpace::Definite(viewport.width),
                                    AvailableSpace::Definite(viewport.height),
                                ),
                                window,
                                cx,
                            );
                            window.defer_draw(overlay, point(px(0.), px(0.)), 1);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
            })
    }

    pub(super) fn checkbox(
        &self,
        id: impl Into<ElementId>,
        checked: bool,
        partial: bool,
        command: Command,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let hint = match &command {
            Command::Check(_) if checked => Some("点击取消勾选此模组"),
            Command::Check(_) => Some("点击勾选此模组，用于批量操作"),
            Command::CheckAll if checked => Some("取消勾选当前结果中的全部模组"),
            Command::CheckAll => Some("勾选当前结果中的全部模组"),
            _ => None,
        };
        div()
            .id(id)
            .w(px(CHECKBOX_WIDTH))
            .h(px(36.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .rounded(px(4.))
            .hover(|el| el.bg(rgb(0xe3f0e8)))
            .when_some(hint, |el, hint| {
                el.tooltip(move |_, cx| hover_hint(hint, cx))
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if !event.standard_click() {
                        return;
                    }
                    cx.stop_propagation();
                    window.focus(&this.focus);
                    this.command(command.clone(), cx);
                }),
            )
            .child(
                div()
                    .size(px(16.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(rgb(if checked || partial { ACCENT } else { 0xbcc2c8 }))
                    .bg(rgb(if checked || partial { ACCENT } else { PANEL }))
                    .when(!checked && !partial, |el| {
                        el.group_hover("addon-row", |style| {
                            style.border_color(rgb(ACCENT)).bg(rgb(TINT))
                        })
                    })
                    .text_color(rgb(PANEL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .child(if checked {
                        "✓"
                    } else if partial {
                        "−"
                    } else {
                        ""
                    }),
            )
    }

    pub(super) fn menu_item(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        command: Command,
        primary: bool,
        enabled: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        self.button(id, label, command, primary, enabled, cx)
            .h(px(32.))
            .overflow_hidden()
            .text_ellipsis()
            .justify_start()
    }

    pub(super) fn popup(&self, menu: Menu, cx: &Context<Self>) -> Stateful<Div> {
        overlays::panel("popup")
            .max_h(px(if menu == Menu::Tags { 380. } else { 260. }))
            .overflow_y_scroll()
            .w(px(if menu == Menu::Tags { 340. } else { 260. }))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .shadow_lg()
            .when(menu == Menu::Games, |el| {
                el.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("切换游戏"),
                )
                .children(self.profiles.iter().enumerate().map(|(index, game)| {
                    self.menu_item(
                        ("game-choice", index),
                        format!(
                            "{}{}",
                            if self.game.as_ref() == Some(&game.path) {
                                "✓ "
                            } else {
                                ""
                            },
                            game.name
                        ),
                        Command::SwitchGame(game.path.clone()),
                        false,
                        !self.busy,
                        cx,
                    )
                }))
                .child(div().h(px(1.)).my_1().bg(rgb(LINE)))
                .child(self.menu_item(
                    "add-game",
                    "添加游戏目录",
                    Command::Choose,
                    false,
                    !self.busy,
                    cx,
                ))
            })
            .when_some(
                if let Menu::Groups(index) = menu {
                    Some(index)
                } else {
                    None
                },
                |el, index| el.child(self.group_menu_items(index, cx)),
            )
            .when(menu == Menu::AddToGroup, |el| {
                let keys = self.browser.checked.iter().cloned().collect::<Vec<_>>();
                el.children(self.groups.iter().enumerate().map(|(i, _)| {
                    self.group_join_item(("join-group", i), i, &keys, !keys.is_empty(), cx)
                }))
                .child(div().h(px(1.)).my_1().bg(rgb(LINE)))
                .child(self.menu_item(
                    "create-with-selection",
                    "新建分组并加入所选",
                    Command::EditGroup(None),
                    false,
                    true,
                    cx,
                ))
            })
            .when(menu == Menu::Tags, |el| el.child(self.tag_menu_items(cx)))
            .when(menu == Menu::Status, |el| {
                el.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("启用状态"),
                )
                .children(
                    [
                        (Status::All, "全部状态"),
                        (Status::Enabled, "仅已启用"),
                        (Status::Disabled, "仅已禁用"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (status, label))| {
                        self.menu_item(
                            ("status", i),
                            format!(
                                "{} {label}",
                                if self.browser.status == status {
                                    "✓"
                                } else {
                                    "  "
                                }
                            ),
                            Command::Status(status),
                            false,
                            status == Status::All || self.can_manage(),
                            cx,
                        )
                    }),
                )
            })
    }
}

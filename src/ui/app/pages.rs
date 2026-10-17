use super::*;

impl Manager {
    pub(super) fn workspace_header(&self, cx: &Context<Self>) -> Div {
        let name = self
            .profile
            .as_ref()
            .map(|game| game.name.as_str())
            .unwrap_or("选择游戏");
        div()
            .flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .mr_3()
                    .child("Addon Wrench"),
            )
            .child(
                self.button(
                    "game-selector",
                    format!("{name} ▾"),
                    Command::Menu(Menu::Games),
                    false,
                    true,
                    cx,
                )
                .max_w(px(260.))
                .overflow_hidden(),
            )
            .child(div().flex_1())
            .when_some(self.runtime.message(), |el, message| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(message.to_owned()),
                )
            })
            .child(
                self.button(
                    "launch-game",
                    "启动游戏",
                    Command::LaunchGame,
                    true,
                    self.profile
                        .as_ref()
                        .and_then(games::Game::launch_url)
                        .is_some(),
                    cx,
                ),
            )
            .child(self.button(
                "open-folder",
                "模组文件夹",
                Command::Folder,
                false,
                self.game.is_some(),
                cx,
            ))
            .child(self.button(
                "settings",
                "设置",
                Command::Navigate(Page::Settings),
                false,
                true,
                cx,
            ))
    }

    pub(super) fn sidebar(&self, cx: &Context<Self>) -> Stateful<Div> {
        div()
            .w(px(220.))
            .id("sidebar")
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_context_menu(ContextTarget::Sidebar, event.position, window, cx);
                }),
            )
            .child(
                div()
                    .id("library-navigation")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .child(self.navigation_item(
                        "nav-all",
                        "全部模组",
                        self.addons.len(),
                        None,
                        Status::All,
                        cx,
                    ))
                    .child(
                        div()
                            .mt_5()
                            .mb_2()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .px_2()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child("分组"),
                            )
                            .child(self.button(
                                "new-group",
                                "＋",
                                Command::CreateEmptyGroup,
                                false,
                                self.groups_ready && self.game.is_some(),
                                cx,
                            )),
                    )
                    .when(!self.groups_ready, |el| {
                        el.child(
                            div()
                                .p_2()
                                .text_xs()
                                .text_color(rgb(0xa33929))
                                .child("分组读取失败，请刷新重试"),
                        )
                    })
                    .when(self.groups_ready && self.groups.is_empty(), |el| {
                        el.child(
                            div()
                                .p_2()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("点击 ＋ 创建分组；勾选模组后可加入分组。"),
                        )
                    })
                    .children(self.groups.iter().enumerate().map(|(index, group)| {
                        let count = self
                            .addons
                            .iter()
                            .filter(|addon| group.members.contains(&addon.key))
                            .count();
                        div()
                            .id(("group-row", index))
                            .relative()
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        this.open_context_menu(
                                            ContextTarget::Group(index),
                                            event.position,
                                            window,
                                            cx,
                                        );
                                    },
                                ),
                            )
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                self.navigation_item(
                                    ("nav-group", index),
                                    &group.name,
                                    count,
                                    Some(index),
                                    Status::All,
                                    cx,
                                )
                                .flex_1()
                                .min_w_0(),
                            )
                            .when(self.can_manage(), |el| {
                                el.child(self.group_switch(index, cx))
                            })
                            .child(
                                self.button(
                                    ("group-menu", index),
                                    "⋯",
                                    Command::Menu(Menu::Groups(index)),
                                    false,
                                    self.groups_ready,
                                    cx,
                                )
                                .px_2(),
                            )
                            .when(
                                self.context_menu
                                    .as_ref()
                                    .is_some_and(|menu| menu.targets_group(index))
                                    || self.menu == Some(Menu::Groups(index)),
                                |el| {
                                    el.child(
                                        div()
                                            .absolute()
                                            .inset_0()
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(rgb(ACCENT)),
                                    )
                                },
                            )
                    })),
            )
            .when(self.metadata_loading, |el| {
                el.child(
                    div()
                        .p_3()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("正在更新工坊资料…"),
                )
            })
    }

    fn group_switch(&self, index: usize, cx: &Context<Self>) -> Stateful<Div> {
        let group = &self.groups[index];
        let target = group.toggle_target(&self.addons);
        let enabled = target == Some(false);
        let mixed = target == Some(true) && !group.changes(&self.addons, false).is_empty();
        self.button(
            ("group-toggle", index),
            "",
            Command::GroupToggle(index, target.unwrap_or(true)),
            false,
            self.groups_ready && target.is_some(),
            cx,
        )
        .w(px(36.))
        .px_1()
        .child(
            div()
                .w(px(28.))
                .h(px(18.))
                .p(px(2.))
                .flex()
                .items_center()
                .rounded_full()
                .bg(rgb(if enabled {
                    ACCENT
                } else if mixed {
                    0xb18b42
                } else {
                    0xbfc5cb
                }))
                .when(enabled, |el| el.justify_end())
                .when(mixed, |el| el.justify_center())
                .child(
                    div()
                        .size(px(14.))
                        .rounded_full()
                        .bg(rgb(PANEL))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(if mixed { "−" } else { "" }),
                ),
        )
    }

    fn navigation_item(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        count: usize,
        group: Option<usize>,
        status: Status,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let active = self.active_group == group;
        div()
            .id(id)
            .h(px(40.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .bg(rgb(if active { TINT } else { PANEL }))
            .text_color(rgb(if active { ACCENT } else { TEXT }))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(BG)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let target = group
                        .map(ContextTarget::Group)
                        .unwrap_or(ContextTarget::Sidebar);
                    this.open_context_menu(target, event.position, window, cx);
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.standard_click() {
                        window.focus(&this.focus);
                        this.command(Command::Browse(group, status), cx);
                    }
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(label.to_owned()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(count.to_string()),
            )
    }

    pub(super) fn library_heading(&self, cx: &Context<Self>) -> Div {
        let group = self.active_group.and_then(|index| self.groups.get(index));
        let members: Vec<_> = self
            .addons
            .iter()
            .filter(|addon| group.is_none_or(|group| group.members.contains(&addon.key)))
            .collect();
        let enabled = members.iter().filter(|addon| addon.enabled).count();
        let summary = if self.can_manage() {
            format!("{} 个模组 · 已启用 {enabled} 个", members.len())
        } else {
            format!("{} 个模组 · 仅浏览，启用状态未知", members.len())
        };
        div()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_1()
                    .min_w(px(150.))
                    .child(
                        div().font_weight(FontWeight::BOLD).child(
                            group
                                .map(|group| group.name.clone())
                                .unwrap_or_else(|| "全部模组".into()),
                        ),
                    )
                    .child(div().mt_1().text_xs().text_color(rgb(MUTED)).child(summary)),
            )
            .when_some(self.active_group, |el, index| {
                let group = &self.groups[index];
                let missing = group.members.len() - members.len();
                let shared = groups::overlaps(&self.groups, index)
                    .into_iter()
                    .map(|(other, count)| format!("{}（{count} 个）", self.groups[other].name))
                    .collect::<Vec<_>>()
                    .join("、");
                el.when(self.can_manage(), |el| {
                    el.child(self.button(
                        "enable-group",
                        format!("启用整组（{}）", members.len()),
                        Command::GroupToggle(index, true),
                        false,
                        !group.changes(&self.addons, true).is_empty(),
                        cx,
                    ))
                    .child(self.button(
                        "disable-group",
                        format!("禁用整组（{}）", members.len()),
                        Command::GroupToggle(index, false),
                        false,
                        !group.changes(&self.addons, false).is_empty(),
                        cx,
                    ))
                })
                .when(missing > 0, |el| {
                    el.child(self.button(
                        "missing-group",
                        format!("缺失 {missing} 个"),
                        Command::MissingGroup(index),
                        false,
                        true,
                        cx,
                    ))
                })
                .child(div().w_full().text_xs().text_color(rgb(MUTED)).child(
                    if shared.is_empty() {
                        "整组操作包含被搜索和筛选隐藏的成员。".into()
                    } else {
                        format!("整组操作包含隐藏成员 · 共享成员：{shared}")
                    },
                ))
            })
    }
}

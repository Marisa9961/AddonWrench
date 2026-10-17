use super::*;

const ROW_TAG_LIMIT: usize = 3;

impl Manager {
    pub(super) fn library_panel(&self, window: &Window, cx: &Context<Self>) -> Div {
        let columns = self
            .preferences
            .visible(window.viewport_size().width >= px(1040.));
        let layout = self.column_layout(columns);
        let count = self.browser.visible(&self.addons).len();
        let checked = self.browser.checked.len();
        let all_checked = count > 0 && checked == count;
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .m_4()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.))
            .overflow_hidden()
            .when(self.game.is_some(), |el| {
                el.child(self.library_heading(cx))
                    .child(self.toolbar(cx))
                    .child(self.table_header(columns, layout, all_checked, checked, cx))
            })
            .when(count > 0, |el| {
                el.child(
                    uniform_list(
                        "addon-list",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            let visible = this.browser.visible(&this.addons);
                            range
                                .filter_map(|index| visible.get(index))
                                .map(|addon| this.row(addon, columns, layout, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .flex_1()
                    .min_h_0(),
                )
            })
            .when(count == 0, |el| el.child(self.empty(cx)))
    }

    pub(super) fn sort_header(
        &self,
        id: &'static str,
        label: &str,
        column: SortColumn,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let active = self.browser.sort.column() == column;
        let highlighted = active && self.browser.sort != Sort::Name;
        let arrow = if !active {
            "↕"
        } else if self.browser.sort.ascending() {
            "↑"
        } else {
            "↓"
        };
        self.button(
            id,
            format!("{label} {arrow}"),
            Command::Sort(self.browser.sort.click_column(column)),
            false,
            true,
            cx,
        )
        .h(px(28.))
        .px_1()
        .justify_start()
        .text_xs()
        .bg(rgb(if highlighted { TINT } else { 0xfafbfc }))
        .text_color(rgb(if highlighted { ACCENT } else { MUTED }))
    }

    pub(super) fn modified_label(addon: &Addon) -> String {
        timestamps::format_modified(addon.modified)
    }

    pub(super) fn batch_bar(&self, cx: &Context<Self>) -> Div {
        div()
            .min_h(px(60.))
            .py_2()
            .px_4()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_color(rgb(ACCENT))
                    .child(format!("已选 {} 项", self.browser.checked.len())),
            )
            .child(self.button(
                "clear-selection",
                "取消选择",
                Command::ClearSelection,
                false,
                true,
                cx,
            ))
            .child(div().flex_1())
            .when(self.can_manage(), |el| {
                el.child(self.button(
                    "enable-selected",
                    "启用所选",
                    Command::Bulk(true),
                    false,
                    !self.browser.changes(&self.addons, true).is_empty(),
                    cx,
                ))
                .child(self.button(
                    "disable-selected",
                    "禁用所选",
                    Command::Bulk(false),
                    false,
                    !self.browser.changes(&self.addons, false).is_empty(),
                    cx,
                ))
            })
            .child(self.button(
                "add-to-group",
                "加入分组 ▾",
                Command::Menu(Menu::AddToGroup),
                false,
                self.groups_ready,
                cx,
            ))
            .when(self.active_group.is_some(), |el| {
                el.child(self.button(
                    "remove-from-group",
                    "移出当前分组",
                    Command::RemoveFromGroup,
                    false,
                    self.groups_ready,
                    cx,
                ))
            })
    }
}

impl Manager {
    pub(super) fn detail_link(&self, addon: &Addon, cx: &Context<Self>) -> Stateful<Div> {
        let key = addon.key.clone();
        let hover_key = key.clone();
        div()
            .id(SharedString::from(format!("detail-title-{key}")))
            .cursor_pointer()
            .tooltip(|_, cx| widgets::hover_hint("查看模组详情", cx))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.detail_hover = Some(hover_key.clone());
                } else if this.detail_hover.as_ref() == Some(&hover_key) {
                    this.detail_hover = None;
                }
                cx.notify();
            }))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.standard_click() {
                        cx.stop_propagation();
                        window.focus(&this.focus);
                        this.command(Command::Detail(key.clone()), cx);
                    }
                }),
            )
    }

    pub(super) fn row(
        &self,
        addon: &Addon,
        columns: Preferences,
        layout: columns::TableLayout,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let checked = self.browser.checked.contains(&addon.key);
        let key = addon.key.clone();
        let context_key = addon.key.clone();
        let toggle_key = addon.key.clone();
        let enabled = addon.enabled;
        let detail_hovered = self.detail_hover.as_ref() == Some(&addon.key);
        let tags = widgets::tag_labels(addon);
        div()
            .id(SharedString::from(format!("row-{}", addon.key)))
            .relative()
            .group("addon-row")
            .w_full()
            .h(px(76.))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(rgb(if checked { TINT } else { PANEL }))
            .hover(move |s| s.bg(rgb(if checked { 0xe3f0e8 } else { 0xf6f9f7 })))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let target = ContextTarget::addons(
                        &context_key,
                        &this.browser.checked,
                        this.active_group,
                    );
                    this.open_context_menu(target, event.position, window, cx);
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.standard_click() {
                        window.focus(&this.focus);
                        this.command(Command::Check(key.clone()), cx);
                    }
                }),
            )
            .child(self.checkbox(
                SharedString::from(format!("check-{}", addon.key)),
                checked,
                false,
                Command::Check(addon.key.clone()),
                cx,
            ))
            .child(
                div()
                    .w(px(layout.width(Column::Name)))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.cover(addon, 52.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .child(
                                self.detail_link(addon, cx)
                                    .max_w_full()
                                    .text_sm()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .text_color(rgb(
                                        if !addon_wrench::metadata::has_resolved_title(addon) {
                                            MUTED
                                        } else if detail_hovered {
                                            ACCENT
                                        } else {
                                            TEXT
                                        },
                                    ))
                                    .child(addon.name.clone()),
                            )
                            .when(!tags.is_empty(), |el| {
                                el.child(
                                    div()
                                        .mt_1()
                                        .w_full()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_xs()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex()
                                                .gap_1()
                                                .overflow_hidden()
                                                .children(tags.iter().take(ROW_TAG_LIMIT).map(
                                                    |tag| {
                                                        div()
                                                            .min_w_0()
                                                            .max_w(px(112.))
                                                            .px_2()
                                                            .py(px(2.))
                                                            .rounded(px(3.))
                                                            .bg(rgb(TINT))
                                                            .text_color(rgb(ACCENT))
                                                            .text_ellipsis()
                                                            .overflow_hidden()
                                                            .child((*tag).to_owned())
                                                    },
                                                )),
                                        )
                                        .when(tags.len() > ROW_TAG_LIMIT, |el| {
                                            el.child(
                                                div().flex_shrink_0().text_color(rgb(MUTED)).child(
                                                    format!("+{}", tags.len() - ROW_TAG_LIMIT),
                                                ),
                                            )
                                        }),
                                )
                            }),
                    ),
            )
            .when(columns.author_column, |el| {
                el.child(
                    div()
                        .w(px(layout.width(Column::Author)))
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .text_ellipsis()
                        .overflow_hidden()
                        .child(if addon.metadata.author.trim().is_empty() {
                            "—".into()
                        } else {
                            addon.metadata.author.clone()
                        }),
                )
            })
            .child(
                div()
                    .w(px(layout.width(Column::Size)))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(addons::human_size(addon.bytes)),
            )
            .when(columns.modified_column, |el| {
                el.child(
                    div()
                        .w(px(layout.width(Column::Modified)))
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(Self::modified_label(addon)),
                )
            })
            .child(
                div()
                    .w(px(layout.width(Column::Status)))
                    .h(px(36.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .opacity(if !self.can_write() { 0.4 } else { 1. })
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(if enabled { ACCENT } else { MUTED }))
                            .child(if !self.can_manage() {
                                "仅浏览"
                            } else if enabled {
                                "已启用"
                            } else {
                                "已禁用"
                            }),
                    )
                    .when(self.can_manage(), |el| {
                        el.child(
                            div()
                                .id(SharedString::from(format!("toggle-{}", addon.key)))
                                .on_click(cx.listener(
                                    move |this, event: &gpui::ClickEvent, window, cx| {
                                        if event.standard_click() {
                                            cx.stop_propagation();
                                            if this.can_write() {
                                                window.focus(&this.focus);
                                                this.command(
                                                    Command::Toggle(toggle_key.clone(), !enabled),
                                                    cx,
                                                );
                                            }
                                        }
                                    },
                                ))
                                .w(px(30.))
                                .h(px(18.))
                                .p(px(2.))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .bg(rgb(if enabled { ACCENT } else { 0xbfc5cb }))
                                .when(enabled, |el| el.justify_end())
                                .child(div().size(px(14.)).rounded_full().bg(rgb(PANEL))),
                        )
                    }),
            )
            .when(
                self.context_menu
                    .as_ref()
                    .is_some_and(|menu| menu.targets_addon(&addon.key)),
                |el| {
                    el.child(
                        div()
                            .absolute()
                            .inset_0()
                            .border_1()
                            .border_color(rgb(ACCENT)),
                    )
                },
            )
    }

    pub(super) fn toolbar(&self, cx: &Context<Self>) -> Div {
        let filtered = !self.browser.query.is_empty()
            || self.browser.tag.is_some()
            || self.browser.status != Status::All
            || self.browser.sort != Sort::Name;
        div()
            .flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap_2()
            .py_3()
            .px_4()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_1()
                    .min_w(px(200.))
                    .max_w_full()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .px_3()
                    .gap_2()
                    .border_1()
                    .border_color(rgb(LINE))
                    .rounded(px(4.))
                    .child(self.search.clone())
                    .when(!self.browser.query.is_empty(), |el| {
                        el.child(self.button(
                            "clear-search",
                            "清除",
                            Command::ClearSearch,
                            false,
                            true,
                            cx,
                        ))
                    }),
            )
            .when(self.can_manage(), |el| {
                el.child(self.button(
                    "import-addons",
                    "添加模组",
                    Command::Import,
                    false,
                    self.can_write(),
                    cx,
                ))
            })
            .child(
                self.button(
                    "tag-menu",
                    self.browser
                        .tag
                        .as_ref()
                        .map(|tag| {
                            if tag.is_empty() {
                                "标签：无标签 ▾".into()
                            } else {
                                format!("标签：{tag} ▾")
                            }
                        })
                        .unwrap_or_else(|| "标签：全部 ▾".into()),
                    Command::Menu(Menu::Tags),
                    false,
                    true,
                    cx,
                )
                .max_w(px(180.))
                .overflow_hidden()
                .when(self.browser.tag.is_some(), |el| {
                    el.bg(rgb(TINT)).text_color(rgb(ACCENT))
                }),
            )
            .child(self.button(
                "reset-filters",
                "重置视图",
                Command::ResetFilters,
                false,
                filtered,
                cx,
            ))
    }

    pub(super) fn empty(&self, cx: &Context<Self>) -> Div {
        let connected = self.game.is_some();
        let filtered = connected && !self.addons.is_empty();
        let empty_group = self.active_group.is_some_and(|i| {
            !self
                .addons
                .iter()
                .any(|a| self.groups[i].members.contains(&a.key))
        });
        let (title, hint) = if self.busy {
            ("正在读取模组", "请稍候")
        } else if let Some(error) = &self.scan_error {
            ("扫描未完成", error.as_str())
        } else if !connected {
            ("选择游戏目录", "连接游戏后，即可查看和管理模组。")
        } else if empty_group {
            (
                "组内还没有已安装的模组",
                "在全部模组中勾选条目，再点击底部“加入分组”。",
            )
        } else if filtered {
            ("没有找到匹配的模组", "换个关键词，或清除筛选条件。")
        } else if !self.can_manage() {
            (
                "未发现可浏览的 VPK 模组",
                "目前可浏览 addons 和 custom 中的 VPK，其他模组格式尚未适配。",
            )
        } else {
            (
                "还没有安装模组",
                "添加本地 VPK 文件，或在 Steam 创意工坊订阅模组。",
            )
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .child(div().text_lg().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(hint.to_owned()),
            )
            .when(!self.busy, |el| {
                el.child(self.button(
                    "empty-action",
                    if self.scan_error.is_some() && connected {
                        "重新扫描"
                    } else if !connected {
                        "选择游戏目录"
                    } else if empty_group {
                        "查看全部模组"
                    } else if filtered {
                        "清除搜索与筛选"
                    } else if !self.can_manage() {
                        "选择其他游戏"
                    } else {
                        "添加模组"
                    },
                    if self.scan_error.is_some() && connected {
                        Command::Refresh
                    } else if !connected {
                        Command::Choose
                    } else if empty_group {
                        Command::Browse(None, Status::All)
                    } else if filtered {
                        Command::ResetFilters
                    } else if !self.can_manage() {
                        Command::Menu(Menu::Games)
                    } else {
                        Command::Import
                    },
                    true,
                    true,
                    cx,
                ))
            })
    }
}

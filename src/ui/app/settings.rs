use super::*;

impl Manager {
    pub(super) fn settings_overlay(&self, window: &Window, cx: &Context<Self>) -> Stateful<Div> {
        self.overlay_backdrop("settings-overlay", OverlayKind::Modal, cx)
            .flex()
            .justify_end()
            .bg(rgba(0x17251d33))
            .child(
                overlays::panel("settings-panel")
                    .w(px(560.).min(window.viewport_size().width))
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(rgb(BG))
                    .shadow_lg()
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_5()
                            .py_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .bg(rgb(PANEL))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_weight(FontWeight::BOLD)
                                    .child("设置"),
                            )
                            .child(self.close_button("settings-close", cx)),
                    )
                    .child(
                        div()
                            .id("settings-content")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_5()
                            .child(self.settings_content(cx)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_5()
                            .py_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .bg(rgb(PANEL))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .child(div().flex_1())
                            .child(self.button(
                                "settings-done",
                                "完成",
                                Command::Close,
                                true,
                                true,
                                cx,
                            )),
                    ),
            )
    }

    fn settings_content(&self, cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Self::settings_card()
                    .child(
                        div()
                            .px_4()
                            .pt_3()
                            .pb_2()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::BOLD)
                                    .child("列表显示"),
                            )
                            .child(
                                self.button(
                                    "reset-column-widths",
                                    "重置列宽",
                                    Command::ResetColumnWidths,
                                    false,
                                    true,
                                    cx,
                                )
                                .h(px(28.))
                                .text_xs(),
                            ),
                    )
                    .when_some(self.preferences_error.as_ref(), |el, error| {
                        el.child(
                            div()
                                .mx_4()
                                .mb_3()
                                .p_3()
                                .rounded(px(4.))
                                .bg(rgb(0xfcf0ee))
                                .text_xs()
                                .line_height(px(20.))
                                .text_color(rgb(0xb54747))
                                .child(error.clone()),
                        )
                    })
                    .child(self.column_option(
                        "show-author-column",
                        "作者列",
                        self.preferences.author_column,
                        OptionalColumn::Author,
                        cx,
                    ))
                    .child(Self::settings_divider())
                    .child(self.column_option(
                        "show-modified-column",
                        "修改时间列",
                        self.preferences.modified_column,
                        OptionalColumn::Modified,
                        cx,
                    )),
            )
            .child(
                Self::settings_card()
                    .child(Self::settings_card_heading("数据与维护"))
                    .child(self.settings_action(
                        "settings-rescan",
                        (
                            "模组列表",
                            if self.busy {
                                "扫描中…"
                            } else {
                                "重新扫描"
                            },
                        ),
                        Command::Refresh,
                        self.game.is_some() && !self.busy,
                        cx,
                    ))
                    .child(Self::settings_divider())
                    .child(self.settings_action(
                        "settings-data",
                        ("数据目录", "打开目录"),
                        Command::DataFolder,
                        true,
                        cx,
                    )),
            )
            .child(Self::settings_info())
    }

    fn settings_card() -> Div {
        div()
            .flex_shrink_0()
            .min_w_0()
            .py_1()
            .rounded(px(8.))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
    }

    fn settings_card_heading(title: &str) -> Div {
        div()
            .px_4()
            .pt_3()
            .pb_2()
            .font_weight(FontWeight::BOLD)
            .child(title.to_owned())
    }

    fn settings_divider() -> Div {
        div().mx_4().h(px(1.)).bg(rgb(LINE))
    }

    fn settings_action(
        &self,
        id: &'static str,
        (title, action): (&str, &str),
        command: Command,
        enabled: bool,
        cx: &Context<Self>,
    ) -> Div {
        div()
            .px_4()
            .py_3()
            .min_h(px(56.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(div().flex_1().min_w_0().child(title.to_owned()))
            .child(
                self.styled_button(
                    id,
                    action.to_owned(),
                    command,
                    ButtonStyle::Soft,
                    enabled,
                    cx,
                )
                .h(px(32.)),
            )
    }

    fn column_option(
        &self,
        id: &'static str,
        title: &str,
        enabled: bool,
        column: OptionalColumn,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .mx_2()
            .px_2()
            .py_4()
            .min_h(px(56.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(BG)))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.standard_click() {
                        cx.stop_propagation();
                        window.focus(&this.focus);
                        this.command(Command::ToggleColumn(column), cx);
                    }
                }),
            )
            .child(div().flex_1().min_w_0().child(title.to_owned()))
            .child(
                div()
                    .w(px(34.))
                    .h(px(20.))
                    .p(px(3.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(rgb(if enabled { ACCENT } else { 0xbfc5cb }))
                    .when(enabled, |el| el.justify_end())
                    .child(div().size(px(14.)).rounded_full().bg(rgb(PANEL))),
            )
    }

    fn settings_info() -> Div {
        div()
            .flex_shrink_0()
            .px_4()
            .py_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(div().font_weight(FontWeight::BOLD).child("Addon Wrench"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("版本 {}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Marisa9961 · MIT License"),
            )
    }
}

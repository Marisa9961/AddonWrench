use super::*;

impl Manager {
    pub(super) fn group_impact_dialog(&self, window: &Window, cx: &Context<Self>) -> Stateful<Div> {
        let (index, enabled) = self.group_toggle.unwrap();
        let group = &self.groups[index];
        let changes = group.changes(&self.addons, enabled);
        let affected = groups::affected_groups(&self.groups, index, &changes);
        let content = div()
            .child(div().mt_4().child(format!(
                "{}“{}”将调整 {} 个模组。",
                if enabled { "启用" } else { "禁用" },
                group.name,
                changes.len()
            )))
            .child(
                div()
                    .mt_3()
                    .text_color(rgb(MUTED))
                    .child("以下分组包含这些模组，它们的启用情况也会随之改变："),
            )
            .children(affected.into_iter().map(|(index, count)| {
                div()
                    .mt_3()
                    .child(format!("{} · 影响 {count} 个模组", self.groups[index].name))
            }))
            .child(
                div()
                    .mt_5()
                    .flex()
                    .justify_end()
                    .gap_3()
                    .child(self.button("impact-cancel", "取消", Command::Close, false, true, cx))
                    .child(self.button(
                        "impact-confirm",
                        "确认调整",
                        Command::ConfirmGroupToggle,
                        true,
                        true,
                        cx,
                    )),
            );
        self.information_dialog("group-impact", "分组操作影响", content, window, cx)
    }

    pub(super) fn missing_group_dialog(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let group = &self.groups[self.missing_group.unwrap()];
        let content = div()
            .child(
                div()
                    .mt_4()
                    .text_color(rgb(MUTED))
                    .child("这些文件未在最近一次扫描中找到，恢复后会自动匹配原分组。"),
            )
            .children(
                group
                    .members
                    .iter()
                    .filter(|key| !self.addons.iter().any(|addon| &addon.key == *key))
                    .map(|key| div().mt_3().text_sm().child(key.clone())),
            )
            .child(div().mt_5().flex().justify_end().child(self.button(
                "clear-missing-members",
                "移除缺失成员记录",
                Command::ClearMissingGroup,
                false,
                self.groups_ready && self.scan_error.is_none() && !self.busy,
                cx,
            )));
        self.information_dialog("missing-members", "缺失的分组成员", content, window, cx)
    }

    pub(super) fn information_dialog(
        &self,
        id: &'static str,
        title: &str,
        content: Div,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let dialog_size = layout::dialog_size(window.viewport_size(), size(px(560.), px(450.)));
        self.overlay_backdrop(id, OverlayKind::Modal, cx)
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x17251d55))
            .child(
                overlays::panel(SharedString::from(format!("{id}-panel")))
                    .w(dialog_size.width)
                    .max_h(dialog_size.height)
                    .p_6()
                    .overflow_y_scroll()
                    .rounded(px(8.))
                    .bg(rgb(PANEL))
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .child(title.to_owned()),
                            )
                            .child(self.close_button("close-information", cx)),
                    )
                    .child(content),
            )
    }
}

impl Manager {
    pub(super) fn group_dialog(&self, window: &Window, cx: &Context<Self>) -> Stateful<Div> {
        let dialog_size = layout::dialog_size(window.viewport_size(), size(px(440.), px(520.)));
        self.overlay_backdrop("group-editor-overlay", OverlayKind::Modal, cx)
            .bg(rgba(0x17251e55))
            .flex()
            .items_center()
            .justify_center()
            .child(
                overlays::panel("group-editor")
                    .w(dialog_size.width)
                    .max_h(dialog_size.height)
                    .overflow_y_scroll()
                    .p_6()
                    .bg(rgb(PANEL))
                    .rounded(px(8.))
                    .shadow_lg()
                    .child(div().text_lg().child(if self.group_editor == Some(None) {
                        "新建分组"
                    } else {
                        "重命名分组"
                    }))
                    .child(
                        div()
                            .mt_4()
                            .p_3()
                            .border_1()
                            .border_color(rgb(LINE))
                            .rounded(px(4.))
                            .child(self.group_name.clone()),
                    )
                    .child(div().mt_3().text_xs().text_color(rgb(MUTED)).child(
                        if self.group_editor == Some(None) {
                            format!(
                                "创建后包含 {} 个模组；同一模组可加入多个组。",
                                self.new_group_members.len()
                            )
                        } else {
                            "修改组名不会改变成员或启用状态。".into()
                        },
                    ))
                    .when(!self.group_error.is_empty(), |el| {
                        el.child(
                            div()
                                .mt_3()
                                .text_sm()
                                .text_color(rgb(0xa33929))
                                .child(self.group_error.clone()),
                        )
                    })
                    .child(
                        div()
                            .mt_5()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(self.button(
                                "cancel-group",
                                "取消",
                                Command::Close,
                                false,
                                true,
                                cx,
                            ))
                            .child(self.button(
                                "save-group",
                                "保存",
                                Command::SaveGroup,
                                true,
                                true,
                                cx,
                            )),
                    ),
            )
    }
}

use super::*;

impl Manager {
    pub(super) fn dialog(&self, window: &Window, cx: &Context<Self>) -> Stateful<Div> {
        let addon = self
            .addons
            .iter()
            .find(|addon| Some(&addon.key) == self.detail.as_ref());
        self.overlay_backdrop("dialog-overlay", OverlayKind::Modal, cx)
            .flex()
            .justify_end()
            .bg(rgba(0x17251d33))
            .child(
                overlays::panel("dialog")
                    .w(px(560.).min(window.viewport_size().width))
                    .h_full()
                    .flex()
                    .flex_col()
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
                                    .font_weight(FontWeight::BOLD)
                                    .child("模组详情"),
                            )
                            .child(self.close_button("close-dialog", cx)),
                    )
                    .when_some(addon, |el, addon| {
                        el.child(
                            div()
                                .id(SharedString::from(format!("detail-content:{}", addon.key)))
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .p_5()
                                .flex()
                                .flex_col()
                                .gap_4()
                                .child(self.detail_summary(addon))
                                .when(self.workshop_supported(), |el| {
                                    el.child(self.workshop_details(addon, cx))
                                })
                                .when(
                                    !self.workshop_supported()
                                        && addon.metadata.description.trim().is_empty(),
                                    |el| {
                                        el.child(
                                            Self::detail_card()
                                                .text_color(rgb(MUTED))
                                                .child("暂未提供模组介绍。"),
                                        )
                                    },
                                )
                                .child(self.detail_sources(addon, cx)),
                        )
                        .child(self.detail_actions(addon, cx))
                    }),
            )
    }

    fn detail_card() -> Div {
        div()
            .flex_shrink_0()
            .min_w_0()
            .p_4()
            .rounded(px(8.))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
    }

    fn detail_section_title(label: &str) -> Div {
        div().font_weight(FontWeight::BOLD).child(label.to_owned())
    }

    fn detail_field(label: &str, value: impl Into<SharedString>) -> Div {
        div()
            .flex()
            .items_start()
            .gap_3()
            .text_xs()
            .line_height(px(20.))
            .child(
                div()
                    .w(px(72.))
                    .flex_shrink_0()
                    .text_color(rgb(MUTED))
                    .child(label.to_owned()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(value.into()),
            )
    }

    fn description_text(text: &str) -> Div {
        div()
            .min_w_0()
            .overflow_hidden()
            .text_sm()
            .line_height(px(23.))
            .children(
                text.lines()
                    .map(|line| div().min_h(px(12.)).child(line.to_owned())),
            )
    }

    fn detail_summary(&self, addon: &Addon) -> Div {
        let tags = widgets::tag_labels(addon);
        let memberships = self
            .groups
            .iter()
            .filter(|group| group.members.contains(&addon.key))
            .map(|group| group.name.as_str())
            .collect::<Vec<_>>()
            .join("、");
        Self::detail_card()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    .child(self.cover(addon, 88.).rounded(px(6.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xl()
                                    .line_height(px(28.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(
                                        if addon_wrench::metadata::has_resolved_title(addon) {
                                            TEXT
                                        } else {
                                            MUTED
                                        },
                                    ))
                                    .child(addon.name.clone()),
                            )
                            .when(!addon.metadata.author.is_empty(), |el| {
                                el.child(
                                    div()
                                        .mt_2()
                                        .text_color(rgb(MUTED))
                                        .child(format!("作者 · {}", addon.metadata.author)),
                                )
                            })
                            .child(
                                div()
                                    .mt_3()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded(px(4.))
                                            .text_xs()
                                            .bg(rgb(if self.can_manage() && addon.enabled {
                                                TINT
                                            } else {
                                                BG
                                            }))
                                            .text_color(rgb(
                                                if self.can_manage() && addon.enabled {
                                                    ACCENT
                                                } else {
                                                    MUTED
                                                },
                                            ))
                                            .child(if !self.can_manage() {
                                                "仅浏览 · 状态未知"
                                            } else if addon.enabled {
                                                "已启用"
                                            } else {
                                                "已禁用"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(addons::human_size(addon.bytes)),
                                    ),
                            ),
                    ),
            )
            .when(!addon_wrench::metadata::has_resolved_title(addon), |el| {
                el.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded(px(4.))
                        .bg(rgb(BG))
                        .text_xs()
                        .line_height(px(20.))
                        .text_color(rgb(MUTED))
                        .child("未识别到模组名称，当前显示原始文件名。")
                        .when(self.workshop_supported(), |el| {
                            el.child(div().mt_1().child(if addon.metadata.workshop_id.is_some() {
                                "可刷新工坊资料以补全名称。"
                            } else {
                                "可通过底部“工坊关联”补全名称。"
                            }))
                        }),
                )
            })
            .when(!tags.is_empty(), |el| {
                el.child(
                    div()
                        .mt_4()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(tags.into_iter().map(|tag| {
                            div()
                                .px_2()
                                .py_1()
                                .text_xs()
                                .rounded(px(4.))
                                .bg(rgb(TINT))
                                .text_color(rgb(ACCENT))
                                .child(tag.to_owned())
                        })),
                )
            })
            .child(
                div()
                    .mt_4()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(Self::detail_field("文件修改", Self::modified_label(addon)))
                    .when(!memberships.is_empty(), |el| {
                        el.child(Self::detail_field("所属分组", memberships).mt_2())
                    }),
            )
    }

    fn workshop_supported(&self) -> bool {
        self.profile
            .as_ref()
            .and_then(games::Game::workshop_app)
            .is_some()
    }

    fn workshop_details(&self, addon: &Addon, cx: &Context<Self>) -> Div {
        let loading = self.metadata_keys.contains(&addon.key);
        let ready = !self.metadata_loading && !self.busy && self.scan_error.is_none();
        let status = self.metadata_status.get(&addon.key);
        Self::detail_card()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(Self::detail_section_title("工坊简介")))
                    .when(addon.metadata.workshop_id.is_some(), |el| {
                        el.child(
                            self.styled_button(
                                "fetch-workshop-detail",
                                if loading {
                                    "获取中…"
                                } else if status.is_some() {
                                    "重试"
                                } else {
                                    "刷新"
                                },
                                Command::FetchWorkshop(addon.key.clone()),
                                ButtonStyle::Soft,
                                ready,
                                cx,
                            )
                            .h(px(28.))
                            .text_xs(),
                        )
                    }),
            )
            .when(
                !addon.metadata.workshop_title.is_empty()
                    && addon.metadata.workshop_title != addon.name,
                |el| {
                    el.child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("工坊名称 · {}", addon.metadata.workshop_title)),
                    )
                },
            )
            .when_some(addon.metadata.workshop_id.as_ref(), |el, id| {
                el.child(
                    div()
                        .mt_3()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            self.styled_button(
                                "workshop-page",
                                "在 Steam 中打开",
                                Command::WorkshopPage(id.clone()),
                                ButtonStyle::Soft,
                                true,
                                cx,
                            )
                            .h(px(30.))
                            .text_xs(),
                        )
                        .child(
                            self.button(
                                "workshop-browser",
                                "浏览器打开",
                                Command::WorkshopBrowser(id.clone()),
                                false,
                                true,
                                cx,
                            )
                            .h(px(30.))
                            .text_xs(),
                        ),
                )
            })
            .when(loading, |el| {
                el.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded(px(4.))
                        .bg(rgb(TINT))
                        .text_xs()
                        .text_color(rgb(ACCENT))
                        .child(if addon.metadata.workshop_description.is_empty() {
                            "正在获取作者的文字说明…"
                        } else {
                            "正在更新，仍可阅读已有简介…"
                        }),
                )
            })
            .when(!addon.metadata.workshop_description.is_empty(), |el| {
                el.child(Self::description_text(&addon.metadata.workshop_description).mt_4())
            })
            .when(
                addon.metadata.workshop_description.is_empty() && !loading,
                |el| {
                    el.child(
                        div()
                            .mt_3()
                            .p_3()
                            .rounded(px(4.))
                            .bg(rgb(BG))
                            .text_color(rgb(MUTED))
                            .line_height(px(22.))
                            .child(if addon.metadata.workshop_verified {
                                "作者未提供文字简介。"
                            } else if addon.metadata.workshop_id.is_some() {
                                "暂未获取到简介，可以重试或访问原页面。"
                            } else {
                                "关联工坊项目后，可在这里阅读作者的文字说明。"
                            }),
                    )
                },
            )
            .when_some(
                status.filter(|_| {
                    !self.workshop_link_editing && addon.metadata.workshop_id.is_some()
                }),
                |el, status| el.child(Self::detail_error(status)),
            )
            .when(addon.metadata.workshop_fetched > 0, |el| {
                el.child(
                    div()
                        .mt_4()
                        .pt_3()
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!(
                            "获取于 {}",
                            timestamps::format_modified(std::time::UNIX_EPOCH.checked_add(
                                std::time::Duration::from_secs(addon.metadata.workshop_fetched)
                            ))
                        )),
                )
            })
    }

    fn detail_error(message: &str) -> Div {
        div()
            .mt_3()
            .p_3()
            .rounded(px(4.))
            .bg(rgb(0xfcf0ee))
            .text_xs()
            .line_height(px(20.))
            .text_color(rgb(0xb54747))
            .child(message.to_owned())
    }

    fn workshop_association(&self, addon: &Addon, cx: &Context<Self>) -> Div {
        let ready = !self.metadata_loading && !self.busy && self.scan_error.is_none();
        div()
            .pb_3()
            .mb_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .when_some(addon.metadata.workshop_id.as_ref(), |el, id| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("当前项目 · {id}")),
                )
            })
            .child(
                div()
                    .when(addon.metadata.workshop_id.is_some(), |el| el.mt_2())
                    .text_xs()
                    .line_height(px(20.))
                    .text_color(rgb(MUTED))
                    .child(if addon.metadata.workshop_id.is_some() {
                        "粘贴正确的工坊链接或 ID，验证成功后替换当前关联。"
                    } else {
                        "粘贴工坊链接或数字 ID，验证后保存关联，无需登录。"
                    }),
            )
            .child(
                div()
                    .mt_3()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(180.))
                            .h(px(36.))
                            .px_3()
                            .flex()
                            .items_center()
                            .bg(rgb(BG))
                            .border_1()
                            .border_color(rgb(LINE))
                            .rounded(px(4.))
                            .child(self.workshop_link.clone()),
                    )
                    .child(self.styled_button(
                        "associate-workshop",
                        if self.metadata_keys.contains(&addon.key) {
                            "获取中…"
                        } else {
                            "获取并关联"
                        },
                        Command::AssociateWorkshop(addon.key.clone()),
                        ButtonStyle::Soft,
                        ready,
                        cx,
                    )),
            )
            .when_some(self.metadata_status.get(&addon.key), |el, message| {
                el.child(Self::detail_error(message))
            })
    }

    fn detail_sources(&self, addon: &Addon, cx: &Context<Self>) -> Div {
        Self::detail_card()
            .child(
                self.button(
                    "technical-details",
                    "文件与来源信息",
                    Command::TechnicalDetails,
                    false,
                    true,
                    cx,
                )
                .w_full()
                .h(px(26.))
                .px_0()
                .justify_start()
                .font_weight(FontWeight::BOLD)
                .child(div().flex_1())
                .child(div().text_color(rgb(MUTED)).child(
                    if self.technical_details {
                        "收起 −"
                    } else {
                        "展开 ＋"
                    },
                )),
            )
            .when(self.technical_details, |el| {
                el.child(
                    div()
                        .mt_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(Self::detail_field(
                            "文件名称",
                            addon
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        ))
                        .child(Self::detail_field(
                            "标题来源",
                            if !addon_wrench::metadata::has_resolved_title(addon) {
                                "文件名（名称未识别）"
                            } else if addon.metadata.workshop_verified
                                && !addon_wrench::metadata::has_local_title(addon)
                            {
                                "Steam 工坊补全"
                            } else if addon.metadata.title.is_some() {
                                "VPK 元数据"
                            } else {
                                "文件名"
                            },
                        ))
                        .when(!addon.metadata.local_tags.is_empty(), |el| {
                            el.child(Self::detail_field(
                                "文件分类",
                                addon.metadata.local_tags.join(" · "),
                            ))
                        })
                        .when(!addon.metadata.custom_tags.is_empty(), |el| {
                            el.child(Self::detail_field(
                                "自定义标签",
                                addon.metadata.custom_tags.join(" · "),
                            ))
                        })
                        .when(!addon.metadata.cover_source.is_empty(), |el| {
                            el.child(Self::detail_field(
                                "封面来源",
                                addon.metadata.cover_source.clone(),
                            ))
                        })
                        .when(!addon.metadata.note.is_empty(), |el| {
                            el.child(Self::detail_field("读取说明", addon.metadata.note.clone()))
                        })
                        .when(!addon.metadata.description.trim().is_empty(), |el| {
                            el.child(
                                div()
                                    .pt_3()
                                    .border_t_1()
                                    .border_color(rgb(LINE))
                                    .child(div().text_xs().text_color(rgb(MUTED)).child("本地简介"))
                                    .child(
                                        Self::description_text(&addon.metadata.description).mt_2(),
                                    ),
                            )
                        }),
                )
            })
    }

    fn detail_actions(&self, addon: &Addon, cx: &Context<Self>) -> Div {
        div()
            .flex_shrink_0()
            .px_5()
            .py_3()
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(LINE))
            .when(
                self.workshop_supported() && self.workshop_link_editing,
                |el| el.child(self.workshop_association(addon, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.button(
                        "reveal",
                        "在文件夹中显示",
                        Command::Reveal(addon.path.clone()),
                        false,
                        true,
                        cx,
                    ))
                    .child(self.button(
                        "copy-path",
                        "复制路径",
                        Command::CopyPath(addon.path.clone()),
                        false,
                        true,
                        cx,
                    ))
                    .when(self.workshop_supported(), |el| {
                        el.child(
                            self.button(
                                "edit-workshop-link",
                                if self.workshop_link_editing {
                                    "工坊关联 ▴"
                                } else {
                                    "工坊关联 ▾"
                                },
                                Command::EditWorkshopLink,
                                false,
                                true,
                                cx,
                            )
                            .when(self.workshop_link_editing, |el| {
                                el.bg(rgb(TINT)).text_color(rgb(ACCENT))
                            }),
                        )
                    })
                    .child(div().flex_1())
                    .when(self.can_manage(), |el| {
                        el.child(self.button(
                            "detail-toggle",
                            if addon.enabled {
                                "禁用模组"
                            } else {
                                "启用模组"
                            },
                            Command::Toggle(addon.key.clone(), !addon.enabled),
                            true,
                            true,
                            cx,
                        ))
                    }),
            )
    }
}

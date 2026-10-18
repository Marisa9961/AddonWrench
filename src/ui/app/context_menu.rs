use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ContextTarget {
    Addons {
        keys: Vec<String>,
        selection: bool,
        group: Option<usize>,
    },
    Group(usize),
    Sidebar,
}

impl ContextTarget {
    pub(super) fn addons(key: &str, checked: &HashSet<String>, group: Option<usize>) -> Self {
        let selection = checked.contains(key);
        let mut keys = if selection {
            checked.iter().cloned().collect()
        } else {
            vec![key.to_owned()]
        };
        keys.sort();
        Self::Addons {
            keys,
            selection,
            group,
        }
    }
}

pub(super) struct ContextMenu {
    target: ContextTarget,
    position: Point<Pixels>,
    pub(super) adding_to_group: bool,
}

impl ContextMenu {
    pub(super) fn targets_addon(&self, key: &str) -> bool {
        matches!(&self.target, ContextTarget::Addons { keys, .. } if keys.iter().any(|member| member == key))
    }

    pub(super) fn targets_group(&self, index: usize) -> bool {
        matches!(self.target, ContextTarget::Group(target) if target == index)
    }
}

pub(super) fn pending_group_members(group: &groups::Group, keys: &[String]) -> Vec<String> {
    keys.iter()
        .filter(|key| !group.members.contains(*key))
        .cloned()
        .collect()
}

fn menu_heading(title: String, scope: String) -> Div {
    let tooltip = title.clone();
    div()
        .px_3()
        .py_2()
        .child(
            div()
                .id("context-menu-title")
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_ellipsis()
                .overflow_hidden()
                .child(title)
                .tooltip(move |_, cx| widgets::hover_hint(tooltip.clone(), cx)),
        )
        .child(div().mt_1().text_xs().text_color(rgb(MUTED)).child(scope))
}

pub(super) fn remaining_active_group(active: Option<usize>, deleted: usize) -> Option<usize> {
    active.and_then(|active| {
        if active == deleted {
            None
        } else {
            Some(active - usize::from(active > deleted))
        }
    })
}

impl Manager {
    pub(super) fn group_join_item(
        &self,
        id: impl Into<ElementId>,
        index: usize,
        keys: &[String],
        available: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let group = &self.groups[index];
        let pending = pending_group_members(group, keys);
        let remaining = pending.len();
        let joined = keys.len() - remaining;
        let status = if remaining == 0 {
            "已加入".to_owned()
        } else if joined > 0 {
            format!("部分加入 {joined}/{}", keys.len())
        } else {
            format!("加入 {} 项", keys.len())
        };
        let hint = if !available {
            "模组列表已变化，请关闭菜单后重新操作".to_owned()
        } else if remaining == 0 {
            format!("{}\n这些模组已全部加入该分组", group.name)
        } else if joined > 0 {
            format!("{}\n将加入剩余 {remaining} 项", group.name)
        } else {
            format!("{}\n将加入 {remaining} 项", group.name)
        };
        self.menu_item(
            id,
            "",
            Command::AddKeysToGroup(index, keys.to_vec()),
            false,
            self.groups_ready && available && remaining > 0,
            cx,
        )
        .w_full()
        .when(self.groups_ready && available && remaining == 0, |el| {
            el.opacity(0.65)
        })
        .gap_2()
        .child(
            icons::leading(if remaining == 0 {
                icons::Icon::Check
            } else {
                icons::Icon::AddTo
            })
            .text_color(rgb(MUTED)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .overflow_hidden()
                .child(group.name.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(status),
        )
        .tooltip(move |_, cx| widgets::hover_hint(hint.clone(), cx))
    }

    pub(super) fn open_context_menu(
        &mut self,
        target: ContextTarget,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_open() {
            return;
        }
        self.menu = None;
        self.context_menu = Some(ContextMenu {
            target,
            position,
            adding_to_group: false,
        });
        window.focus(&self.focus);
        cx.notify();
    }

    pub(super) fn group_menu_items(&self, index: usize, cx: &Context<Self>) -> Div {
        let Some(group) = self.groups.get(index) else {
            return div();
        };
        let installed = self
            .addons
            .iter()
            .filter(|addon| group.members.contains(&addon.key))
            .count();
        let missing = group.members.len() - installed;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(menu_heading(
                group.name.clone(),
                format!("整组 {installed} 个，包含筛选隐藏成员"),
            ))
            .child(self.menu_item(
                "rename-group",
                "重命名分组",
                Command::EditGroup(Some(index)),
                false,
                self.groups_ready,
                cx,
            ))
            .when(missing > 0, |el| {
                el.child(self.menu_item(
                    "context-missing",
                    format!("查看缺失成员（{missing}）"),
                    Command::MissingGroup(index),
                    false,
                    true,
                    cx,
                ))
            })
            .child(div().h(px(1.)).my_1().bg(rgb(LINE)))
            .child(self.menu_item(
                "delete-group",
                "删除分组（保留模组）",
                Command::DeleteGroup(index),
                false,
                self.groups_ready,
                cx,
            ))
    }

    fn addon_menu_items(
        &self,
        keys: &[String],
        selection: bool,
        group: Option<usize>,
        adding: bool,
        cx: &Context<Self>,
    ) -> Div {
        let targets: HashSet<_> = keys.iter().map(String::as_str).collect();
        let addons: Vec<_> = self
            .addons
            .iter()
            .filter(|addon| targets.contains(addon.key.as_str()))
            .collect();
        let available = !keys.is_empty() && addons.len() == keys.len();
        let title = if selection {
            format!("操作已选 {} 项", keys.len())
        } else {
            addons
                .first()
                .map(|addon| addon.name.clone())
                .unwrap_or_else(|| "模组已不在列表中".into())
        };
        let scope = if !available {
            "模组列表已变化，请关闭菜单后重新操作".to_owned()
        } else if selection {
            "以下操作作用于全部勾选项".to_owned()
        } else if !self.browser.checked.is_empty() {
            format!(
                "仅操作此模组 · 另有 {} 项已勾选",
                self.browser.checked.len()
            )
        } else {
            "仅操作此模组".to_owned()
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(menu_heading(title, scope))
            .when(adding, |el| {
                el.child(self.menu_item(
                    "context-back",
                    "返回模组操作",
                    Command::ContextBack,
                    false,
                    true,
                    cx,
                ))
                .children(self.groups.iter().enumerate().map(|(index, _)| {
                    self.group_join_item(("context-join", index), index, keys, available, cx)
                }))
                .when(self.groups.is_empty(), |el| {
                    el.child(
                        div()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("还没有分组，可在下方新建。"),
                    )
                })
                .child(div().h(px(1.)).my_1().bg(rgb(LINE)))
                .child(self.menu_item(
                    "context-create",
                    format!("新建分组并加入这 {} 项", keys.len()),
                    Command::CreateGroupWith(keys.to_vec()),
                    false,
                    self.groups_ready && available,
                    cx,
                ))
            })
            .when(!adding, |el| {
                el.when(keys.len() == 1 && available, |el| {
                    let Some(addon) = addons.first() else {
                        return el;
                    };
                    el.child(self.menu_item(
                        "context-detail",
                        "查看详情",
                        Command::Detail(keys[0].clone()),
                        false,
                        true,
                        cx,
                    ))
                    .child(self.menu_item(
                        "context-reveal",
                        "在文件夹中显示",
                        Command::Reveal(addon.path.clone()),
                        false,
                        true,
                        cx,
                    ))
                    .child(self.menu_item(
                        "context-copy",
                        "复制路径",
                        Command::CopyPath(addon.path.clone()),
                        false,
                        true,
                        cx,
                    ))
                    .child(div().h(px(1.)).my_1().bg(rgb(LINE)))
                })
                .child(self.menu_item(
                    "context-add",
                    "加入分组 ›",
                    Command::ContextAddToGroup,
                    false,
                    self.groups_ready && available,
                    cx,
                ))
                .when_some(group, |el, index| {
                    let Some(group) = self.groups.get(index) else {
                        return el;
                    };
                    let label = if keys.len() > 1 {
                        format!("移出「{}」（{} 项）", group.name, keys.len())
                    } else {
                        format!("移出「{}」", group.name)
                    };
                    let hint = format!("{label}\n仅移除分组成员关系，保留模组文件和启用状态");
                    el.child(
                        self.menu_item(
                            "context-remove",
                            label,
                            Command::RemoveKeysFromGroup(index, keys.to_vec()),
                            false,
                            self.groups_ready && available,
                            cx,
                        )
                        .tooltip(move |_, cx| widgets::hover_hint(hint.clone(), cx)),
                    )
                })
                .when(selection, |el| {
                    el.child(div().h(px(1.)).my_1().bg(rgb(LINE)))
                        .child(self.menu_item(
                            "context-clear",
                            format!("取消勾选（{} 项）", keys.len()),
                            Command::ClearSelection,
                            false,
                            true,
                            cx,
                        ))
                })
            })
    }

    pub(super) fn context_menu_overlay(
        &self,
        menu: &ContextMenu,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let viewport = window.viewport_size();
        let panel = overlays::panel("context-menu")
            .w(px(260.))
            .max_w((viewport.width - px(16.)).max(px(0.)))
            .max_h((viewport.height - px(16.)).max(px(0.)).min(px(420.)))
            .overflow_y_scroll()
            .p_2()
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .shadow_lg()
            .child(match &menu.target {
                ContextTarget::Group(index) => self.group_menu_items(*index, cx),
                ContextTarget::Sidebar => div().child(self.menu_item(
                    "context-new-group",
                    "新建分组",
                    Command::CreateEmptyGroup,
                    false,
                    self.groups_ready && self.game.is_some(),
                    cx,
                )),
                ContextTarget::Addons {
                    keys,
                    selection,
                    group,
                } => self.addon_menu_items(keys, *selection, *group, menu.adding_to_group, cx),
            });
        deferred(
            self.overlay_backdrop("context-menu-shield", OverlayKind::Menu, cx)
                .child(
                    anchored()
                        .position(menu.position)
                        .anchor(Corner::TopLeft)
                        .snap_to_window_with_margin(px(8.))
                        .child(panel),
                ),
        )
        .with_priority(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_click_on_unchecked_addon_preserves_existing_selection_and_targets_only_that_addon() {
        let checked = HashSet::from(["a.vpk".into(), "b.vpk".into()]);
        assert_eq!(
            ContextTarget::addons("c.vpk", &checked, Some(2)),
            ContextTarget::Addons {
                keys: vec!["c.vpk".into()],
                selection: false,
                group: Some(2),
            }
        );
        assert_eq!(checked.len(), 2);
    }

    #[test]
    fn right_click_on_checked_addon_snapshots_the_entire_selection() {
        let mut checked = HashSet::from(["b.vpk".into(), "a.vpk".into()]);
        let target = ContextTarget::addons("a.vpk", &checked, None);
        checked.clear();
        assert_eq!(
            target,
            ContextTarget::Addons {
                keys: vec!["a.vpk".into(), "b.vpk".into()],
                selection: true,
                group: None,
            }
        );
    }

    #[test]
    fn deleting_another_group_keeps_the_current_group_selected() {
        assert_eq!(remaining_active_group(Some(3), 1), Some(2));
        assert_eq!(remaining_active_group(Some(1), 3), Some(1));
        assert_eq!(remaining_active_group(Some(1), 1), None);
        assert_eq!(remaining_active_group(None, 1), None);
    }
}

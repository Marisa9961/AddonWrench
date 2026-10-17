use super::*;

fn unique(tags: &mut Vec<String>) {
    tags.sort_by_key(|tag| tag.to_lowercase());
    tags.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
}

impl Manager {
    fn tag_section(
        &self,
        id: &'static str,
        label: &str,
        tags: &[String],
        scope: &[&Addon],
        cx: &Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_3()
                    .pt_3()
                    .pb_1()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(label.to_owned()),
            )
            .children(tags.iter().enumerate().map(|(index, tag)| {
                let count = scope
                    .iter()
                    .filter(|addon| addon.metadata.has_tag(tag))
                    .count();
                let selected = self
                    .browser
                    .tag
                    .as_ref()
                    .is_some_and(|active| active.to_lowercase() == tag.to_lowercase());
                self.menu_item(
                    (id, index),
                    format!("{}{}  {count}", if selected { "✓ " } else { "" }, tag),
                    Command::Tag(Some(tag.clone())),
                    false,
                    true,
                    cx,
                )
                .when(selected, |el| el.bg(rgb(TINT)).text_color(rgb(ACCENT)))
            }))
    }

    pub(super) fn tag_menu_items(&self, cx: &Context<Self>) -> Div {
        let scope: Vec<_> = self
            .addons
            .iter()
            .filter(|addon| {
                self.browser
                    .members
                    .as_ref()
                    .is_none_or(|members| members.contains(&addon.key))
            })
            .collect();
        let mut categories: Vec<_> = scope
            .iter()
            .flat_map(|addon| addon.metadata.local_tags.iter().cloned())
            .collect();
        let mut tags: Vec<_> = scope
            .iter()
            .flat_map(|addon| {
                addon
                    .metadata
                    .custom_tags
                    .iter()
                    .chain(addon.metadata.workshop_tags.iter())
                    .cloned()
            })
            .collect();
        unique(&mut categories);
        unique(&mut tags);
        tags.retain(|tag| {
            !categories
                .iter()
                .any(|category| category.to_lowercase() == tag.to_lowercase())
        });
        let no_tags = scope
            .iter()
            .filter(|addon| addon.metadata.tags().next().is_none())
            .count();
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("当前范围的标签 · 包含作者自定义标签"),
            )
            .child(self.menu_item(
                "all-tags",
                if self.browser.tag.is_none() {
                    "✓ 全部标签"
                } else {
                    "全部标签"
                },
                Command::Tag(None),
                false,
                true,
                cx,
            ))
            .child(self.menu_item(
                "no-tags",
                format!(
                    "{}未提供标签  {no_tags}",
                    if self.browser.tag.as_deref() == Some("") {
                        "✓ "
                    } else {
                        ""
                    }
                ),
                Command::Tag(Some(String::new())),
                false,
                true,
                cx,
            ))
            .when(!categories.is_empty(), |el| {
                el.child(self.tag_section(
                    "category-tag",
                    "内容分类 · VPK",
                    &categories,
                    &scope,
                    cx,
                ))
            })
            .when(!tags.is_empty(), |el| {
                el.child(self.tag_section(
                    "custom-tag",
                    "模组标签 · 作者 / 工坊",
                    &tags,
                    &scope,
                    cx,
                ))
            })
            .when(categories.is_empty() && tags.is_empty(), |el| {
                el.child(
                    div()
                        .p_3()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("没有读取到标签；工坊模组可在刷新菜单中更新资料。"),
                )
            })
    }
}

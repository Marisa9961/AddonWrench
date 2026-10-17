//! Search, ordering and explicit batch selection, independent of the GUI.
use crate::addons::Addon;
use std::collections::HashSet;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Source {
    #[default]
    All,
    Local,
    Workshop,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Status {
    #[default]
    All,
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Sort {
    #[default]
    Name,
    Size,
    NameDescending,
    SizeAscending,
    ModifiedNewest,
    ModifiedOldest,
    Author,
    AuthorDescending,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortColumn {
    Name,
    Author,
    Size,
    Modified,
}

impl Sort {
    pub fn column(self) -> SortColumn {
        match self {
            Self::Name | Self::NameDescending => SortColumn::Name,
            Self::Author | Self::AuthorDescending => SortColumn::Author,
            Self::Size | Self::SizeAscending => SortColumn::Size,
            Self::ModifiedNewest | Self::ModifiedOldest => SortColumn::Modified,
        }
    }

    pub fn ascending(self) -> bool {
        matches!(
            self,
            Self::Name | Self::Author | Self::SizeAscending | Self::ModifiedOldest
        )
    }

    pub fn click_column(self, column: SortColumn) -> Self {
        match column {
            SortColumn::Name => {
                if self == Self::Name {
                    Self::NameDescending
                } else {
                    Self::Name
                }
            }
            SortColumn::Author => {
                if self == Self::Author {
                    Self::AuthorDescending
                } else {
                    Self::Author
                }
            }
            SortColumn::Size => {
                if self == Self::Size {
                    Self::SizeAscending
                } else {
                    Self::Size
                }
            }
            SortColumn::Modified => {
                if self == Self::ModifiedNewest {
                    Self::ModifiedOldest
                } else {
                    Self::ModifiedNewest
                }
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "名称 A → Z",
            Self::NameDescending => "名称 Z → A",
            Self::Size => "大小：从大到小",
            Self::SizeAscending => "大小：从小到大",
            Self::ModifiedNewest => "文件修改：最近优先",
            Self::ModifiedOldest => "文件修改：最早优先",
            Self::Author => "作者 A → Z",
            Self::AuthorDescending => "作者 Z → A",
        }
    }
}

#[derive(Default)]
pub struct Browser {
    pub members: Option<HashSet<String>>,
    pub query: String,
    pub tag: Option<String>,
    pub source: Source,
    pub status: Status,
    pub sort: Sort,
    pub checked: HashSet<String>,
}

impl Browser {
    pub fn matches(&self, addon: &Addon) -> bool {
        let source = match self.source {
            Source::All => true,
            Source::Local => !addon.workshop,
            Source::Workshop => addon.workshop,
        };
        let status = match self.status {
            Status::All => true,
            Status::Enabled => addon.enabled,
            Status::Disabled => !addon.enabled,
        };
        let name = format!(
            "{} {} {} {}",
            addon.name,
            addon.key,
            addon.metadata.author,
            addon.metadata.tags().cloned().collect::<Vec<_>>().join(" ")
        )
        .to_lowercase();
        source
            && self
                .members
                .as_ref()
                .is_none_or(|members| members.contains(&addon.key))
            && self.tag.as_ref().is_none_or(|tag| {
                if tag.is_empty() {
                    addon.metadata.tags().next().is_none()
                } else {
                    addon.metadata.has_tag(tag)
                }
            })
            && status
            && self
                .query
                .split_whitespace()
                .all(|word| name.contains(&word.to_lowercase()))
    }

    pub fn visible<'a>(&self, addons: &'a [Addon]) -> Vec<&'a Addon> {
        let mut result: Vec<_> = addons.iter().filter(|a| self.matches(a)).collect();
        result.sort_by(|a, b| {
            let name = || {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then(a.key.cmp(&b.key))
            };
            match self.sort {
                Sort::Name => name(),
                Sort::NameDescending => b
                    .name
                    .to_lowercase()
                    .cmp(&a.name.to_lowercase())
                    .then(a.key.cmp(&b.key)),
                Sort::Size => b.bytes.cmp(&a.bytes).then_with(name),
                Sort::SizeAscending => a.bytes.cmp(&b.bytes).then_with(name),
                Sort::ModifiedNewest | Sort::ModifiedOldest => match (a.modified, b.modified) {
                    (Some(a), Some(b)) => {
                        if self.sort == Sort::ModifiedNewest {
                            b.cmp(&a)
                        } else {
                            a.cmp(&b)
                        }
                    }
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then_with(name),
                Sort::Author | Sort::AuthorDescending => {
                    let author_a = a.metadata.author.trim().to_lowercase();
                    let author_b = b.metadata.author.trim().to_lowercase();
                    author_a
                        .is_empty()
                        .cmp(&author_b.is_empty())
                        .then_with(|| {
                            if self.sort == Sort::Author {
                                author_a.cmp(&author_b)
                            } else {
                                author_b.cmp(&author_a)
                            }
                        })
                        .then_with(name)
                }
            }
        });
        result
    }

    /// Hidden selections must never be silently included in a batch write.
    pub fn reconcile(&mut self, addons: &[Addon]) {
        let visible: HashSet<_> = addons
            .iter()
            .filter(|a| self.matches(a))
            .map(|a| a.key.clone())
            .collect();
        self.checked.retain(|key| visible.contains(key));
    }

    pub fn toggle_all(&mut self, addons: &[Addon]) {
        let keys: HashSet<_> = self.visible(addons).iter().map(|a| a.key.clone()).collect();
        if keys.iter().all(|key| self.checked.contains(key)) {
            self.checked.clear();
        } else {
            self.checked = keys;
        }
    }

    /// Clear only the completed action's selection; failures and unrelated new selections survive.
    pub fn complete_action(&mut self, keys: &[String], succeeded: bool) {
        if succeeded {
            self.checked.retain(|key| !keys.contains(key));
        }
    }

    pub fn changes(&self, addons: &[Addon], enabled: bool) -> Vec<(String, bool)> {
        addons
            .iter()
            .filter(|a| self.matches(a) && self.checked.contains(&a.key) && a.enabled != enabled)
            .map(|a| (a.key.clone(), enabled))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_clicks_choose_a_field_then_reverse_its_direction() {
        let initial = Sort::Name;
        assert!(initial.ascending());
        assert_eq!(initial.click_column(SortColumn::Name), Sort::NameDescending);
        assert_eq!(
            Sort::NameDescending.click_column(SortColumn::Name),
            Sort::Name
        );
        for (column, first, second) in [
            (SortColumn::Size, Sort::Size, Sort::SizeAscending),
            (SortColumn::Author, Sort::Author, Sort::AuthorDescending),
            (
                SortColumn::Modified,
                Sort::ModifiedNewest,
                Sort::ModifiedOldest,
            ),
        ] {
            assert_eq!(initial.click_column(column), first);
            assert_eq!(first.column(), column);
            assert_eq!(first.click_column(column), second);
            assert_eq!(second.click_column(column), first);
            assert_ne!(first.ascending(), second.ascending());
        }
        assert_eq!(
            Sort::SizeAscending.click_column(SortColumn::Name),
            Sort::Name
        );
    }

    #[test]
    fn all_sort_choices_handle_direction_and_missing_values() {
        use std::time::{Duration, UNIX_EPOCH};
        let mut items = addons();
        for (index, (name, filename, author)) in [
            ("Alpha", "z.vpk", "B"),
            ("beta", "m.vpk", "a"),
            ("Gamma", "a.vpk", ""),
        ]
        .into_iter()
        .enumerate()
        {
            items[index].name = name.into();
            items[index].path = filename.into();
            items[index].metadata.author = author.into();
        }
        items[0].modified = Some(UNIX_EPOCH + Duration::from_secs(20));
        items[1].modified = Some(UNIX_EPOCH + Duration::from_secs(10));
        let cases = [
            (Sort::Name, [0, 1, 2]),
            (Sort::NameDescending, [2, 1, 0]),
            (Sort::Size, [1, 2, 0]),
            (Sort::SizeAscending, [0, 2, 1]),
            (Sort::ModifiedNewest, [0, 1, 2]),
            (Sort::ModifiedOldest, [1, 0, 2]),
            (Sort::Author, [1, 0, 2]),
            (Sort::AuthorDescending, [0, 1, 2]),
        ];
        for (sort, expected) in cases {
            let browser = Browser {
                sort,
                ..Default::default()
            };
            let actual: Vec<_> = browser
                .visible(&items)
                .into_iter()
                .map(|addon| addon.key.as_str())
                .collect();
            assert_eq!(
                actual,
                expected.map(|index| items[index].key.as_str()),
                "{}",
                sort.label()
            );
        }
    }

    #[test]
    fn author_tags_are_searchable_filterable_and_clear_hidden_selections() {
        let mut items = addons();
        items[0].metadata.custom_tags = vec!["自定义主题".into(), "Ästhetik".into()];
        let mut browser = Browser::default();
        browser.toggle_all(&items);
        browser.tag = Some("ästhetik".into());
        browser.reconcile(&items);
        assert_eq!(browser.checked, HashSet::from([items[0].key.clone()]));
        browser.tag = None;
        browser.query = "自定义主题".into();
        assert_eq!(browser.visible(&items).len(), 1);
        assert_eq!(browser.visible(&items)[0].key, items[0].key);
    }

    #[test]
    fn successful_action_clears_only_its_selection_and_keeps_filters() {
        let mut browser = Browser {
            query: "武器".into(),
            checked: HashSet::from(["a".into(), "b".into(), "new".into()]),
            ..Default::default()
        };
        browser.complete_action(&["a".into(), "b".into()], true);
        assert_eq!(browser.checked, HashSet::from(["new".into()]));
        assert_eq!(browser.query, "武器");
    }

    #[test]
    fn failed_action_and_unselected_context_target_preserve_selection() {
        let mut browser = Browser {
            checked: HashSet::from(["selected".into()]),
            ..Default::default()
        };
        browser.complete_action(&["selected".into()], false);
        assert!(browser.checked.contains("selected"));
        browser.complete_action(&["other".into()], true);
        assert_eq!(browser.checked, HashSet::from(["selected".into()]));
    }

    fn addons() -> Vec<Addon> {
        [
            ("local/a.vpk", "武器 Alpha", false, true, 20),
            ("local/b.vpk", "Beta", false, false, 40),
            ("workshop/123.vpk", "武器 alpha", true, false, 30),
        ]
        .into_iter()
        .map(|(key, name, workshop, enabled, bytes)| Addon {
            key: key.into(),
            name: name.into(),
            path: key.into(),
            workshop,
            enabled,
            bytes,
            modified: None,
            metadata: Default::default(),
        })
        .collect()
    }
    #[test]
    fn group_switch_clears_hidden_selection_and_filters_by_membership() {
        let items = addons();
        let mut browser = Browser::default();
        browser.toggle_all(&items);
        browser.members = Some([items[0].key.clone()].into());
        browser.reconcile(&items);
        assert_eq!(browser.checked.len(), 1);
        assert_eq!(browser.visible(&items).len(), 1);
        assert_eq!(
            browser.changes(&items, false),
            [(items[0].key.clone(), false)]
        );
        browser.members = Some(HashSet::new());
        browser.reconcile(&items);
        assert!(browser.checked.is_empty());
        assert!(browser.visible(&items).is_empty());
    }

    #[test]
    fn search_intersects_source_and_status() {
        let items = addons();
        let mut browser = Browser {
            query: "  武器 ALPHA  ".into(),
            ..Default::default()
        };
        assert_eq!(browser.visible(&items).len(), 2);
        browser.source = Source::Workshop;
        browser.status = Status::Disabled;
        assert_eq!(browser.visible(&items)[0].key, "workshop/123.vpk");
        browser.query = "123.vpk".into();
        assert_eq!(browser.visible(&items).len(), 1);
        browser.status = Status::Enabled;
        assert!(browser.visible(&items).is_empty());
    }
    #[test]
    fn batch_only_changes_explicit_visible_selection() {
        let items = addons();
        let mut browser = Browser::default();
        assert!(browser.changes(&items, true).is_empty());
        browser
            .checked
            .extend(["local/a.vpk".into(), "local/b.vpk".into()]);
        assert_eq!(
            browser.changes(&items, true),
            [("local/b.vpk".into(), true)]
        );
        browser.source = Source::Workshop;
        assert!(browser.changes(&items, true).is_empty());
        browser.reconcile(&items);
        assert!(browser.checked.is_empty());
    }
    #[test]
    fn select_all_and_refresh_remove_stale_selection() {
        let mut items = addons();
        let mut browser = Browser {
            source: Source::Local,
            ..Default::default()
        };
        browser.toggle_all(&items);
        assert_eq!(browser.checked.len(), 2);
        browser.toggle_all(&items);
        assert!(browser.checked.is_empty());
        browser.toggle_all(&items);
        items.remove(0);
        browser.reconcile(&items);
        assert_eq!(browser.checked.len(), 1);
        browser.status = Status::Enabled;
        browser.reconcile(&items);
        assert!(browser.checked.is_empty());
    }
    #[test]
    fn size_sort_is_descending_with_deterministic_ties() {
        let items = addons();
        let browser = Browser {
            sort: Sort::Size,
            ..Default::default()
        };
        assert_eq!(
            browser
                .visible(&items)
                .iter()
                .map(|a| a.bytes)
                .collect::<Vec<_>>(),
            [40, 30, 20]
        );
    }

    #[test]
    fn tags_are_searchable_and_filtering_clears_hidden_selection() {
        let mut items = addons();
        items[0].metadata.local_tags = vec!["武器".into()];
        items[1].metadata.workshop_tags = vec!["Weapons".into(), "Rifles".into()];
        items[1].metadata.author = "Mod Author".into();
        let mut browser = Browser {
            query: "author rifles".into(),
            ..Default::default()
        };
        assert_eq!(browser.visible(&items)[0].key, "local/b.vpk");
        browser.query.clear();
        browser.toggle_all(&items);
        browser.tag = Some("weapons".into());
        browser.reconcile(&items);
        assert_eq!(browser.checked.len(), 1);
        browser.tag = Some(String::new());
        assert_eq!(browser.visible(&items)[0].key, "workshop/123.vpk");
        browser.reconcile(&items);
        assert!(browser.checked.is_empty());
    }
}

use super::*;

impl Manager {
    pub(super) fn command(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.column_resize.is_some() {
            return;
        }
        if self.page != Page::Addons && !command.allowed_in_settings() {
            return;
        }
        if self.group_editor.is_some() && !matches!(command, Command::Close | Command::SaveGroup) {
            return;
        }
        if self.group_toggle.is_some()
            && !matches!(command, Command::Close | Command::ConfirmGroupToggle)
        {
            return;
        }
        if self.missing_group.is_some()
            && !matches!(
                command,
                Command::Close | Command::Reveal(_) | Command::ClearMissingGroup
            )
        {
            return;
        }
        if self.detail.is_some() && !command.allowed_in_details(self.detail.as_deref()) {
            return;
        }
        if self.busy && !command.available_while_busy() {
            return;
        }
        if !matches!(command, Command::ContextAddToGroup | Command::ContextBack) {
            self.context_menu = None;
        }
        if !matches!(command, Command::Menu(_)) {
            self.menu = None;
        }
        if matches!(
            command,
            Command::Import
                | Command::Toggle(_, _)
                | Command::Bulk(_)
                | Command::GroupToggle(_, _)
                | Command::ConfirmGroupToggle
        ) && !self.can_write()
        {
            return;
        }
        if matches!(command, Command::Status(Status::Enabled | Status::Disabled))
            && !self.can_manage()
        {
            return;
        }
        match command {
            Command::ClearMissingGroup => {
                if self.groups_ready
                    && self.scan_error.is_none()
                    && let Some(index) = self
                        .missing_group
                        .filter(|index| *index < self.groups.len())
                {
                    let mut updated = self.groups.clone();
                    updated[index]
                        .members
                        .retain(|key| self.addons.iter().any(|addon| &addon.key == key));
                    if self.persist_groups(updated) {
                        self.missing_group = None;
                    }
                }
            }
            Command::ResetColumnWidths => self.reset_column_widths(),
            Command::ToggleColumn(column) => {
                let mut updated = self.preferences;
                let sort_column = match column {
                    OptionalColumn::Author => {
                        updated.author_column = !updated.author_column;
                        SortColumn::Author
                    }
                    OptionalColumn::Modified => {
                        updated.modified_column = !updated.modified_column;
                        SortColumn::Modified
                    }
                };
                match updated.save(&discovery::data_dir().join("preferences.json")) {
                    Ok(()) => {
                        let hidden = match column {
                            OptionalColumn::Author => !updated.author_column,
                            OptionalColumn::Modified => !updated.modified_column,
                        };
                        if hidden && self.browser.sort.column() == sort_column {
                            self.browser.sort = Sort::Name;
                        }
                        self.preferences = updated;
                        self.preferences_error = None;
                    }
                    Err(error) => self.preferences_error = Some(error),
                }
            }
            Command::Browse(group, status) => {
                if status != Status::All && !self.can_manage() {
                    return;
                }
                self.page = Page::Addons;
                self.active_group = group.filter(|index| *index < self.groups.len());
                self.browser = Browser::default();
                self.browser.status = status;
                self.search.update(cx, |input, cx| input.clear(cx));
                self.detail = None;
                self.reconcile();
            }
            Command::CreateEmptyGroup => {
                self.command(Command::CreateGroupWith(Vec::new()), cx);
            }
            Command::CreateGroupWith(keys) => {
                self.command(Command::EditGroup(None), cx);
                self.new_group_members = keys;
                self.new_group_stay = true;
            }
            Command::ContextAddToGroup => {
                if let Some(menu) = &mut self.context_menu {
                    menu.adding_to_group = true;
                }
            }
            Command::ContextBack => {
                if let Some(menu) = &mut self.context_menu {
                    menu.adding_to_group = false;
                }
            }
            Command::AddKeysToGroup(index, keys) => {
                let mut groups = self.groups.clone();
                if let Some(group) = groups.get_mut(index) {
                    let pending = context_menu::pending_group_members(group, &keys);
                    if !pending.is_empty() {
                        group.members.extend(pending);
                        let saved = self.persist_groups(groups);
                        self.browser.complete_action(&keys, saved);
                    }
                }
            }
            Command::RemoveKeysFromGroup(index, keys) => {
                let mut groups = self.groups.clone();
                if let Some(group) = groups.get_mut(index) {
                    group.members.retain(|key| !keys.contains(key));
                    if self.persist_groups(groups) {
                        self.browser.complete_action(&keys, true);
                        self.reconcile();
                    }
                }
            }
            Command::SwitchGame(path) => {
                self.page = Page::Addons;
                self.job(cx, move || {
                    let addons = games::scan_path(&path)?;
                    Ok(Snapshot {
                        scan_error: None,
                        game: Some(path),
                        addons,
                        message: String::new(),
                        catalog: None,
                        profile: None,
                    })
                });
            }
            Command::EditWorkshopLink => self.workshop_link_editing = !self.workshop_link_editing,
            Command::TechnicalDetails => self.technical_details = !self.technical_details,
            Command::MissingGroup(index) => {
                if self.groups.get(index).is_some() {
                    self.missing_group = Some(index);
                }
            }
            Command::ConfirmGroupToggle => {
                if let Some((index, enabled)) = self.group_toggle.take()
                    && let Some(group) = self.groups.get(index)
                {
                    self.change(group.changes(&self.addons, enabled), cx);
                }
            }

            Command::Navigate(page) => {
                self.page = page;
                self.detail = None;
            }
            Command::EditGroup(index) => {
                if !self.groups_ready || index.is_some_and(|index| index >= self.groups.len()) {
                    return;
                }
                self.new_group_members = self.browser.checked.iter().cloned().collect();
                self.new_group_stay = false;
                let name = index
                    .and_then(|i| self.groups.get(i))
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                self.group_name
                    .update(cx, |input, cx| input.set_text(name, cx));
                self.group_editor = Some(index);
                self.group_error.clear();
            }
            Command::SaveGroup => {
                if let Some(index) = self.group_editor {
                    match groups::validate_name(
                        self.group_name.read(cx).value(),
                        &self.groups,
                        index,
                    ) {
                        Ok(name) => {
                            let mut groups = self.groups.clone();
                            let target = index.unwrap_or(groups.len());
                            if let Some(index) = index {
                                groups[index].name = name;
                            } else {
                                groups.push(groups::Group {
                                    name,
                                    members: self.new_group_members.iter().cloned().collect(),
                                });
                            }
                            if self.persist_groups(groups) {
                                self.group_editor = None;
                                if index.is_none() {
                                    self.browser.complete_action(&self.new_group_members, true);
                                }
                                if index.is_none() && !self.new_group_stay {
                                    self.active_group = Some(target);
                                    self.browser = Browser::default();
                                    self.search.update(cx, |input, cx| input.clear(cx));
                                }
                                self.reconcile();
                            } else {
                                self.group_error = self.message.clone();
                            }
                        }
                        Err(e) => self.group_error = e,
                    }
                }
            }
            Command::RemoveFromGroup => {
                if let Some(index) = self.active_group {
                    let mut groups = self.groups.clone();
                    groups[index]
                        .members
                        .retain(|key| !self.browser.checked.contains(key));
                    if self.persist_groups(groups) {
                        self.browser.checked.clear();
                        self.reconcile();
                    }
                }
            }
            Command::DeleteGroup(index) => {
                if index < self.groups.len() {
                    let mut groups = self.groups.clone();
                    groups.remove(index);
                    if self.persist_groups(groups) {
                        self.active_group =
                            context_menu::remaining_active_group(self.active_group, index);
                        self.reconcile();
                    }
                }
            }
            Command::GroupToggle(index, enabled) => {
                if !self.groups_ready {
                    return;
                }
                if let Some(group) = self.groups.get(index) {
                    let changes = group.changes(&self.addons, enabled);
                    if !changes.is_empty() {
                        if groups::affected_groups(&self.groups, index, &changes).is_empty() {
                            self.change(changes, cx);
                        } else {
                            self.group_toggle = Some((index, enabled));
                        }
                    }
                }
            }
            Command::Tag(tag) => {
                self.browser.tag = tag;
                self.reconcile();
            }
            Command::FetchWorkshop(key) => self.start_metadata(key, None, cx),
            Command::AssociateWorkshop(key) => {
                if let Some(id) =
                    addon_wrench::metadata::workshop_id(self.workshop_link.read(cx).value())
                {
                    self.start_metadata(key, Some(id), cx);
                } else {
                    self.metadata_status
                        .insert(key, "请输入有效的 Steam 工坊链接或数字 ID".into());
                }
            }
            Command::WorkshopBrowser(id) => {
                if workshop::valid_id(&id) {
                    cx.open_url(&format!(
                        "https://steamcommunity.com/sharedfiles/filedetails/?id={id}"
                    ));
                }
            }
            Command::WorkshopPage(id) => {
                if workshop::valid_id(&id) {
                    cx.open_url(&format!("steam://url/CommunityFilePage/{id}"));
                }
            }
            Command::LaunchGame => {
                if let Some((url, name)) = self
                    .profile
                    .as_ref()
                    .and_then(|game| game.launch_url().map(|url| (url, game.name.clone())))
                {
                    cx.open_url(&url);
                    self.error = false;
                    self.set_message(format!("已向 Steam 发送启动请求：{name}"));
                }
            }
            Command::Choose => self.pick(false, cx),
            Command::Import => self.pick(true, cx),
            Command::Folder => {
                if let Some(game) = &self.game {
                    let folder = [game.join("addons"), game.join("custom"), game.clone()]
                        .into_iter()
                        .find(|path| path.is_dir())
                        .unwrap_or_else(|| game.clone());
                    cx.open_with_system(&folder);
                }
            }
            Command::DataFolder => cx.open_with_system(&discovery::data_dir()),
            Command::Refresh => {
                self.refresh_mods(false, cx);
            }
            Command::Toggle(key, enabled) => self.change(vec![(key, enabled)], cx),
            Command::Bulk(enabled) => {
                let selection = self.browser.checked.iter().cloned().collect();
                self.change_selected(self.browser.changes(&self.addons, enabled), selection, cx);
            }
            Command::Status(status) => {
                self.browser.status = status;
                self.reconcile();
            }
            Command::Sort(sort) => self.browser.sort = sort,
            Command::Check(key) => {
                if !self.browser.checked.remove(&key) {
                    self.browser.checked.insert(key);
                }
            }
            Command::CheckAll => self.browser.toggle_all(&self.addons),
            Command::ClearSelection => self.browser.checked.clear(),
            Command::ClearSearch => {
                self.browser.query.clear();
                self.search.update(cx, |input, cx| input.clear(cx));
                self.reconcile();
            }
            Command::ResetFilters => {
                self.browser = Browser::default();
                self.reconcile();
                self.search.update(cx, |input, cx| input.clear(cx));
            }
            Command::Detail(key) => {
                self.workshop_link.update(cx, |input, cx| input.clear(cx));
                self.detail = Some(key.clone());
                self.workshop_link_editing = false;
                self.technical_details = false;
                let should_fetch = self
                    .addons
                    .iter()
                    .find(|addon| addon.key == key)
                    .is_some_and(|addon| {
                        addon
                            .metadata
                            .workshop_id
                            .as_ref()
                            .is_some_and(|id| !self.metadata_attempted.contains(id))
                            && workshop::needs_refresh(addon.metadata.workshop_fetched)
                    });
                if should_fetch {
                    self.start_metadata(key, None, cx);
                }
            }
            Command::Close => {
                self.page = Page::Addons;
                self.menu = None;
                self.detail = None;
                self.group_editor = None;
                self.group_toggle = None;
                self.missing_group = None;
            }
            Command::Menu(menu) => {
                self.menu = if self.menu == Some(menu) {
                    None
                } else {
                    Some(menu)
                };
            }
            Command::Reveal(path) => cx.reveal_path(&path),
            Command::CopyPath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
                self.error = false;
                self.set_message("文件路径已复制".into());
            }
            Command::DismissMessage => self.dismiss_notice(cx),
        }
        cx.notify();
    }
}

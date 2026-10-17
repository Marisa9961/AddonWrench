use super::*;

impl Manager {
    pub(super) fn start_metadata(
        &mut self,
        key: String,
        association: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.metadata_loading || self.busy || self.scan_error.is_some() {
            return;
        }
        let Some(app_id) = self.profile.as_ref().and_then(games::Game::workshop_app) else {
            return;
        };
        let directory = workshop::cache_dir();
        let mut addons = self
            .addons
            .iter()
            .filter(|addon| addon.key == key)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(id) = &association {
            for addon in &mut addons {
                if addon.metadata.workshop_id.as_ref() != Some(id) {
                    workshop::clear_online_cover(addon);
                }
                addon.metadata.workshop_id = Some(id.clone());
            }
        }
        let ids = addons
            .iter()
            .filter_map(|addon| addon.metadata.workshop_id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            self.set_message("未识别到工坊项目，请先关联工坊链接".into());
            return;
        }
        addons.retain(|addon| {
            addon
                .metadata
                .workshop_id
                .as_ref()
                .is_some_and(|id| ids.contains(id))
        });
        self.metadata_attempted.extend(ids.iter().cloned());
        self.metadata_keys = addons.iter().map(|addon| addon.key.clone()).collect();
        for key in &self.metadata_keys {
            self.metadata_status.remove(key);
        }
        self.metadata_loading = true;
        self.error = false;
        let keys = self.metadata_keys.clone();
        let game = self.game.clone();
        let saving_association = association.is_some();
        let task = cx.background_executor().spawn(async move {
            let fingerprints: HashMap<_, _> = addons
                .iter()
                .map(|addon| {
                    (
                        addon.path.clone(),
                        addon_wrench::cache::fingerprint(&addon.path),
                    )
                })
                .collect();
            let mut result = workshop::fetch(&ids, addons, &directory, app_id);
            if saving_association {
                result.addons.retain(|addon| {
                    let saved = fingerprints
                        .get(&addon.path)
                        .and_then(|stamp| stamp.as_deref())
                        .ok_or_else(|| "无法读取模组文件信息".to_string())
                        .and_then(|stamp| {
                            crate::steam::links::save(&crate::steam::links::path(), addon, stamp)
                        });
                    if let Err(error) = saved {
                        result.issues.insert(
                            addon.metadata.workshop_id.clone().unwrap(),
                            format!("关联未保存：{error}"),
                        );
                        result.message = format!("工坊资料已获取，关联未保存：{error}");
                        false
                    } else {
                        true
                    }
                });
            }
            (result, fingerprints)
        });
        cx.spawn(async move |this, cx| {
            let (result, fingerprints) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.metadata_loading = false;
                this.metadata_keys.clear();
                if this.game == game
                    && this.profile.as_ref().and_then(games::Game::workshop_app) == Some(app_id)
                {
                    for updated in result.addons {
                        if let Some(addon) = this.addons.iter_mut().find(|addon| {
                            addon.key == updated.key
                                && addon.path == updated.path
                                && addon.bytes == updated.bytes
                                && fingerprints.get(&addon.path).is_some_and(|before| {
                                    before.is_some()
                                        && *before == addon_wrench::cache::fingerprint(&addon.path)
                                })
                        }) {
                            if association.is_some() {
                                if addon.metadata.workshop_id != updated.metadata.workshop_id {
                                    workshop::clear_online_cover(addon);
                                }
                                addon.metadata.workshop_id = updated.metadata.workshop_id.clone();
                            }
                            workshop::merge_metadata(addon, &updated);
                            if association.is_some() && this.detail.as_ref() == Some(&addon.key) {
                                this.workshop_link_editing = false;
                                this.workshop_link.update(cx, |input, cx| input.clear(cx));
                            }
                        } else {
                            this.metadata_status
                                .insert(updated.key, "模组文件已变化，请刷新后重新获取".into());
                        }
                    }
                    for key in keys {
                        let id = association.as_ref().or_else(|| {
                            this.addons
                                .iter()
                                .find(|addon| addon.key == key)
                                .and_then(|addon| addon.metadata.workshop_id.as_ref())
                        });
                        if let Some(error) = id.and_then(|id| result.issues.get(id)) {
                            this.metadata_status.insert(key, error.clone());
                        }
                    }
                    if !this.busy && !this.error {
                        this.error = !result.issues.is_empty();
                        this.set_message(result.message);
                    }
                    this.rebuild_covers();
                    this.reconcile();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> Result<Snapshot, String> + Send + 'static,
    ) {
        self.job_with_selection(cx, Vec::new(), work);
    }

    pub(super) fn job_with_selection(
        &mut self,
        cx: &mut Context<Self>,
        completed_selection: Vec<String>,
        work: impl FnOnce() -> Result<Snapshot, String> + Send + 'static,
    ) {
        self.run_job(cx, completed_selection, false, work);
    }

    pub(super) fn run_job(
        &mut self,
        cx: &mut Context<Self>,
        completed_selection: Vec<String>,
        runtime_refresh: bool,
        work: impl FnOnce() -> Result<Snapshot, String> + Send + 'static,
    ) {
        self.busy = true;
        self.error = false;
        self.scan_error = None;
        self.set_message("正在处理，请稍候…".into());
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            work().and_then(|mut snapshot| {
                snapshot.profile = snapshot
                    .game
                    .as_ref()
                    .map(|p| games::identify(p))
                    .transpose()?;
                crate::steam::links::attach(&crate::steam::links::path(), &mut snapshot.addons);
                if snapshot.scan_error.is_none()
                    && let Some(game) = snapshot.game.as_ref()
                    && let Some(app_id) = snapshot
                        .profile
                        .as_ref()
                        .and_then(games::Game::workshop_app)
                {
                    let _ =
                        workshop::maintain(&snapshot.addons, &workshop::cache_dir(), game, app_id);
                }
                workshop::prepare(
                    &mut snapshot.addons,
                    &workshop::cache_dir(),
                    snapshot
                        .profile
                        .as_ref()
                        .and_then(games::Game::workshop_app),
                );
                Ok(snapshot)
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let refresh_error = result.as_ref().err().cloned().or_else(|| {
                    result
                        .as_ref()
                        .ok()
                        .and_then(|snapshot| snapshot.scan_error.clone())
                });
                this.busy = false;
                this.context_menu = None;
                this.menu = None;
                match result {
                    Ok(mut snapshot) => {
                        this.browser
                            .complete_action(&completed_selection, snapshot.scan_error.is_none());
                        let switched_game = this.game != snapshot.game;
                        let group_name = this
                            .active_group
                            .and_then(|i| this.groups.get(i))
                            .map(|g| g.name.clone());
                        if let Some(catalog) = snapshot.catalog {
                            this.catalog = catalog;
                        }
                        if let Some(profile) = &snapshot.profile {
                            this.catalog.add(profile.path.clone());
                            this.catalog.selected = Some(profile.path.clone());
                            if let Err(error) = games::save(&games::path(), &this.catalog) {
                                this.set_message(format!("游戏列表未能保存：{error}"));
                            }
                        }
                        this.profiles = this
                            .catalog
                            .paths
                            .iter()
                            .filter_map(|path| games::identify(path).ok())
                            .collect();
                        this.profile = snapshot.profile;
                        this.scan_error = snapshot.scan_error;
                        this.game = snapshot.game;
                        if switched_game {
                            this.runtime = if this.can_manage() {
                                runtime::Monitor::checking()
                            } else {
                                runtime::Monitor::default()
                            };
                        }
                        if !switched_game {
                            workshop::preserve_recent(&mut snapshot.addons, &this.addons);
                        }
                        this.addons = snapshot.addons;
                        if !this.message.starts_with("游戏列表未能保存") {
                            this.set_message(snapshot.message);
                        }
                        this.error = this.scan_error.is_some();
                        if let Some(error) = &this.scan_error {
                            this.set_message(error.clone());
                        }
                        if let Some(game) = &this.game {
                            match groups::load(&groups::path(), game) {
                                Ok(groups) => {
                                    this.groups = groups;
                                    this.groups_ready = true;
                                    this.active_group = group_name.and_then(|name| {
                                        this.groups.iter().position(|g| g.name == name)
                                    });
                                    if this.active_group.is_none() {
                                        this.menu = None;
                                    }
                                }
                                Err(error) => {
                                    if switched_game {
                                        this.groups.clear();
                                    }
                                    this.groups_ready = false;
                                    this.error = true;
                                    this.set_message(error);
                                }
                            }
                        }
                        if switched_game {
                            this.active_group = None;
                            this.group_editor = None;
                            this.menu = None;
                            this.browser = Browser::default();
                            this.metadata_attempted.clear();
                            this.metadata_keys.clear();
                            this.metadata_status.clear();
                            this.search.update(cx, |input, cx| input.clear(cx));
                            this.detail = None;
                            this.technical_details = false;
                            this.group_toggle = None;
                            this.missing_group = None;
                        }
                    }
                    Err(message) => {
                        this.scan_error = Some(message.clone());
                        this.error = true;
                        this.set_message(message);
                    }
                }
                if runtime_refresh {
                    this.finish_runtime_refresh(refresh_error);
                }
                this.reconcile();
                this.rebuild_covers();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn pick(&mut self, import: bool, cx: &mut Context<Self>) {
        if import && !self.can_write() {
            return;
        }
        self.busy = true;
        self.error = false;
        self.set_message("等待选择文件或目录…".into());
        cx.notify();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: import,
            directories: !import,
            multiple: import,
            prompt: Some(
                if import {
                    "选择 VPK 模组"
                } else {
                    "选择游戏目录"
                }
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.context_menu = None;
                this.menu = None;
                match result {
                    Ok(Ok(Some(paths))) if !paths.is_empty() => {
                        if import {
                            this.page = Page::Addons;
                            let game = this.game.clone().unwrap();
                            this.job(cx, move || {
                                let mut installed = 0;
                                let mut failures = Vec::new();
                                for source in paths {
                                    match games::identify(&game)
                                        .and_then(|profile| profile.import(&source))
                                    {
                                        Ok(_) => installed += 1,
                                        Err(e) => failures.push(format!(
                                            "{}：{e}",
                                            source
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                        )),
                                    }
                                }
                                let message = if failures.is_empty() {
                                    format!("已导入 {installed} 个模组。启用状态以列表为准。")
                                } else {
                                    format!(
                                        "已导入 {installed} 个；{} 个未导入：{}",
                                        failures.len(),
                                        failures.join("；")
                                    )
                                };
                                Ok(Snapshot {
                                    scan_error: None,
                                    catalog: None,
                                    profile: None,
                                    addons: games::scan_path(&game)?,
                                    game: Some(game),
                                    message,
                                })
                            });
                        } else {
                            this.page = Page::Addons;
                            let selected = paths[0].clone();
                            this.job(cx, move || {
                                let game = discovery::game_dir(&selected)?;
                                let addons = games::scan_path(&game)?;
                                let message = match discovery::remember(&game) {
                                    Ok(()) => "已连接游戏目录。修改模组前请先退出游戏。".into(),
                                    Err(e) => format!("已连接，但无法记住目录：{e}"),
                                };
                                Ok(Snapshot {
                                    scan_error: None,
                                    catalog: None,
                                    profile: None,
                                    game: Some(game),
                                    addons,
                                    message,
                                })
                            });
                        }
                    }
                    Ok(Err(e)) => {
                        this.error = true;
                        this.set_message(format!("无法打开文件选择器：{e}"));
                    }
                    Err(e) => {
                        this.error = true;
                        this.set_message(format!("文件选择器已断开：{e}"));
                    }
                    _ => {
                        this.set_message(String::new());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn persist_groups(&mut self, updated: Vec<groups::Group>) -> bool {
        if !self.groups_ready {
            return false;
        }
        let Some(game) = &self.game else {
            return false;
        };
        match groups::save(&groups::path(), game, &self.groups, &updated) {
            Ok(()) => {
                self.groups = updated;
                self.error = false;
                self.set_message("分组已保存".into());
                true
            }
            Err(e) => {
                self.error = true;
                self.set_message(e);
                false
            }
        }
    }

    pub(super) fn change(&mut self, changes: Vec<(String, bool)>, cx: &mut Context<Self>) {
        self.change_selected(changes, Vec::new(), cx);
    }

    pub(super) fn change_selected(
        &mut self,
        changes: Vec<(String, bool)>,
        selection: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.can_write() || changes.is_empty() {
            return;
        }
        let Some(game) = self.game.clone() else {
            return;
        };
        self.job_with_selection(cx, selection, move || {
            games::identify(&game)?.set_enabled(&changes)?;
            let message = format!("已保存 {} 个模组的启用状态。", changes.len());
            Ok(Snapshot {
                scan_error: None,
                catalog: None,
                profile: None,
                addons: games::scan_path(&game)?,
                game: Some(game),
                message,
            })
        });
    }
}

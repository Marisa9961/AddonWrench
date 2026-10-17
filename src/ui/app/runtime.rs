use super::*;
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_secs(1);
const SETTLE_TIME: Duration = Duration::from_secs(2);

#[derive(Debug, Default, PartialEq, Eq)]
enum Protection {
    Checking,
    #[default]
    Ready,
    Running,
    Settling,
    Refreshing,
    Unavailable(String),
}

#[derive(Default)]
pub(super) struct Monitor {
    protection: Protection,
    needs_refresh: bool,
    stable: Option<(Option<Vec<u8>>, Instant)>,
}

enum Observation {
    Running,
    Stopped(Option<Vec<u8>>),
}

fn configuration(game: &std::path::Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(game.join("addonlist.txt")) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("无法读取模组配置，暂时禁止修改：{error}")),
    }
}

impl Monitor {
    pub(super) fn checking() -> Self {
        Self {
            protection: Protection::Checking,
            ..Self::default()
        }
    }

    pub(super) fn can_write(&self) -> bool {
        self.protection == Protection::Ready
    }

    pub(super) fn message(&self) -> Option<&str> {
        match &self.protection {
            Protection::Ready => None,
            Protection::Checking => Some("正在检查游戏运行状态…"),
            Protection::Running => Some(addon_wrench::runtime::RUNNING_MESSAGE),
            Protection::Settling => Some("游戏已退出，正在等待配置保存…"),
            Protection::Refreshing => Some("游戏已退出，正在刷新模组列表…"),
            Protection::Unavailable(error) => Some(error),
        }
    }

    fn observe(&mut self, observation: Result<Observation, String>, now: Instant) {
        match observation {
            Ok(Observation::Running) => {
                self.protection = Protection::Running;
                self.needs_refresh = true;
                self.stable = None;
            }
            Err(error) => {
                self.protection = Protection::Unavailable(error);
                self.needs_refresh = true;
                self.stable = None;
            }
            Ok(Observation::Stopped(stamp)) => {
                if !self.needs_refresh {
                    self.protection = Protection::Ready;
                    return;
                }
                if self
                    .stable
                    .as_ref()
                    .is_none_or(|(previous, _)| previous != &stamp)
                {
                    self.stable = Some((stamp, now));
                    self.protection = Protection::Settling;
                } else if self
                    .stable
                    .as_ref()
                    .is_some_and(|(_, since)| now.duration_since(*since) >= SETTLE_TIME)
                {
                    self.protection = Protection::Refreshing;
                }
            }
        }
    }

    fn finish_refresh(&mut self, error: Option<String>) {
        if self.protection != Protection::Refreshing {
            return;
        }
        if let Some(error) = error {
            self.protection =
                Protection::Unavailable(format!("无法刷新模组列表，暂时禁止修改：{error}"));
            self.stable = None;
        } else {
            self.protection = Protection::Ready;
            self.needs_refresh = false;
            self.stable = None;
        }
    }
}

impl Manager {
    pub(super) fn watch_game(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(INTERVAL).await;
                let Ok(game) = this.update(cx, |this, _| {
                    this.profile
                        .as_ref()
                        .filter(|game| game.can_manage())
                        .map(|game| game.path.clone())
                }) else {
                    break;
                };
                let Some(game) = game else {
                    continue;
                };
                let checked_game = game.clone();
                let observation = cx
                    .background_executor()
                    .spawn(async move {
                        if addon_wrench::runtime::is_running(&checked_game)? {
                            return Ok(Observation::Running);
                        }
                        Ok(Observation::Stopped(configuration(&checked_game)?))
                    })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        // A previous installation's result must never unlock the current one.
                        if this.game.as_ref() != Some(&game) {
                            return;
                        }
                        this.runtime.observe(observation, Instant::now());
                        if this.runtime.protection == Protection::Running {
                            this.group_toggle = None;
                        }
                        if this.runtime.protection == Protection::Refreshing && !this.busy {
                            this.refresh_mods(true, cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn finish_runtime_refresh(&mut self, error: Option<String>) {
        self.runtime.finish_refresh(error);
    }

    pub(super) fn refresh_mods(&mut self, after_exit: bool, cx: &mut Context<Self>) {
        let Some(game) = self.game.clone() else {
            return;
        };
        self.run_job(cx, Vec::new(), after_exit, move || {
            let before = if after_exit {
                addon_wrench::runtime::ensure_stopped(&game)?;
                configuration(&game)?
            } else {
                None
            };
            let addons = games::scan_path(&game)?;
            if after_exit {
                addon_wrench::runtime::ensure_stopped(&game)?;
                if configuration(&game)? != before {
                    return Err("模组配置仍在变化，稍后将自动重新扫描".into());
                }
            }
            Ok(Snapshot {
                scan_error: None,
                catalog: None,
                profile: None,
                addons,
                game: Some(game),
                message: if after_exit {
                    "游戏已退出，模组列表已更新"
                } else {
                    "模组列表已更新"
                }
                .into(),
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stopped(bytes: &[u8]) -> Result<Observation, String> {
        Ok(Observation::Stopped(Some(bytes.to_vec())))
    }

    #[test]
    fn exit_waits_for_stable_configuration_and_a_completed_rescan() {
        let now = Instant::now();
        let mut monitor = Monitor::checking();
        assert!(!monitor.can_write());
        monitor.observe(stopped(b"old"), now);
        assert!(monitor.can_write());
        monitor.observe(Ok(Observation::Running), now);
        assert!(!monitor.can_write());
        monitor.observe(stopped(b"old"), now + INTERVAL);
        monitor.observe(stopped(b"new"), now + 2 * INTERVAL);
        monitor.observe(stopped(b"new"), now + 3 * INTERVAL);
        assert_eq!(monitor.protection, Protection::Settling);
        monitor.observe(stopped(b"new"), now + 4 * INTERVAL);
        assert_eq!(monitor.protection, Protection::Refreshing);
        assert!(!monitor.can_write());
        monitor.finish_refresh(None);
        assert!(monitor.can_write());
    }

    #[test]
    fn errors_restarts_and_configuration_changes_cannot_unlock_pending_refreshes() {
        let now = Instant::now();
        let mut monitor = Monitor::checking();
        monitor.observe(Err("Access denied".into()), now);
        assert!(!monitor.can_write());
        monitor.observe(stopped(b"a"), now);
        monitor.observe(stopped(b"a"), now + SETTLE_TIME);
        monitor.finish_refresh(Some("invalid configuration".into()));
        assert!(!monitor.can_write());
        monitor.observe(stopped(b"b"), now + 3 * INTERVAL);
        monitor.observe(stopped(b"b"), now + 5 * INTERVAL);
        assert_eq!(monitor.protection, Protection::Refreshing);
        monitor.observe(Ok(Observation::Running), now + 6 * INTERVAL);
        monitor.finish_refresh(None);
        assert!(!monitor.can_write());
        monitor.observe(stopped(b"b"), now + 7 * INTERVAL);
        monitor.observe(stopped(b"b"), now + 9 * INTERVAL);
        monitor.observe(stopped(b"c"), now + 10 * INTERVAL);
        monitor.finish_refresh(None);
        assert!(!monitor.can_write());
    }
}

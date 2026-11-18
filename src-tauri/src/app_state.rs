use std::sync::{Arc, RwLock};

use crate::config::model::{RuleBook, Settings};
use crate::db::Db;
use crate::monitor::MonitorHandle;
use crate::win::protected::SelfGuard;

/// 全局共享状态。所有字段都可从任意线程访问。
pub struct AppState {
    pub db: Arc<Db>,
    pub settings: RwLock<Settings>,
    pub rules: RwLock<RuleBook>,
    pub guard: RwLock<SelfGuard>,
    pub monitor: RwLock<Option<MonitorHandle>>,
}

impl AppState {
    pub fn new(db: Arc<Db>, settings: Settings, rules: RuleBook, guard: SelfGuard) -> Self {
        Self {
            db,
            settings: RwLock::new(settings),
            rules: RwLock::new(rules),
            guard: RwLock::new(guard),
            monitor: RwLock::new(None),
        }
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.read().expect("settings poisoned").clone()
    }

    pub fn rules_snapshot(&self) -> RuleBook {
        self.rules.read().expect("rules poisoned").clone()
    }

    pub fn guard_snapshot(&self) -> SelfGuard {
        self.guard.read().expect("guard poisoned").clone()
    }

    /// 向监控线程投递命令。监控未启动时静默忽略。
    pub fn notify_monitor(&self, cmd: crate::monitor::MonitorCmd) {
        if let Some(h) = self.monitor.read().expect("monitor poisoned").as_ref() {
            h.send(cmd);
        }
    }
}

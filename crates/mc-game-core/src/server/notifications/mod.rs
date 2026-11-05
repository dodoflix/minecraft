//! `net.minecraft.server.notifications`: the event sink the JSON-RPC management server listens on.

use std::sync::Arc;

use parking_lot::RwLock;

use super::players::{IpBanListEntry, NameAndId, ServerOpListEntry, UserBanListEntry};

/// A typed game rule value (`GameRulesService.getTypedRule`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleValue {
    Bool(bool),
    Int(i32),
}

/// `NotificationService`. Players are passed as their profile (`PlayerDto.from(player)` only reads id and name).
pub trait NotificationService: Send + Sync {
    fn player_joined(&self, _player: &NameAndId) {}
    fn player_left(&self, _player: &NameAndId) {}
    fn server_started(&self) {}
    fn server_shutting_down(&self) {}
    fn server_save_started(&self) {}
    fn server_save_completed(&self) {}
    fn server_activity_occured(&self) {}
    fn world_upgrade_started(&self) {}
    fn world_upgrade_progress(&self, _progress_percentage: f32) {}
    fn world_upgrade_finished(&self) {}
    fn world_upgrade_failed(&self, _reason: &str) {}
    fn player_oped(&self, _operator: &ServerOpListEntry) {}
    fn player_deoped(&self, _operator: &ServerOpListEntry) {}
    fn player_added_to_allowlist(&self, _player: &NameAndId) {}
    fn player_removed_from_allowlist(&self, _player: &NameAndId) {}
    fn ip_banned(&self, _ban: &IpBanListEntry) {}
    fn ip_unbanned(&self, _ip: &str) {}
    fn player_banned(&self, _ban: &UserBanListEntry) {}
    fn player_unbanned(&self, _player: &NameAndId) {}
    /// `onGameRuleChanged`: the rule's key (`minecraft:...`) and new value.
    fn on_game_rule_changed(&self, _rule: &str, _value: &GameRuleValue) {}
    fn status_heartbeat(&self) {}
}

/// `EmptyNotificationService`.
pub struct EmptyNotificationService;

impl NotificationService for EmptyNotificationService {}

/// `NotificationManager`: fans every event out to the registered services.
#[derive(Default)]
pub struct NotificationManager {
    services: RwLock<Vec<Arc<dyn NotificationService>>>,
}

impl NotificationManager {
    pub fn new() -> NotificationManager {
        NotificationManager::default()
    }

    pub fn register_service(&self, service: Arc<dyn NotificationService>) {
        self.services.write().push(service);
    }

    fn each(&self, f: impl Fn(&dyn NotificationService)) {
        for service in self.services.read().iter() {
            f(service.as_ref());
        }
    }
}

macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(fn $name(&self, $($arg: $ty),*) { self.each(|s| s.$name($($arg),*)); })*
    };
}

impl NotificationService for NotificationManager {
    forward! {
        player_joined(player: &NameAndId);
        player_left(player: &NameAndId);
        server_started();
        server_shutting_down();
        server_save_started();
        server_save_completed();
        server_activity_occured();
        world_upgrade_started();
        world_upgrade_progress(progress_percentage: f32);
        world_upgrade_finished();
        world_upgrade_failed(reason: &str);
        player_oped(operator: &ServerOpListEntry);
        player_deoped(operator: &ServerOpListEntry);
        player_added_to_allowlist(player: &NameAndId);
        player_removed_from_allowlist(player: &NameAndId);
        ip_banned(ban: &IpBanListEntry);
        ip_unbanned(ip: &str);
        player_banned(ban: &UserBanListEntry);
        player_unbanned(player: &NameAndId);
        on_game_rule_changed(rule: &str, value: &GameRuleValue);
        status_heartbeat();
    }
}

/// `ServerActivityMonitor`: turns login activity into at most one `serverActivityOccured` per interval.
pub struct ServerActivityMonitor {
    notifications: Arc<dyn NotificationService>,
    minimum_between_notifications: std::time::Duration,
    start: std::time::Instant,
    /// Milliseconds since `start` of the last notification (`i64::MIN` = never, i.e. `0` in vanilla's epoch millis).
    last_notification_ms: std::sync::atomic::AtomicI64,
    server_activity: std::sync::atomic::AtomicBool,
}

impl ServerActivityMonitor {
    pub fn new(notifications: Arc<dyn NotificationService>, seconds_between_notifications: u64) -> Self {
        ServerActivityMonitor {
            notifications,
            minimum_between_notifications: std::time::Duration::from_secs(seconds_between_notifications),
            start: std::time::Instant::now(),
            last_notification_ms: std::sync::atomic::AtomicI64::new(i64::MIN / 2),
            server_activity: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn now_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }

    pub fn tick(&self) {
        self.process_with_rate_limit();
    }

    pub fn report_login_activity(&self) {
        self.server_activity.store(true, std::sync::atomic::Ordering::SeqCst);
        self.process_with_rate_limit();
    }

    fn process_with_rate_limit(&self) {
        use std::sync::atomic::Ordering::SeqCst;
        let now = self.now_ms();
        if self.server_activity.load(SeqCst) && now - self.last_notification_ms.load(SeqCst) >= self.minimum_between_notifications.as_millis() as i64 {
            self.notifications.server_activity_occured();
            self.last_notification_ms.store(self.now_ms(), SeqCst);
        }
        self.server_activity.store(false, SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl NotificationService for Counter {
        fn server_activity_occured(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn activity_is_rate_limited_and_manager_fans_out() {
        let counter = Arc::new(Counter::default());
        let manager = Arc::new(NotificationManager::new());
        manager.register_service(counter.clone());
        manager.register_service(counter.clone());
        let monitor = ServerActivityMonitor::new(manager, 30);
        monitor.tick(); // no activity yet
        assert_eq!(counter.0.load(Ordering::SeqCst), 0);
        monitor.report_login_activity();
        assert_eq!(counter.0.load(Ordering::SeqCst), 2); // two registered services
        monitor.report_login_activity(); // inside the 30 s window
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    }
}

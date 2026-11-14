//! The tick time samples of `MinecraftServer` (`logFullTickTime`, `startMeasuringTaskExecutionTime`, ...) and
//! `DedicatedServer`'s `isTickTimeLoggingEnabled` / `tickTimeLogger`: sent to the operators subscribed to
//! `minecraft:dedicated_server_tick_time` for the client's tick time chart.

use super::minecraft_server::MinecraftServer;
use super::shared::get_nanos;
use crate::util::debug::{DebugSubscribers, subscriptions};
use crate::util::debugchart::{RemoteSampleLogger, TpsDebugDimensions};

pub(super) struct TickTimeLogging {
    logger: RemoteSampleLogger,
    /// `isTickTimeLoggingEnabled`: whether somebody subscribed as of the end of the previous tick.
    enabled: bool,
    task_execution_start_nanos: i64,
    pub(super) idle_time_nanos: i64,
}

impl TickTimeLogging {
    pub(super) fn new(subscribers: DebugSubscribers) -> Self {
        Self {
            logger: RemoteSampleLogger::new(TpsDebugDimensions::COUNT, subscribers),
            enabled: false,
            task_execution_start_nanos: get_nanos(),
            idle_time_nanos: 0,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }
}

impl MinecraftServer {
    /// `logFullTickTime()`.
    pub(super) fn log_full_tick_time(&mut self) {
        let current_time = get_nanos();
        if self.tick_time_logging.enabled {
            self.tick_time_logging
                .logger
                .log_sample(current_time - self.last_tick_nanos);
        }
        self.last_tick_nanos = current_time;
    }

    /// `startMeasuringTaskExecutionTime()`.
    pub(super) fn start_measuring_task_execution_time(&mut self) {
        let logging = &mut self.tick_time_logging;
        if logging.enabled {
            logging.task_execution_start_nanos = get_nanos();
            logging.idle_time_nanos = 0;
        }
    }

    /// `finishMeasuringTaskExecutionTime()`.
    pub(super) fn finish_measuring_task_execution_time(&mut self) {
        let logging = &mut self.tick_time_logging;
        if logging.enabled {
            let scheduled =
                get_nanos() - logging.task_execution_start_nanos - logging.idle_time_nanos;
            logging
                .logger
                .log_partial_sample(scheduled, TpsDebugDimensions::ScheduledTasks.ordinal());
            logging
                .logger
                .log_partial_sample(logging.idle_time_nanos, TpsDebugDimensions::Idle.ordinal());
        }
    }

    /// `logTickMethodTime(startTime)`.
    pub(super) fn log_tick_method_time(&mut self, start_time: i64) {
        let logging = &mut self.tick_time_logging;
        if logging.enabled {
            logging.logger.log_partial_sample(
                get_nanos() - start_time,
                TpsDebugDimensions::TickServerMethod.ordinal(),
            );
        }
    }

    /// The part of `DedicatedServer.endMetricsRecordingTick` after `super`: re-reads whether the tick time is subscribed to.
    pub(super) fn refresh_tick_time_logging(&mut self) {
        self.tick_time_logging.enabled = self
            .debug_subscribers
            .has_any_subscriber_for(subscriptions::dedicated_server_tick_time());
    }
}

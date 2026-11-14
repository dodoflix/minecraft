//! `AbstractSampleLogger` + `RemoteSampleLogger` for `RemoteDebugSampleType.TICK_TIME`.

use crate::util::debug::{DebugSubscribers, subscriptions};
use mc_protocol::packets::play::{ClientboundDebugSamplePacket, RemoteDebugSampleType};

/// `TpsDebugDimensions`: the index of a value in a tick time sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TpsDebugDimensions {
    FullTick,
    TickServerMethod,
    ScheduledTasks,
    Idle,
}

impl TpsDebugDimensions {
    /// `values().length`.
    pub const COUNT: usize = 4;

    /// `ordinal()`.
    pub fn ordinal(self) -> usize {
        self as usize
    }
}

/// `RemoteSampleLogger`: collects the dimensions of a sample and broadcasts the sample to the subscribers once it is complete.
pub struct RemoteSampleLogger {
    subscribers: DebugSubscribers,
    sample: Vec<i64>,
}

impl RemoteSampleLogger {
    /// `new RemoteSampleLogger(dimensions, subscribers, TICK_TIME)` (the defaults are zero).
    pub fn new(dimensions: usize, subscribers: DebugSubscribers) -> Self {
        Self {
            subscribers,
            sample: vec![0; dimensions],
        }
    }

    /// `logSample(sample)`: dimension 0 completes the sample.
    pub fn log_sample(&mut self, sample: i64) {
        self.sample[0] = sample;
        self.use_sample();
        self.sample.fill(0);
    }

    /// `logPartialSample(sample, dimension)`: dimensions other than 0 wait for the next `log_sample`.
    pub fn log_partial_sample(&mut self, sample: i64, dimension: usize) {
        assert!(
            (1..self.sample.len()).contains(&dimension),
            "{dimension} out of bounds for dimensions {}",
            self.sample.len()
        );
        self.sample[dimension] = sample;
    }

    /// `useSample()`.
    fn use_sample(&self) {
        let subscription = subscriptions::dedicated_server_tick_time();
        if self.subscribers.has_any_subscriber_for(subscription) {
            self.subscribers.broadcast_to_all(
                subscription,
                &ClientboundDebugSamplePacket {
                    sample: self.sample.clone(),
                    debug_sample_type: RemoteDebugSampleType::TickTime,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::network::Outbound;
    use crate::server::players::player_handle::test_support::{Wire, handle};
    use mc_protocol::packet::encode_packet;

    fn operator_subscribed(
        subscriptions: Vec<mc_data::DebugSubscription>,
    ) -> (DebugSubscribers, Wire) {
        let (player, wire) = handle(1, "Op", "minecraft:overworld");
        player.shared.lock().requested_debug_subscriptions = subscriptions;
        let subscribers = DebugSubscribers::default();
        subscribers.tick(&[player], |_| true);
        (subscribers, wire)
    }

    fn drain(wire: &mut Wire) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Ok(Outbound::Payload(bytes)) = wire.0.try_recv() {
            out.push(bytes.to_vec());
        }
        out
    }

    fn expected(sample: [i64; 4]) -> Vec<u8> {
        encode_packet(&ClientboundDebugSamplePacket {
            sample: sample.to_vec(),
            debug_sample_type: RemoteDebugSampleType::TickTime,
        })
        .unwrap()
    }

    #[test]
    fn partial_dimensions_ride_the_next_full_sample_and_reset() {
        let (subscribers, mut wire) =
            operator_subscribed(vec![subscriptions::dedicated_server_tick_time()]);
        let mut logger = RemoteSampleLogger::new(TpsDebugDimensions::COUNT, subscribers);
        logger.log_partial_sample(7, TpsDebugDimensions::TickServerMethod.ordinal());
        logger.log_partial_sample(9, TpsDebugDimensions::Idle.ordinal());
        assert!(drain(&mut wire).is_empty());
        logger.log_sample(50);
        logger.log_sample(51);
        assert_eq!(
            drain(&mut wire),
            vec![expected([50, 7, 0, 9]), expected([51, 0, 0, 0])]
        );
    }

    #[test]
    fn nothing_is_sent_without_a_subscriber_for_the_tick_time() {
        let (subscribers, mut wire) = operator_subscribed(vec![subscriptions::bees()]);
        let mut logger = RemoteSampleLogger::new(TpsDebugDimensions::COUNT, subscribers);
        logger.log_sample(50);
        assert!(drain(&mut wire).is_empty());
    }

    #[test]
    #[should_panic(expected = "0 out of bounds for dimensions 4")]
    fn dimension_zero_is_not_partial() {
        RemoteSampleLogger::new(4, DebugSubscribers::default()).log_partial_sample(1, 0);
    }
}

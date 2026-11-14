//! `net.minecraft.util.debugchart`: the samples the server sends to the operators that subscribed to them (the client's
//! tick time chart).

mod remote_sample_logger;

pub use remote_sample_logger::{RemoteSampleLogger, TpsDebugDimensions};

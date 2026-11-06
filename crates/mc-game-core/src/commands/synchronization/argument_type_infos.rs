//! `ArgumentTypeInfos`: maps each argument type to its wire template (`ArgumentTypeInfo.unpack`). Vanilla keys a
//! `Class -> info` map; here every argument type registers a downcast-and-unpack function in a linkme slice
//! ([`register_argument_type!`]) from its own module. The registry ids and the (de)serialization of the templates are
//! `mc_protocol::packets::play::ArgumentTypeTemplate`.

use crate::commands::CommandSourceStack;
use linkme::distributed_slice;
use mc_brigadier::ArgumentType;
use mc_protocol::packets::play::ArgumentTypeTemplate;

pub struct ArgumentTypeInfoEntry {
    /// `Some(template)` when the argument type is the one this entry registered.
    pub unpack: fn(&dyn ArgumentType<CommandSourceStack>) -> Option<ArgumentTypeTemplate>,
}

#[distributed_slice]
pub static ARGUMENT_TYPE_INFOS: [ArgumentTypeInfoEntry];

/// Registers the wire template of an argument type: `register_argument_type!(EntityArgument, |a| Template { .. })`.
#[macro_export]
macro_rules! register_argument_type {
    ($ty:ty, |$arg:ident| $template:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::commands::synchronization::ARGUMENT_TYPE_INFOS)]
            static INFO: $crate::commands::synchronization::ArgumentTypeInfoEntry =
                $crate::commands::synchronization::ArgumentTypeInfoEntry {
                    unpack: |arg| {
                        let $arg: &$ty = arg.downcast_ref::<$ty>()?;
                        Some($template)
                    },
                };
        };
    };
}

pub struct ArgumentTypeInfos;

impl ArgumentTypeInfos {
    /// `isClassRecognized`.
    pub fn is_class_recognized(argument_type: &dyn ArgumentType<CommandSourceStack>) -> bool {
        Self::try_unpack(argument_type).is_some()
    }

    pub fn try_unpack(
        argument_type: &dyn ArgumentType<CommandSourceStack>,
    ) -> Option<ArgumentTypeTemplate> {
        ARGUMENT_TYPE_INFOS
            .iter()
            .find_map(|entry| (entry.unpack)(argument_type))
    }

    /// `unpack(argumentType)`: panics for an unregistered type like vanilla's `IllegalArgumentException`.
    pub fn unpack(argument_type: &dyn ArgumentType<CommandSourceStack>) -> ArgumentTypeTemplate {
        Self::try_unpack(argument_type).unwrap_or_else(|| {
            panic!(
                "Unrecognized argument type {} ({})",
                argument_type.describe(),
                argument_type.describe()
            )
        })
    }
}

// Brigadier's own argument types.
mod brigadier_types {
    use mc_brigadier::arguments::{
        BoolArgumentType, DoubleArgumentType, FloatArgumentType, IntegerArgumentType,
        LongArgumentType, StringArgumentType, StringType,
    };
    use mc_protocol::packets::play::{
        ArgumentTypeTemplate as T, StringArgumentType as WireStringType,
    };

    crate::register_argument_type!(BoolArgumentType, |_a| T::Bool);
    crate::register_argument_type!(FloatArgumentType, |a| T::Float {
        min: a.get_minimum(),
        max: a.get_maximum()
    });
    crate::register_argument_type!(DoubleArgumentType, |a| T::Double {
        min: a.get_minimum(),
        max: a.get_maximum()
    });
    crate::register_argument_type!(IntegerArgumentType, |a| T::Integer {
        min: a.get_minimum(),
        max: a.get_maximum()
    });
    crate::register_argument_type!(LongArgumentType, |a| T::Long {
        min: a.get_minimum(),
        max: a.get_maximum()
    });
    crate::register_argument_type!(StringArgumentType, |a| T::String(match a.get_type() {
        StringType::SingleWord => WireStringType::SingleWord,
        StringType::QuotablePhrase => WireStringType::QuotablePhrase,
        StringType::GreedyPhrase => WireStringType::GreedyPhrase,
    }));
}

//! `StringRepresentableArgument` and its subclasses: arguments over an enum by serialized name.

/// Defines `pub struct $name;` implementing `ArgumentType<CommandSourceStack>` for an enum `$t`: `values()` lists the
/// accepted constants, `id(&value)` is the (already converted) serialized name.
macro_rules! string_representable_argument {
    ($(#[$meta:meta])* $name:ident, $t:ty, $values:expr, $id:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $name;

        impl $name {
            fn values() -> Vec<$t> {
                ($values)()
            }

            fn id(value: &$t) -> String {
                ($id)(value)
            }

            pub fn get(context: &mc_brigadier::CommandContext<$crate::commands::CommandSourceStack>, name: &str) -> $t {
                *context.get_argument::<$t>(name)
            }
        }

        impl mc_brigadier::ArgumentType<$crate::commands::CommandSourceStack> for $name {
            fn parse(&self, reader: &mut mc_brigadier::StringReader) -> Result<mc_brigadier::ParsedValue, mc_brigadier::CommandSyntaxException> {
                let id = reader.read_unquoted_string();
                match Self::values().into_iter().find(|v| Self::id(v) == id) {
                    Some(v) => Ok(mc_brigadier::arguments::value(v)),
                    None => Err($crate::commands::arguments::ERROR_ENUM_INVALID.create_with_context(reader, id)),
                }
            }

            fn list_suggestions(
                &self,
                _context: &mc_brigadier::CommandContext<$crate::commands::CommandSourceStack>,
                builder: mc_brigadier::SuggestionsBuilder,
            ) -> mc_brigadier::Suggestions {
                $crate::commands::shared_suggestion_provider::suggest(Self::values().iter().map(Self::id), builder)
            }

            fn get_examples(&self) -> Vec<String> {
                Self::values().iter().map(Self::id).take(2).collect()
            }

            fn describe(&self) -> String {
                stringify!($name).to_owned()
            }
        }
    };
}
pub(crate) use string_representable_argument;

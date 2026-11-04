use crate::arguments::{ArgumentType, ParsedValue, value};
use crate::context::CommandContext;
use crate::exceptions::{CommandSyntaxException, built_in_exceptions};
use crate::java_fmt::{double_to_string, float_to_string};
use crate::string_reader::StringReader;

fn int_to_string<T: ToString>(v: T) -> String {
    v.to_string()
}

macro_rules! number_argument_type {
    (
        $name:ident, $t:ty, $read:ident, $low:ident, $high:ident, $getter:ident,
        $ctor:ident, $ctor_min:ident, $ctor_range:ident, $min_const:expr, $max_const:expr,
        $label:literal, $examples:expr, $fmt:path
    ) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name {
            minimum: $t,
            maximum: $t,
        }

        pub fn $ctor() -> $name {
            $ctor_min($min_const)
        }

        pub fn $ctor_min(min: $t) -> $name {
            $ctor_range(min, $max_const)
        }

        pub fn $ctor_range(min: $t, max: $t) -> $name {
            $name { minimum: min, maximum: max }
        }

        impl $name {
            pub fn $getter<S: 'static>(context: &CommandContext<S>, name: &str) -> $t {
                *context.get_argument::<$t>(name)
            }

            pub fn get_minimum(&self) -> $t {
                self.minimum
            }

            pub fn get_maximum(&self) -> $t {
                self.maximum
            }

            fn parse_value(&self, reader: &mut StringReader) -> Result<$t, CommandSyntaxException> {
                let start = crate::string_reader::ImmutableStringReader::get_cursor(reader);
                let result = reader.$read()?;
                if result < self.minimum {
                    reader.set_cursor(start);
                    return Err(built_in_exceptions().$low().create_with_context(reader, result, self.minimum));
                }
                if result > self.maximum {
                    reader.set_cursor(start);
                    return Err(built_in_exceptions().$high().create_with_context(reader, result, self.maximum));
                }
                Ok(result)
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.maximum == other.maximum && self.minimum == other.minimum
            }
        }

        impl<S: 'static> ArgumentType<S> for $name {
            fn parse(&self, reader: &mut StringReader) -> Result<ParsedValue, CommandSyntaxException> {
                self.parse_value(reader).map(value)
            }

            fn get_examples(&self) -> Vec<String> {
                $examples.iter().map(|s| (*s).to_owned()).collect()
            }

            fn equals(&self, other: &dyn ArgumentType<S>) -> bool {
                other.downcast_ref::<$name>().is_some_and(|o| o == self)
            }

            fn describe(&self) -> String {
                if self.minimum == $min_const && self.maximum == $max_const {
                    concat!($label, "()").to_owned()
                } else if self.maximum == $max_const {
                    format!(concat!($label, "({})"), $fmt(self.minimum))
                } else {
                    format!(concat!($label, "({}, {})"), $fmt(self.minimum), $fmt(self.maximum))
                }
            }
        }
    };
}

number_argument_type!(
    IntegerArgumentType, i32, read_int, integer_too_low, integer_too_high, get_integer,
    integer, integer_min, integer_range, i32::MIN, i32::MAX, "integer", ["0", "123", "-123"], int_to_string
);
number_argument_type!(
    LongArgumentType, i64, read_long, long_too_low, long_too_high, get_long,
    long_arg, long_arg_min, long_arg_range, i64::MIN, i64::MAX, "longArg", ["0", "123", "-123"], int_to_string
);
number_argument_type!(
    FloatArgumentType, f32, read_float, float_too_low, float_too_high, get_float,
    float_arg, float_arg_min, float_arg_range, -f32::MAX, f32::MAX, "float",
    ["0", "1.2", ".5", "-1", "-.5", "-1234.56"], float_to_string
);
number_argument_type!(
    DoubleArgumentType, f64, read_double, double_too_low, double_too_high, get_double,
    double_arg, double_arg_min, double_arg_range, -f64::MAX, f64::MAX, "double",
    ["0", "1.2", ".5", "-1", "-.5", "-1234.56"], double_to_string
);

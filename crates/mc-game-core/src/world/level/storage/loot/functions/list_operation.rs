//! `ListOperation` and `ListOperation.StandAlone` (lore, pages, fireworks, custom model data).

use mc_dfu::extra::NON_NEGATIVE_INT;
use mc_dfu::{Codec, DataResult, MapCodec, record};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    ReplaceAll,
    ReplaceSection,
    Insert,
    Append,
}

const MODES: &[(Mode, &str)] = &[
    (Mode::ReplaceAll, "replace_all"),
    (Mode::ReplaceSection, "replace_section"),
    (Mode::Insert, "insert"),
    (Mode::Append, "append"),
];

/// `ListOperation`.
#[derive(Clone, Debug, PartialEq)]
pub enum ListOperation {
    ReplaceAll,
    ReplaceSection { offset: i32, size: Option<i32> },
    Insert { offset: i32 },
    Append,
}

impl ListOperation {
    fn mode(&self) -> Mode {
        match self {
            ListOperation::ReplaceAll => Mode::ReplaceAll,
            ListOperation::ReplaceSection { .. } => Mode::ReplaceSection,
            ListOperation::Insert { .. } => Mode::Insert,
            ListOperation::Append => Mode::Append,
        }
    }

    /// `ListOperation.codec(maxSize)`.
    pub fn codec(max_size: usize) -> MapCodec<ListOperation> {
        mc_dfu::string_enum_codec(MODES)
            .dispatch_map(
                "mode",
                |op: &ListOperation| op.mode(),
                |mode: &Mode| match mode {
                    Mode::ReplaceAll => MapCodec::unit(ListOperation::ReplaceAll),
                    Mode::Append => MapCodec::unit(ListOperation::Append),
                    Mode::Insert => NON_NEGATIVE_INT
                        .optional_field_of_default("offset", 0)
                        .xmap(
                            |offset| ListOperation::Insert { offset },
                            |op: &ListOperation| match op {
                                ListOperation::Insert { offset } => *offset,
                                _ => 0,
                            },
                        ),
                    Mode::ReplaceSection => record!(Section {
                        offset: NON_NEGATIVE_INT.optional_field_of_default("offset", 0),
                        size: NON_NEGATIVE_INT.optional_field_of("size"),
                    })
                    .xmap(
                        |s| ListOperation::ReplaceSection {
                            offset: s.offset,
                            size: s.size,
                        },
                        |op: &ListOperation| match op {
                            ListOperation::ReplaceSection { offset, size } => Section {
                                offset: *offset,
                                size: *size,
                            },
                            _ => Section {
                                offset: 0,
                                size: None,
                            },
                        },
                    ),
                },
            )
            .validate(move |op| {
                if let ListOperation::ReplaceSection {
                    size: Some(size), ..
                } = &op
                    && *size as i64 > max_size as i64
                {
                    let (size, max) = (*size, max_size);
                    return DataResult::error(move || {
                        format!("Size value too large: {size}, max size is {max}")
                    });
                }
                DataResult::success(op)
            })
    }

    /// `apply(original, replacement, maxSize)`.
    pub fn apply<T: Clone>(&self, original: &[T], replacement: &[T], max_size: usize) -> Vec<T> {
        let original_size = original.len();
        match self {
            ListOperation::Append => {
                if original_size + replacement.len() > max_size {
                    log::error!("Contents overflow in section append");
                    original.to_vec()
                } else {
                    original.iter().chain(replacement).cloned().collect()
                }
            }
            ListOperation::Insert { offset } => {
                let offset = *offset as usize;
                if offset > original_size {
                    log::error!("Cannot insert when offset is out of bounds");
                    original.to_vec()
                } else if original_size + replacement.len() > max_size {
                    log::error!("Contents overflow in section insertion");
                    original.to_vec()
                } else {
                    let mut out = original[..offset].to_vec();
                    out.extend_from_slice(replacement);
                    out.extend_from_slice(&original[offset..]);
                    out
                }
            }
            ListOperation::ReplaceAll => replacement.to_vec(),
            ListOperation::ReplaceSection { offset, size } => {
                let offset = *offset as usize;
                if offset > original_size {
                    log::error!("Cannot replace when offset is out of bounds");
                    return original.to_vec();
                }
                let mut out = original[..offset].to_vec();
                out.extend_from_slice(replacement);
                let resume = offset + size.map_or(replacement.len(), |s| s as usize);
                if resume < original_size {
                    out.extend_from_slice(&original[resume..]);
                }
                if out.len() > max_size {
                    log::error!("Contents overflow in section replacement");
                    original.to_vec()
                } else {
                    out
                }
            }
        }
    }
}

#[derive(Clone)]
struct Section {
    offset: i32,
    size: Option<i32>,
}

/// `ListOperation.StandAlone<T>`.
#[derive(Clone, Debug, PartialEq)]
pub struct StandAlone<T> {
    pub value: Vec<T>,
    pub operation: ListOperation,
}

impl<T: Clone + Send + Sync + 'static> StandAlone<T> {
    /// `StandAlone.codec(valueCodec, maxSize)`.
    pub fn codec(value_codec: &Codec<T>, max_size: usize) -> Codec<StandAlone<T>> {
        record!(StandAlone<T> {
            value: value_codec.size_limited_list_of(max_size).field_of("values"),
            operation: ListOperation::codec(max_size),
        })
        .codec()
    }

    /// `apply(input)`.
    pub fn apply(&self, input: &[T]) -> Vec<T> {
        self.operation.apply(input, &self.value, usize::MAX)
    }
}

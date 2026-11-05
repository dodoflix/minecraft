//! `InvertableSetOptionState` and `SetOnceOptionState`.

use mc_core::ident::Identifier;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Limitation {
    #[default]
    None,
    Single,
    Multiple,
}

#[derive(Clone, Debug, Default)]
pub struct InvertableSetOptionState {
    state: Limitation,
    tags: HashSet<Identifier>,
}

impl InvertableSetOptionState {
    fn can_limit_to_single(&self) -> bool {
        self.state == Limitation::None
    }

    fn can_limit_to_multiple(&self) -> bool {
        self.state != Limitation::Single
    }

    pub fn can_parse_positive_element(&self) -> bool {
        self.can_limit_to_single()
    }

    pub fn can_parse_negative_element(&self) -> bool {
        self.can_limit_to_multiple()
    }

    pub fn can_parse_any_tag(&self) -> bool {
        self.can_limit_to_multiple()
    }

    pub fn can_parse_tag(&self, tag: &Identifier) -> bool {
        self.can_parse_any_tag() && !self.tags.contains(tag)
    }

    pub fn can_parse_any(&self) -> bool {
        self.state != Limitation::Single
    }

    pub fn can_parse_element(&self, inverted: bool) -> bool {
        if inverted { self.can_parse_negative_element() } else { self.can_parse_positive_element() }
    }

    pub fn mark_parsed_positive_element(&mut self) {
        self.state = Limitation::Single;
    }

    pub fn mark_parsed_negative_element(&mut self) {
        self.state = Limitation::Multiple;
    }

    pub fn mark_parsed_tag(&mut self, tag: Identifier) {
        self.state = Limitation::Multiple;
        self.tags.insert(tag);
    }

    pub fn mark_parsed_element(&mut self, inverted: bool) {
        if inverted {
            self.mark_parsed_negative_element();
        } else {
            self.mark_parsed_positive_element();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SetOnceOptionState {
    has_value: bool,
}

impl SetOnceOptionState {
    pub fn can_parse(&self) -> bool {
        !self.has_value
    }

    pub fn mark_parsed(&mut self) {
        self.has_value = true;
    }
}

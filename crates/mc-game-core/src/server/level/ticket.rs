//! `Ticket` + `TicketType`.
use mc_core::ident::Identifier;
use mc_dfu::extra::{IDENTIFIER, NON_NEGATIVE_INT};
use mc_dfu::{DataResult, LONG, MapCodec};

/// `TicketType(timeout, flags)`; the registered instances are the associated constants (`TicketType.PLAYER_SPAWN`, ...).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TicketType {
    pub name: &'static str,
    pub timeout: i64,
    pub flags: u8,
}

impl TicketType {
    pub const NO_TIMEOUT: i64 = 0;
    pub const FLAG_PERSIST: u8 = 1;
    pub const FLAG_LOADING: u8 = 2;
    pub const FLAG_SIMULATION: u8 = 4;
    pub const FLAG_KEEP_DIMENSION_ACTIVE: u8 = 8;
    pub const FLAG_CAN_EXPIRE_IF_UNLOADED: u8 = 16;

    pub const PLAYER_SPAWN: TicketType = TicketType {
        name: "player_spawn",
        timeout: 20,
        flags: 2,
    };
    pub const SPAWN_SEARCH: TicketType = TicketType {
        name: "spawn_search",
        timeout: 1,
        flags: 2,
    };
    pub const DRAGON: TicketType = TicketType {
        name: "dragon",
        timeout: 0,
        flags: 6,
    };
    pub const PLAYER_LOADING: TicketType = TicketType {
        name: "player_loading",
        timeout: 0,
        flags: 2,
    };
    pub const PLAYER_SIMULATION: TicketType = TicketType {
        name: "player_simulation",
        timeout: 0,
        flags: 12,
    };
    pub const FORCED: TicketType = TicketType {
        name: "forced",
        timeout: 0,
        flags: 15,
    };
    pub const PORTAL: TicketType = TicketType {
        name: "portal",
        timeout: 300,
        flags: 15,
    };
    pub const ENDER_PEARL: TicketType = TicketType {
        name: "ender_pearl",
        timeout: 40,
        flags: 14,
    };
    pub const UNKNOWN: TicketType = TicketType {
        name: "unknown",
        timeout: 1,
        flags: 18,
    };

    /// `BuiltInRegistries.TICKET_TYPE` in registration order.
    pub const ALL: [TicketType; 9] = [
        Self::PLAYER_SPAWN,
        Self::SPAWN_SEARCH,
        Self::DRAGON,
        Self::PLAYER_LOADING,
        Self::PLAYER_SIMULATION,
        Self::FORCED,
        Self::PORTAL,
        Self::ENDER_PEARL,
        Self::UNKNOWN,
    ];

    pub fn by_name(name: &Identifier) -> Option<TicketType> {
        if name.namespace() != "minecraft" {
            return None;
        }
        Self::ALL.iter().copied().find(|t| t.name == name.path())
    }

    pub fn persist(&self) -> bool {
        self.flags & Self::FLAG_PERSIST != 0
    }
    pub fn does_load(&self) -> bool {
        self.flags & Self::FLAG_LOADING != 0
    }
    pub fn does_simulate(&self) -> bool {
        self.flags & Self::FLAG_SIMULATION != 0
    }
    pub fn should_keep_dimension_active(&self) -> bool {
        self.flags & Self::FLAG_KEEP_DIMENSION_ACTIVE != 0
    }
    pub fn can_expire_if_unloaded(&self) -> bool {
        self.flags & Self::FLAG_CAN_EXPIRE_IF_UNLOADED != 0
    }
    pub fn has_timeout(&self) -> bool {
        self.timeout != 0
    }
}

#[derive(Clone, Debug)]
pub struct Ticket {
    ty: TicketType,
    ticket_level: i32,
    ticks_left: i64,
}

impl Ticket {
    pub fn new(ty: TicketType, ticket_level: i32) -> Self {
        Self {
            ty,
            ticket_level,
            ticks_left: ty.timeout,
        }
    }

    pub fn get_type(&self) -> TicketType {
        self.ty
    }

    pub fn get_ticket_level(&self) -> i32 {
        self.ticket_level
    }

    pub fn reset_ticks_left(&mut self) {
        self.ticks_left = self.ty.timeout;
    }

    pub fn decrease_ticks_left(&mut self) {
        if self.ty.has_timeout() {
            self.ticks_left -= 1;
        }
    }

    pub fn is_timed_out(&self) -> bool {
        self.ty.has_timeout() && self.ticks_left < 0
    }

    pub fn ticks_left(&self) -> i64 {
        self.ticks_left
    }

    /// `Ticket.CODEC` (the `type`/`level`/`ticks_left` fields of a persisted ticket entry).
    pub fn codec() -> MapCodec<Ticket> {
        let ticket_type = IDENTIFIER.flat_xmap(
            |id| match TicketType::by_name(&id) {
                Some(ty) => DataResult::success(ty),
                None => DataResult::error(move || {
                    format!(
                        "Unknown registry key in ResourceKey[minecraft:root / minecraft:ticket_type]: {id}"
                    )
                }),
            },
            |ty: &TicketType| {
                DataResult::success(
                    Identifier::parse(&format!("minecraft:{}", ty.name)).expect("valid id"),
                )
            },
        );
        mc_dfu::record!(Ticket {
            ty: ticket_type.field_of("type"),
            ticket_level: NON_NEGATIVE_INT.field_of("level"),
            ticks_left: LONG.optional_field_of_default("ticks_left", 0),
        })
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ty.has_timeout() {
            // Vanilla's text really has "( out of" without a space after "of".
            write!(
                f,
                "Ticket[{} {}] with {} ticks left ( out of{})",
                self.ty.name, self.ticket_level, self.ticks_left, self.ty.timeout
            )
        } else {
            write!(
                f,
                "Ticket[{} {}] with no timeout",
                self.ty.name, self.ticket_level
            )
        }
    }
}

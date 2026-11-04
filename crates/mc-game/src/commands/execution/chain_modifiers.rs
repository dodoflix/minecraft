//! `net.minecraft.commands.execution.ChainModifiers`.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ChainModifiers {
    flags: u8,
}

impl ChainModifiers {
    pub const DEFAULT: ChainModifiers = ChainModifiers { flags: 0 };
    const FLAG_FORKED: u8 = 1;
    const FLAG_IS_RETURN: u8 = 2;

    pub fn is_forked(self) -> bool {
        self.flags & Self::FLAG_FORKED != 0
    }

    pub fn set_forked(self) -> ChainModifiers {
        ChainModifiers { flags: self.flags | Self::FLAG_FORKED }
    }

    pub fn is_return(self) -> bool {
        self.flags & Self::FLAG_IS_RETURN != 0
    }

    pub fn set_return(self) -> ChainModifiers {
        ChainModifiers { flags: self.flags | Self::FLAG_IS_RETURN }
    }
}

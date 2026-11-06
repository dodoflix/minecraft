//! `ClientboundMerchantOffersPacket` and its nested types.
use super::strict_item;
use crate::codec::DecodeResult;
use crate::codec::EncodeResult;
use crate::components::{DataComponentExactPredicate, ItemStack};
use crate::{Decode, Encode, Packet};

/// `ItemCost.STREAM_CODEC`: the item, how many, and the exact component values the offered stack must carry.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ItemCost {
    #[mc(with = strict_item)]
    pub item: mc_data::Item,
    #[mc(var)]
    pub count: i32,
    pub components: DataComponentExactPredicate,
}

/// `MerchantOffer.STREAM_CODEC`. The wire carries an out-of-stock flag next to `uses`: it is `uses >= max_uses` on
/// encode, and decoding a set flag sets `uses = max_uses` (vanilla's `setToOutOfStock`), so `uses` round-trips only
/// for offers that are consistent with that.
#[derive(Debug, Clone, PartialEq)]
pub struct MerchantOffer {
    pub base_cost_a: ItemCost,
    pub cost_b: Option<ItemCost>,
    /// Never empty (`ItemStack.STREAM_CODEC`).
    pub result: ItemStack,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub special_price_diff: i32,
    pub price_multiplier: f32,
    pub demand: i32,
}

impl Encode for MerchantOffer {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.base_cost_a.encode(out)?;
        self.result.encode_non_empty(out)?;
        self.cost_b.encode(out)?;
        (self.uses >= self.max_uses).encode(out)?;
        self.uses.encode(out)?;
        self.max_uses.encode(out)?;
        self.xp.encode(out)?;
        self.special_price_diff.encode(out)?;
        self.price_multiplier.encode(out)?;
        self.demand.encode(out)?;
        Ok(())
    }
}
impl Decode for MerchantOffer {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let base_cost_a = ItemCost::decode(input)?;
        let result = ItemStack::decode_non_empty(input)?;
        let cost_b = Option::<ItemCost>::decode(input)?;
        let out_of_stock = bool::decode(input)?;
        let mut uses = i32::decode(input)?;
        let max_uses = i32::decode(input)?;
        let xp = i32::decode(input)?;
        let special_price_diff = i32::decode(input)?;
        let price_multiplier = f32::decode(input)?;
        let demand = i32::decode(input)?;
        if out_of_stock {
            uses = max_uses;
        }
        Ok(Self {
            base_cost_a,
            cost_b,
            result,
            uses,
            max_uses,
            xp,
            special_price_diff,
            price_multiplier,
            demand,
        })
    }
}

/// `ClientboundMerchantOffersPacket` (`MerchantOffers` is a plain list on the wire)
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "merchant_offers")]
pub struct ClientboundMerchantOffersPacket {
    #[mc(var)]
    pub container_id: i32,
    pub offers: Vec<MerchantOffer>,
    #[mc(var)]
    pub villager_level: i32,
    #[mc(var)]
    pub villager_xp: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}

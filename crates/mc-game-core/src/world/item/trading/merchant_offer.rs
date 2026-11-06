//! `MerchantOffer` / `MerchantOffers`: one trade with its mutable trading state, its saved form (`Recipes`) and its wire form.

use std::ops::{Deref, DerefMut};

use mc_core::math::mth;
use mc_dfu::{BOOL, Codec, FLOAT, INT, record};
use mc_protocol::codec::{Decode, DecodeResult, Encode};
use mc_protocol::components::ItemStack;
use mc_protocol::packets::play::MerchantOffer as WireMerchantOffer;

use super::item_cost::ItemCost;

/// `MerchantOffer`. Equality is structural (item stacks compare by item/count/components).
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantOffer {
    base_cost_a: ItemCost,
    cost_b: Option<ItemCost>,
    result: ItemStack,
    uses: i32,
    max_uses: i32,
    reward_exp: bool,
    special_price_diff: i32,
    demand: i32,
    price_multiplier: f32,
    xp: i32,
}

impl MerchantOffer {
    /// `new MerchantOffer(buy, result, maxUses, xp, priceMultiplier)`.
    pub fn new(
        buy: ItemCost,
        result: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> MerchantOffer {
        MerchantOffer::with_cost_b(buy, None, result, max_uses, xp, price_multiplier)
    }

    /// `new MerchantOffer(baseCostA, costB, result, maxUses, xp, priceMultiplier)`.
    pub fn with_cost_b(
        base_cost_a: ItemCost,
        cost_b: Option<ItemCost>,
        result: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> MerchantOffer {
        MerchantOffer::with_uses(
            base_cost_a,
            cost_b,
            result,
            0,
            max_uses,
            xp,
            price_multiplier,
            0,
        )
    }

    /// `new MerchantOffer(baseCostA, costB, result, uses, maxUses, xp, priceMultiplier, demand)`.
    pub fn with_uses(
        base_cost_a: ItemCost,
        cost_b: Option<ItemCost>,
        result: ItemStack,
        uses: i32,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
        demand: i32,
    ) -> MerchantOffer {
        MerchantOffer {
            base_cost_a,
            cost_b,
            result,
            uses,
            max_uses,
            reward_exp: true,
            special_price_diff: 0,
            demand,
            price_multiplier,
            xp,
        }
    }

    /// `getBaseCostA()`.
    pub fn get_base_cost_a(&self) -> &ItemStack {
        self.base_cost_a.item_stack()
    }

    /// `getCostA()`: the base cost with demand and special price applied.
    pub fn get_cost_a(&self) -> ItemStack {
        self.base_cost_a
            .item_stack()
            .copy_with_count(self.get_modified_cost_count(&self.base_cost_a))
    }

    fn get_modified_cost_count(&self, cost: &ItemCost) -> i32 {
        let base_price = cost.count();
        // `basePrice * demand` is an int product, `* priceMultiplier` a float one
        let demand_diff =
            mth::floor_f32(base_price.wrapping_mul(self.demand) as f32 * self.price_multiplier)
                .max(0);
        mth::clamp_i32(
            base_price
                .wrapping_add(demand_diff)
                .wrapping_add(self.special_price_diff),
            1,
            cost.item_stack().get_max_stack_size(),
        )
    }

    /// `getCostB()`: `ItemStack.EMPTY` without a second cost.
    pub fn get_cost_b(&self) -> ItemStack {
        self.cost_b
            .as_ref()
            .map_or_else(ItemStack::empty, |c| c.item_stack().clone())
    }

    /// `getItemCostA()`.
    pub fn get_item_cost_a(&self) -> &ItemCost {
        &self.base_cost_a
    }

    /// `getItemCostB()`.
    pub fn get_item_cost_b(&self) -> Option<&ItemCost> {
        self.cost_b.as_ref()
    }

    /// `getResult()`.
    pub fn get_result(&self) -> &ItemStack {
        &self.result
    }

    /// `updateDemand()`.
    pub fn update_demand(&mut self) {
        self.demand = self
            .demand
            .wrapping_add(self.uses)
            .wrapping_sub(self.max_uses.wrapping_sub(self.uses));
    }

    /// `assemble()`.
    pub fn assemble(&self) -> ItemStack {
        self.result.copy()
    }

    pub fn get_uses(&self) -> i32 {
        self.uses
    }

    pub fn reset_uses(&mut self) {
        self.uses = 0;
    }

    pub fn get_max_uses(&self) -> i32 {
        self.max_uses
    }

    pub fn increase_uses(&mut self) {
        self.uses = self.uses.wrapping_add(1);
    }

    pub fn get_demand(&self) -> i32 {
        self.demand
    }

    pub fn add_to_special_price_diff(&mut self, add: i32) {
        self.special_price_diff = self.special_price_diff.wrapping_add(add);
    }

    pub fn reset_special_price_diff(&mut self) {
        self.special_price_diff = 0;
    }

    pub fn get_special_price_diff(&self) -> i32 {
        self.special_price_diff
    }

    pub fn set_special_price_diff(&mut self, value: i32) {
        self.special_price_diff = value;
    }

    pub fn get_price_multiplier(&self) -> f32 {
        self.price_multiplier
    }

    pub fn get_xp(&self) -> i32 {
        self.xp
    }

    /// `isOutOfStock()`.
    pub fn is_out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }

    /// `setToOutOfStock()`.
    pub fn set_to_out_of_stock(&mut self) {
        self.uses = self.max_uses;
    }

    /// `needsRestock()`.
    pub fn needs_restock(&self) -> bool {
        self.uses > 0
    }

    /// `shouldRewardExp()`.
    pub fn should_reward_exp(&self) -> bool {
        self.reward_exp
    }

    /// `satisfiedBy(buyA, buyB)`.
    pub fn satisfied_by(&self, buy_a: &ItemStack, buy_b: &ItemStack) -> bool {
        if !self.base_cost_a.test(buy_a)
            || buy_a.count() < self.get_modified_cost_count(&self.base_cost_a)
        {
            return false;
        }
        match &self.cost_b {
            None => buy_b.is_empty(),
            Some(b) => b.test(buy_b) && buy_b.count() >= b.count(),
        }
    }

    /// `take(buyA, buyB)`: pays the (modified) costs out of the given stacks when they satisfy the offer.
    pub fn take(&self, buy_a: &mut ItemStack, buy_b: &mut ItemStack) -> bool {
        if !self.satisfied_by(buy_a, buy_b) {
            return false;
        }
        buy_a.shrink(self.get_cost_a().count());
        let cost_b = self.get_cost_b();
        if !cost_b.is_empty() {
            buy_b.shrink(cost_b.count());
        }
        true
    }

    /// `MerchantOffer.CODEC` (the saved form).
    pub fn codec() -> Codec<MerchantOffer> {
        record!(MerchantOffer {
            base_cost_a: ItemCost::codec().field_of("buy"),
            cost_b: ItemCost::codec().lenient_optional_field_of("buyB"),
            result: ItemStack::codec().field_of("sell"),
            uses: INT.lenient_optional_field_of_default("uses", 0),
            max_uses: INT.lenient_optional_field_of_default("maxUses", 4),
            reward_exp: BOOL.lenient_optional_field_of_default("rewardExp", true),
            special_price_diff: INT.lenient_optional_field_of_default("specialPrice", 0),
            demand: INT.lenient_optional_field_of_default("demand", 0),
            price_multiplier: FLOAT.lenient_optional_field_of_default("priceMultiplier", 0.0),
            xp: INT.lenient_optional_field_of_default("xp", 1),
        })
        .codec()
    }

    /// The `MerchantOffer.STREAM_CODEC` value; the encoding itself lives in `mc-protocol`
    /// (`ClientboundMerchantOffersPacket`).
    pub fn to_wire(&self) -> WireMerchantOffer {
        WireMerchantOffer {
            base_cost_a: self.base_cost_a.to_wire(),
            cost_b: self.cost_b.as_ref().map(ItemCost::to_wire),
            result: self.result.clone(),
            uses: self.uses,
            max_uses: self.max_uses,
            xp: self.xp,
            special_price_diff: self.special_price_diff,
            price_multiplier: self.price_multiplier,
            demand: self.demand,
        }
    }

    /// `MerchantOffer.createFromStream`'s result (the wire decoder already applied the out-of-stock flag).
    pub fn from_wire(wire: WireMerchantOffer) -> MerchantOffer {
        let mut offer = MerchantOffer::with_uses(
            ItemCost::from_wire(wire.base_cost_a),
            wire.cost_b.map(ItemCost::from_wire),
            wire.result,
            wire.uses,
            wire.max_uses,
            wire.xp,
            wire.price_multiplier,
            wire.demand,
        );
        offer.set_special_price_diff(wire.special_price_diff);
        offer
    }
}

/// `MerchantOffers` (an `ArrayList<MerchantOffer>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MerchantOffers(pub Vec<MerchantOffer>);

impl Deref for MerchantOffers {
    type Target = Vec<MerchantOffer>;
    fn deref(&self) -> &Vec<MerchantOffer> {
        &self.0
    }
}

impl DerefMut for MerchantOffers {
    fn deref_mut(&mut self) -> &mut Vec<MerchantOffer> {
        &mut self.0
    }
}

impl MerchantOffers {
    pub fn new() -> MerchantOffers {
        MerchantOffers(Vec::new())
    }

    /// `getRecipeFor(buyA, buyB, selectionHint)`.
    pub fn get_recipe_for(
        &mut self,
        buy_a: &ItemStack,
        buy_b: &ItemStack,
        selection_hint: i32,
    ) -> Option<&mut MerchantOffer> {
        if selection_hint > 0 && (selection_hint as usize) < self.0.len() {
            let offer = &mut self.0[selection_hint as usize];
            return if offer.satisfied_by(buy_a, buy_b) {
                Some(offer)
            } else {
                None
            };
        }
        self.0.iter_mut().find(|o| o.satisfied_by(buy_a, buy_b))
    }

    /// `MerchantOffers.CODEC`: `{Recipes: [...]}` (absent when empty).
    pub fn codec() -> Codec<MerchantOffers> {
        MerchantOffer::codec()
            .list_of()
            .optional_field_of_default("Recipes", Vec::new())
            .xmap(MerchantOffers, |o: &MerchantOffers| o.0.clone())
            .codec()
    }

    /// `MerchantOffers.STREAM_CODEC` value: the offers as the packet carries them.
    pub fn to_wire(&self) -> Vec<WireMerchantOffer> {
        self.0.iter().map(MerchantOffer::to_wire).collect()
    }

    pub fn from_wire(wire: Vec<WireMerchantOffer>) -> MerchantOffers {
        MerchantOffers(wire.into_iter().map(MerchantOffer::from_wire).collect())
    }

    /// `MerchantOffers.STREAM_CODEC.encode`: a var-int count, then every offer.
    pub fn encode_stream(&self, out: &mut Vec<u8>) -> mc_protocol::codec::EncodeResult<()> {
        mc_protocol::codec::write_varint(out, self.0.len() as i32);
        for o in &self.0 {
            o.to_wire().encode(out)?;
        }
        Ok(())
    }

    /// `MerchantOffers.STREAM_CODEC.decode`.
    pub fn decode_stream(input: &mut &[u8]) -> DecodeResult<MerchantOffers> {
        let len = mc_protocol::codec::read_varint(input)?;
        let mut offers = Vec::new();
        for _ in 0..len.max(0) {
            offers.push(MerchantOffer::from_wire(WireMerchantOffer::decode(input)?));
        }
        Ok(MerchantOffers(offers))
    }
}

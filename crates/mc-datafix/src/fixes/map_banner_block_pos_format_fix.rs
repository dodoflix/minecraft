//! `MapBannerBlockPosFormatFix`: converts the banner positions of map data to the int list block position.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::fix_block_pos;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new MapBannerBlockPosFormatFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("MapBannerBlockPosFormatFix", output_schema, false, |df| {
        let ty = df.input_schema().get_type(&References::SAVED_DATA_MAP_DATA);
        let data_f = ty.find_field("data");
        let banners_f = data_f.ty().find_field("banners");
        let banner_f = DSL::type_finder(
            banners_f
                .ty()
                .as_list()
                .expect("the banners of map data are a list"),
        );
        df.fix_type_everywhere_typed("MapBannerBlockPosFormatFix", &ty, move |input| {
            input.update_typed(&data_f, |data| {
                data.update_typed(&banners_f, |banners| {
                    banners.update_typed(&banner_f, |banner| {
                        banner.update_remainder(|banner_tag| {
                            banner_tag.update("Pos", |pos| fix_block_pos(&pos))
                        })
                    })
                })
            })
        })
    })
}

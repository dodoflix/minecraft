//! `ClientAsset.ResourceTexture` and `ModelAndTexture<T>`: the client-asset half of the animal variants (kept so the registry
//! data re-encodes exactly; the server never reads them).

use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, MapCodec, record};

/// `ClientAsset.ResourceTexture(id, texturePath)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceTexture {
    pub id: Identifier,
    pub texture_path: Identifier,
}

impl ResourceTexture {
    /// `new ResourceTexture(texture)`: the path is `textures/<path>.png`.
    pub fn new(id: Identifier) -> ResourceTexture {
        let texture_path = Identifier::new(id.namespace(), &format!("textures/{}.png", id.path())).expect("derived texture path is valid");
        ResourceTexture { id, texture_path }
    }

    /// `ResourceTexture.CODEC`: the asset id.
    pub fn codec() -> Codec<ResourceTexture> {
        IDENTIFIER.xmap(ResourceTexture::new, |t: &ResourceTexture| t.id.clone())
    }

    /// `ResourceTexture.DEFAULT_FIELD_CODEC`: the `asset_id` field.
    pub fn default_field_codec() -> MapCodec<ResourceTexture> {
        Self::codec().field_of("asset_id")
    }
}

/// `ModelAndTexture<T>(model, asset)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelAndTexture<T> {
    pub model: T,
    pub asset: ResourceTexture,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> ModelAndTexture<T> {
    /// `ModelAndTexture.codec(modelCodec, defaultModel)`.
    pub fn codec(model_codec: &Codec<T>, default_model: T) -> MapCodec<ModelAndTexture<T>> {
        record!(ModelAndTexture<T> {
            model: model_codec.optional_field_of_default("model", default_model),
            asset: ResourceTexture::default_field_codec(),
        })
    }
}

/// A `StringRepresentable` enum with a `fromEnum` codec (the animal `ModelType`s).
#[macro_export]
macro_rules! model_type {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            const VALUES: &'static [($name, &'static str)] = &[$(($name::$variant, $text)),+];

            /// `getSerializedName()`.
            pub fn serialized_name(self) -> &'static str {
                Self::VALUES.iter().find(|(v, _)| *v == self).map(|(_, n)| *n).expect("listed")
            }

            /// `ModelType.CODEC`.
            pub fn codec() -> mc_dfu::Codec<$name> {
                mc_dfu::string_enum_codec(Self::VALUES)
            }
        }
    };
}
pub use model_type;

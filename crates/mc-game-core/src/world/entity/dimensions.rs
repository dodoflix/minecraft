//! `EntityDimensions`, `EntityAttachments`, `EntityAttachment`.
use mc_core::phys::{AABB, Vec3};
use mc_data::EntityType;
use mc_data::entity_type::attachment;

/// `EntityAttachment` (index = ordinal, also the index into `EntityTypeData::attachments`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityAttachment {
    Passenger = attachment::PASSENGER as isize,
    Vehicle = attachment::VEHICLE as isize,
    NameTag = attachment::NAME_TAG as isize,
    WardenChest = attachment::WARDEN_CHEST as isize,
}

/// `EntityAttachments`: every attachment type always has at least one point (fallbacks are already applied).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityAttachments {
    attachments: [Vec<Vec3>; 4],
}

impl EntityAttachments {
    /// `createDefault(width, height)`: PASSENGER/NAME_TAG at the height, VEHICLE at the feet, WARDEN_CHEST at the center.
    pub fn create_default(_width: f32, height: f32) -> Self {
        Self {
            attachments: [
                vec![Vec3::new(0.0, f64::from(height), 0.0)],
                vec![Vec3::ZERO],
                vec![Vec3::new(0.0, f64::from(height), 0.0)],
                vec![Vec3::new(0.0, f64::from(height / 2.0), 0.0)],
            ],
        }
    }

    /// `Builder.attach(PASSENGER, point)` on top of the defaults.
    pub fn with_passenger(mut self, point: Vec3) -> Self {
        self.attachments[EntityAttachment::Passenger as usize] = vec![point];
        self
    }

    /// `Builder.attach(VEHICLE, point)` on top of the defaults.
    pub fn with_vehicle(mut self, point: Vec3) -> Self {
        self.attachments[EntityAttachment::Vehicle as usize] = vec![point];
        self
    }

    pub fn scale(&self, x: f32, y: f32, z: f32) -> Self {
        let f = |list: &Vec<Vec3>| {
            list.iter()
                .map(|v| v.multiply_xyz(f64::from(x), f64::from(y), f64::from(z)))
                .collect::<Vec<_>>()
        };
        Self {
            attachments: [
                f(&self.attachments[0]),
                f(&self.attachments[1]),
                f(&self.attachments[2]),
                f(&self.attachments[3]),
            ],
        }
    }

    fn transform_point(point: Vec3, rot_y: f32) -> Vec3 {
        point.y_rot(-rot_y * (std::f64::consts::PI / 180.0) as f32)
    }

    pub fn get_nullable(
        &self,
        attachment: EntityAttachment,
        index: i32,
        rot_y: f32,
    ) -> Option<Vec3> {
        let points = &self.attachments[attachment as usize];
        (index >= 0 && (index as usize) < points.len())
            .then(|| Self::transform_point(points[index as usize], rot_y))
    }

    /// `get`: panics like vanilla's `IllegalStateException` when the index has no point.
    pub fn get(&self, attachment: EntityAttachment, index: i32, rot_y: f32) -> Vec3 {
        self.get_nullable(attachment, index, rot_y)
            .unwrap_or_else(|| {
                panic!("Had no attachment point of type: {attachment:?} for index: {index}")
            })
    }

    pub fn get_average(&self, attachment: EntityAttachment) -> Vec3 {
        let points = &self.attachments[attachment as usize];
        assert!(
            !points.is_empty(),
            "No attachment points of type: PASSENGER"
        );
        let mut sum = Vec3::ZERO;
        for p in points {
            sum = sum.add(*p);
        }
        sum.scale(f64::from(1.0f32 / points.len() as f32))
    }

    pub fn get_clamped(&self, attachment: EntityAttachment, index: i32, rot_y: f32) -> Vec3 {
        let points = &self.attachments[attachment as usize];
        assert!(
            !points.is_empty(),
            "Had no attachment points of type: {attachment:?}"
        );
        let i = index.clamp(0, points.len() as i32 - 1) as usize;
        Self::transform_point(points[i], rot_y)
    }
}

/// `EntityDimensions` (floats, as vanilla).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityDimensions {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub attachments: EntityAttachments,
    pub fixed: bool,
}

impl EntityDimensions {
    fn new(width: f32, height: f32, fixed: bool) -> Self {
        Self {
            width,
            height,
            eye_height: height * 0.85,
            attachments: EntityAttachments::create_default(width, height),
            fixed,
        }
    }

    pub fn scalable(width: f32, height: f32) -> Self {
        Self::new(width, height, false)
    }

    pub fn fixed(width: f32, height: f32) -> Self {
        Self::new(width, height, true)
    }

    /// `EntityType.getDimensions()`.
    pub fn of_type(ty: EntityType) -> Self {
        let d = ty.data();
        let points = |i: usize| {
            d.attachments[i]
                .iter()
                .map(|p| Vec3::new(p[0], p[1], p[2]))
                .collect::<Vec<_>>()
        };
        Self {
            width: d.width,
            height: d.height,
            eye_height: d.eye_height,
            attachments: EntityAttachments {
                attachments: [points(0), points(1), points(2), points(3)],
            },
            fixed: d.fixed,
        }
    }

    pub fn make_bounding_box_vec(&self, pos: Vec3) -> AABB {
        self.make_bounding_box(pos.x, pos.y, pos.z)
    }

    pub fn make_bounding_box(&self, x: f64, y: f64, z: f64) -> AABB {
        let w = f64::from(self.width / 2.0);
        let h = f64::from(self.height);
        AABB::new(x - w, y, z - w, x + w, y + h, z + w)
    }

    pub fn scale(&self, scale_factor: f32) -> Self {
        self.scale_xy(scale_factor, scale_factor)
    }

    pub fn scale_xy(&self, width_scale_factor: f32, height_scale_factor: f32) -> Self {
        if !self.fixed && (width_scale_factor != 1.0 || height_scale_factor != 1.0) {
            Self {
                width: self.width * width_scale_factor,
                height: self.height * height_scale_factor,
                eye_height: self.eye_height * height_scale_factor,
                attachments: self.attachments.scale(
                    width_scale_factor,
                    height_scale_factor,
                    width_scale_factor,
                ),
                fixed: false,
            }
        } else {
            self.clone()
        }
    }

    pub fn with_eye_height(&self, eye_height: f32) -> Self {
        Self {
            eye_height,
            ..self.clone()
        }
    }

    /// `withAttachments(builder)`.
    pub fn with_attachments(&self, attachments: EntityAttachments) -> Self {
        Self {
            attachments,
            ..self.clone()
        }
    }
}

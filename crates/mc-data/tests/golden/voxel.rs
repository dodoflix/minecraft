use mc_data::shape::{shape_count, ShapeId};

#[test]
fn voxel_shapes_match_extracted_aabbs() {
    for id in 0..shape_count() {
        let id = ShapeId::from_id(id as u32).unwrap();
        let got: Vec<[f64; 6]> =
            id.voxel_shape().to_aabbs().iter().map(|a| [a.min_x, a.min_y, a.min_z, a.max_x, a.max_y, a.max_z]).collect();
        assert_eq!(got.as_slice(), id.data().aabbs, "shape {}", id.id());
        assert_eq!(id.voxel_shape().is_empty(), id.is_empty(), "shape {}", id.id());
    }
}

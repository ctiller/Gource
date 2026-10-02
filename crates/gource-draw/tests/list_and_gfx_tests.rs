use gource_core::{UVec2, Vec2, Vec4};
use gource_draw::Gfx;
use gource_draw::list::{DrawList, Material, TextureId, Vertex};

#[test]
fn test_drawlist_all_primitives() {
    let mut list = DrawList::default();
    assert_eq!(list.viewport, UVec2::new(1, 1));
    assert_eq!(list.width(), 1.0);
    assert_eq!(list.height(), 1.0);
    assert!(list.is_empty());
    assert_eq!(list.vertex_count(), 0);

    list.reset(UVec2::new(800, 600), Vec4::new(0.1, 0.2, 0.3, 1.0));
    assert_eq!(list.width(), 800.0);
    assert_eq!(list.height(), 600.0);
    assert_eq!(list.clear_colour, Vec4::new(0.1, 0.2, 0.3, 1.0));

    // rect
    list.rect(
        TextureId(1),
        Vec2::new(10.0, 10.0),
        Vec2::new(50.0, 50.0),
        Vec4::ONE,
    );
    assert!(!list.is_empty());
    assert_eq!(list.vertex_count(), 4);

    // line
    list.line(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), 2.0, Vec4::ONE);
    // line with length 0 should be no-op
    list.line(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0), 2.0, Vec4::ONE);

    // rect_outline
    list.rect_outline(
        Vec2::new(10.0, 10.0),
        Vec2::new(100.0, 100.0),
        1.0,
        Vec4::ONE,
    );

    // triangles with empty indices
    list.triangles(Material::Alpha, TextureId::WHITE, &[], &[]);

    // push_quad
    let corners = [
        Vertex::new(Vec2::ZERO, Vec2::ZERO, Vec4::ONE),
        Vertex::new(Vec2::X, Vec2::X, Vec4::ONE),
        Vertex::new(Vec2::ONE, Vec2::ONE, Vec4::ONE),
        Vertex::new(Vec2::Y, Vec2::Y, Vec4::ONE),
    ];
    list.push_quad(Material::Alpha, TextureId::WHITE, corners);
}

#[test]
fn test_gfx_facade() {
    let mut gfx = Gfx::default();
    let face = gfx.fonts.default_face();
    let font = gfx.fonts.font(face, 18);

    let mut list = DrawList::new(UVec2::new(640, 480));
    gfx.draw_text(
        &mut list,
        font,
        Vec2::new(50.0, 50.0),
        "Testing Gfx Facade",
        &gource_draw::TextStyle::default(),
    );

    let w = gfx.text_width(font, "Testing Gfx Facade");
    assert!(w > 0.0);
    assert!(!list.is_empty());
}

#[test]
fn test_drawlist_bloom_faded() {
    let mut a = DrawList::new(UVec2::new(100, 100));
    a.bloom(Vec2::new(50.0, 50.0), 20.0, Vec4::new(0.8, 0.6, 0.4, 1.0));
    let mut b = DrawList::new(UVec2::new(100, 100));
    b.append_faded(&a, 0.5);

    assert_eq!(b.batches.len(), 1);
    assert_eq!(b.batches[0].material, Material::Bloom);
    // Colour was scaled by alpha 0.5
    let v = &b.batches[0].vertices[0];
    assert!((v.colour.x - 0.4).abs() < 1e-4);
    assert!((v.colour.y - 0.3).abs() < 1e-4);
    assert!((v.colour.z - 0.2).abs() < 1e-4);
    assert!((v.colour.w - 0.5).abs() < 1e-4);
}

#[test]
fn test_projection_to_screen_len() {
    let p = gource_draw::Projection::new(
        gource_core::Vec3::new(0.0, 0.0, -100.0),
        Vec2::new(800.0, 600.0),
    );
    let len_screen = p.to_screen_len(10.0);
    assert!((len_screen - 30.0).abs() < 1e-4);
}

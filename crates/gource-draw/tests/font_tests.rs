use gource_core::{Vec2, Vec4};
use gource_draw::font::{FontStore, TextStyle};
use gource_draw::list::{DrawList, Material};
use gource_draw::texture::TextureStore;
use std::collections::HashMap;

#[allow(dead_code)]
struct FreeTypeGlyph {
    adv_x: i32,
    adv_y: i32,
    h: i32,
    left: i32,
    top: i32,
    w: i32,
    rows: i32,
}

#[allow(dead_code)]
struct FreeTypeSize {
    size: u32,
    ascender: f32,
    descender: f32,
    height: f32,
    max_width: f32,
    glyphs: HashMap<char, FreeTypeGlyph>,
}

#[allow(dead_code)]
fn load_goldens() -> Vec<FreeTypeSize> {
    let content = include_str!("data/freesans_metrics.txt");
    let mut sizes: Vec<FreeTypeSize> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("SIZE") {
            // e.g. SIZE 10 ascender=10.000000 descender=-3.000000 height=7.0 max_width=15.000000
            let parts: Vec<&str> = line.split_whitespace().collect();
            let size: u32 = parts[1].parse().unwrap();
            let asc: f32 = parts[2].split('=').nth(1).unwrap().parse().unwrap();
            let desc: f32 = parts[3].split('=').nth(1).unwrap().parse().unwrap();
            let h: f32 = parts[4].split('=').nth(1).unwrap().parse().unwrap();
            let mw: f32 = parts[5].split('=').nth(1).unwrap().parse().unwrap();
            sizes.push(FreeTypeSize {
                size,
                ascender: asc,
                descender: desc,
                height: h,
                max_width: mw,
                glyphs: HashMap::new(),
            });
        } else if line.starts_with("GLYPH") {
            // e.g. GLYPH size=10 char=32 adv_x=3 adv_y=0 h=0 left=0 top=0 w=0 rows=0
            let parts: Vec<&str> = line.split_whitespace().collect();
            let mut char_code: u32 = 0;
            let mut adv_x: i32 = 0;
            let mut adv_y: i32 = 0;
            let mut h: i32 = 0;
            let mut left: i32 = 0;
            let mut top: i32 = 0;
            let mut w: i32 = 0;
            let mut rows: i32 = 0;

            for part in &parts[1..] {
                let mut kv = part.split('=');
                let key = kv.next().unwrap();
                let val = kv.next().unwrap();
                match key {
                    "char" => char_code = val.parse().unwrap(),
                    "adv_x" => adv_x = val.parse().unwrap(),
                    "adv_y" => adv_y = val.parse().unwrap(),
                    "h" => h = val.parse().unwrap(),
                    "left" => left = val.parse().unwrap(),
                    "top" => top = val.parse().unwrap(),
                    "w" => w = val.parse().unwrap(),
                    "rows" => rows = val.parse().unwrap(),
                    _ => {}
                }
            }

            let ch = char::from_u32(char_code).unwrap();
            if let Some(cur_size) = sizes.last_mut() {
                cur_size.glyphs.insert(
                    ch,
                    FreeTypeGlyph {
                        adv_x,
                        adv_y,
                        h,
                        left,
                        top,
                        w,
                        rows,
                    },
                );
            }
        }
    }

    sizes
}

#[cfg(feature = "ab-glyph")]
#[test]
fn test_font_metrics_parity_with_freetype() {
    let goldens = load_goldens();
    let mut store = FontStore::new();
    let mut textures = TextureStore::new();
    let default_face = store.default_face();

    let mut total_glyphs = 0;
    let mut adv_matches = 0;
    let mut total_visual_glyphs = 0;
    let mut box_within_1px = 0;

    for g_size in &goldens {
        let font_id = store.font(default_face, g_size.size);

        // Check ascender, descender, height()
        assert!(
            (store.ascender(font_id) - g_size.ascender).abs() < 1e-4,
            "size {}: ascender {} vs {}",
            g_size.size,
            store.ascender(font_id),
            g_size.ascender
        );
        assert!(
            (store.descender(font_id) - g_size.descender).abs() < 1e-4,
            "size {}: descender {} vs {}",
            g_size.size,
            store.descender(font_id),
            g_size.descender
        );
        assert_eq!(
            store.height(font_id),
            g_size.height,
            "size {}: height {} vs {}",
            g_size.size,
            store.height(font_id),
            g_size.height
        );

        for (&ch, ft_glyph) in &g_size.glyphs {
            total_glyphs += 1;
            let str_val = ch.to_string();
            let w = store.width(font_id, &str_val);
            if (w - ft_glyph.adv_x as f32).abs() < 1e-4 {
                adv_matches += 1;
            } else {
                eprintln!(
                    "Adv mismatch at size {} char '{}' (U+{:04X}): ab_glyph {} vs FreeType {}",
                    g_size.size, ch, ch as u32, w, ft_glyph.adv_x
                );
            }

            // Compare bitmap bounding box by drawing into a list and inspecting quad dims
            if ft_glyph.w > 0 && ft_glyph.rows > 0 {
                total_visual_glyphs += 1;
                let mut list = DrawList::new(gource_core::UVec2::new(100, 100));
                store.draw(
                    &mut textures,
                    &mut list,
                    font_id,
                    Vec2::ZERO,
                    &str_val,
                    &TextStyle::default(),
                );
                if !list.batches.is_empty() && !list.batches[0].vertices.is_empty() {
                    let v = &list.batches[0].vertices;
                    let quad_w = v[1].pos.x - v[0].pos.x;
                    let quad_h = v[2].pos.y - v[1].pos.y;
                    let rendered_w = quad_w - 2.0;
                    let rendered_h = quad_h - 2.0;

                    let dw = (rendered_w - ft_glyph.w as f32).abs();
                    let dh = (rendered_h - ft_glyph.rows as f32).abs();
                    if dw <= 1.5 && dh <= 1.5 {
                        box_within_1px += 1;
                    }
                }
            }
        }
    }

    let adv_match_rate = adv_matches as f64 / total_glyphs as f64;
    let box_match_rate = box_within_1px as f64 / total_visual_glyphs as f64;
    println!(
        "Font metric parity: {} / {} advances match ({:.2}%)",
        adv_matches,
        total_glyphs,
        adv_match_rate * 100.0
    );
    println!(
        "Glyph bitmap box parity: {} / {} within +-1px ({:.2}%)",
        box_within_1px,
        total_visual_glyphs,
        box_match_rate * 100.0
    );

    // Advances must have high agreement (>= 98%)
    assert!(
        adv_match_rate >= 0.98,
        "Advance match rate too low: {:.2}%",
        adv_match_rate * 100.0
    );
    // Bounding boxes within +-1px should be >= 95%
    assert!(
        box_match_rate >= 0.95,
        "Box match rate too low: {:.2}%",
        box_match_rate * 100.0
    );
}

#[test]
fn test_text_rendering_layout_and_quads() {
    let mut fonts = FontStore::new();
    let mut textures = TextureStore::new();
    let face = fonts.default_face();
    let font = fonts.font(face, 16);

    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
    let text = "Hello World!";
    let style = TextStyle::default().with_colour(Vec4::new(1.0, 0.0, 0.0, 1.0));

    fonts.draw(
        &mut textures,
        &mut list,
        font,
        Vec2::new(100.0, 200.0),
        text,
        &style,
    );

    assert_eq!(list.batches.len(), 1);
    let batch = &list.batches[0];
    assert_eq!(batch.material, Material::Alpha);
    // ' ' space has no visual quad, so 11 visual glyphs * 4 vertices = 44 vertices
    assert_eq!(batch.vertices.len(), 11 * 4);
    assert_eq!(batch.indices.len(), 11 * 6);
}

#[test]
fn test_shadow_and_alignment() {
    let mut fonts = FontStore::new();
    let mut textures = TextureStore::new();
    let face = fonts.default_face();
    let font = fonts.font(face, 16);

    // Test align_right, align_top, round, and shadow
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
    let style = TextStyle::default()
        .with_colour(Vec4::new(1.0, 1.0, 1.0, 1.0))
        .with_align_right(true)
        .with_align_top(true)
        .with_round(true)
        .with_shadow(true);

    fonts.draw(
        &mut textures,
        &mut list,
        font,
        Vec2::new(150.3, 250.7),
        "Test",
        &style,
    );

    let batch = &list.batches[0];
    // Shadow quads + text quads = 4 glyphs * 2 * 4 = 32 vertices
    assert_eq!(batch.vertices.len(), 32);

    // First quad is shadow (colour rgb is 0, alpha is 0.7)
    let shadow_v = &batch.vertices[0];
    assert_eq!(shadow_v.colour.x, 0.0);
    assert_eq!(shadow_v.colour.y, 0.0);
    assert_eq!(shadow_v.colour.z, 0.0);
    assert!((shadow_v.colour.w - 0.7).abs() < 1e-4);

    // Corresponding text quad has colour 1.0, 1.0, 1.0, 1.0
    let text_v = &batch.vertices[16];
    assert_eq!(text_v.colour, Vec4::ONE);
}

#[test]
fn test_tabs_and_empty_string() {
    let mut fonts = FontStore::new();
    let mut textures = TextureStore::new();
    let face = fonts.default_face();
    let font = fonts.font(face, 16);

    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
    // Empty string should produce nothing
    fonts.draw(
        &mut textures,
        &mut list,
        font,
        Vec2::ZERO,
        "",
        &TextStyle::default(),
    );
    assert!(list.is_empty());

    // Tab string
    fonts.draw(
        &mut textures,
        &mut list,
        font,
        Vec2::ZERO,
        "A\tB",
        &TextStyle::default(),
    );
    let batch = &list.batches[0];
    // Two letters 'A' and 'B'
    assert_eq!(batch.vertices.len(), 8);
    // B position should be separated from A by advance(A) + 4 * advance('M')
    let a_adv = fonts.width(font, "A");
    let m_adv = fonts.width(font, "M");
    let expected_b_x = a_adv + 4.0 * m_adv;
    let b_quad_x = batch.vertices[4].pos.x;
    let a_quad_x = batch.vertices[0].pos.x;
    assert!((b_quad_x - a_quad_x - expected_b_x).abs() < 2.0);
}

#[test]
fn test_atlas_overflow_multiple_pages() {
    let mut fonts = FontStore::new();
    let mut textures = TextureStore::new();
    let face = fonts.default_face();
    let font = fonts.font(face, 48); // Large glyph size

    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Render many distinct unicode characters to force atlas page overflow
    let mut large_text = String::new();
    for i in 32..500 {
        if let Some(c) = char::from_u32(i) {
            large_text.push(c);
        }
    }

    fonts.draw(
        &mut textures,
        &mut list,
        font,
        Vec2::ZERO,
        &large_text,
        &TextStyle::default(),
    );

    // Textures should have more than 1 atlas page created
    // id 0 is WHITE, id 1 is page 0, id 2 is page 1, ...
    let texture_count = textures.iter().count();
    assert!(
        texture_count >= 3,
        "Expected multiple atlas pages, found {}",
        texture_count
    );
}

#[test]
fn test_text_style_builders() {
    let style = TextStyle::new(Vec4::new(0.5, 0.5, 0.5, 1.0))
        .with_colour(Vec4::new(1.0, 0.0, 0.0, 1.0))
        .with_alpha(0.8)
        .with_shadow(true)
        .with_align_top(false)
        .with_align_right(true)
        .with_round(true);

    assert_eq!(style.colour, Vec4::new(1.0, 0.0, 0.0, 0.8));
    assert!(style.shadow);
    assert!(!style.align_top);
    assert!(style.align_right);
    assert!(style.round);
}

#[test]
fn test_font_store_files_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FontStore::new();

    // Load from valid file
    let font_path = dir.path().join("FreeSans.ttf");
    std::fs::write(&font_path, gource_draw::resources::FREESANS_TTF).unwrap();
    let face_file = store.load_face_file(&font_path).expect("load face file");
    let face_file_cached = store.load_face_file(&font_path).unwrap();
    assert_eq!(face_file, face_file_cached);

    // Non-existent file error
    let bad_path = dir.path().join("non_existent.ttf");
    assert!(store.load_face_file(&bad_path).is_err());

    // Invalid font data error
    assert!(store.load_face("invalid", vec![1, 2, 3, 4]).is_err());

    // Check size(), max_width(), max_height(), ascender(), descender(), height()
    let font_id = store.font(face_file, 16);
    assert_eq!(store.size(font_id), 16);
    assert!(store.max_width(font_id) > 0.0);
    assert!(store.max_height(font_id) > 0.0);
    assert!(store.ascender(font_id) > 0.0);
    assert!(store.descender(font_id) < 0.0);
    assert!(store.height(font_id) > 0.0);

    // Invalid FontId
    let bad_font = gource_draw::FontId(999);
    assert_eq!(store.size(bad_font), 0);
    assert_eq!(store.max_width(bad_font), 0.0);
    assert_eq!(store.max_height(bad_font), 0.0);
    assert_eq!(store.ascender(bad_font), 0.0);
    assert_eq!(store.descender(bad_font), 0.0);
    assert_eq!(store.height(bad_font), 0.0);
    assert_eq!(store.width(bad_font, "abc"), 0.0);

    // Missing glyph behaves without crashing
    let mut textures = TextureStore::new();
    let mut list = DrawList::new(gource_core::UVec2::new(100, 100));
    // Character from private use area or undefined
    store.draw(
        &mut textures,
        &mut list,
        font_id,
        Vec2::ZERO,
        "\u{E000}",
        &TextStyle::default(),
    );

    // Test load_face caching (calling load_face twice with same name)
    let face_mem1 = store
        .load_face("mem_font", gource_draw::resources::FREESANS_TTF.to_vec())
        .expect("load face mem");
    let face_mem2 = store
        .load_face("mem_font", gource_draw::resources::FREESANS_TTF.to_vec())
        .expect("load face mem cached");
    assert_eq!(face_mem1, face_mem2);

    // Test font() caching (calling font with same FaceId and size)
    let f1 = store.font(face_mem1, 14);
    let f2 = store.font(face_mem1, 14);
    assert_eq!(f1, f2);

    // Test default_face() caching
    let df1 = store.default_face();
    let df2 = store.default_face();
    assert_eq!(df1, df2);

    // Test drawing whitespace string (space has outline but width=0/height=0)
    store.draw(
        &mut textures,
        &mut list,
        font_id,
        Vec2::ZERO,
        "   ",
        &TextStyle::default(),
    );

    // Test a font with size > atlas page width (512)
    let giant_font = store.font(df1, 600);
    store.draw(
        &mut textures,
        &mut list,
        giant_font,
        Vec2::ZERO,
        "W",
        &TextStyle::default(),
    );
}

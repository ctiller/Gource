use gource_core::UVec2;
use gource_draw::TextureId;
use gource_draw::texture::{Filter, TextureOptions, TextureStore, Wrap};
use image::{ImageBuffer, Rgb, Rgba};
use tempfile::tempdir;

#[test]
fn test_white_texture_always_present() {
    let mut store = TextureStore::new();
    let white = store.get(TextureId::WHITE).expect("WHITE must exist");
    assert_eq!(white.name, "WHITE");
    assert_eq!(white.width, 1);
    assert_eq!(white.height, 1);
    assert_eq!(white.levels.len(), 1);
    assert_eq!(white.levels[0], vec![255, 255, 255, 255]);
    assert_eq!(store.size(TextureId::WHITE), UVec2::new(1, 1));

    // Trying to remove WHITE should be a no-op
    store.remove(TextureId::WHITE);
    assert!(store.get(TextureId::WHITE).is_some());
}

#[test]
fn test_create_rgba_validation() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut store = TextureStore::new();
        store.create_rgba("invalid", 2, 2, vec![0; 15], TextureOptions::plain());
    }));
    assert!(result.is_err());
}

#[test]
fn test_create_and_remove_rgba() {
    let mut store = TextureStore::new();
    let id1 = store.create_rgba("tex1", 2, 2, vec![10; 16], TextureOptions::plain());
    assert_eq!(id1.0, 1);
    assert_eq!(store.size(id1), UVec2::new(2, 2));

    let tex1 = store.get(id1).unwrap();
    assert_eq!(tex1.name, "tex1");
    assert_eq!(tex1.version, 1);
    assert_eq!(tex1.levels.len(), 1);

    // Iter
    let items: Vec<_> = store.iter().collect();
    assert_eq!(items.len(), 2); // WHITE and id1

    // Remove
    store.remove(id1);
    assert!(store.get(id1).is_none());
    assert_eq!(store.size(id1), UVec2::ZERO);

    // After removal, WHITE remains
    let items_after: Vec<_> = store.iter().collect();
    assert_eq!(items_after.len(), 1);
}

#[test]
fn test_mipmaps_generation_and_clamping() {
    let mut store = TextureStore::new();
    // 5x3 texture with mipmaps enabled
    // Dimensions should scale: 5x3 -> 2x1 -> 1x1
    let width = 5;
    let height = 3;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            rgba.push((x * 20) as u8);
            rgba.push((y * 40) as u8);
            rgba.push(100);
            rgba.push(255);
        }
    }

    let opts = TextureOptions {
        mipmaps: true,
        wrap: Wrap::Clamp,
        filter: Filter::Linear,
    };
    let id = store.create_rgba("mip_test", width as u32, height as u32, rgba.clone(), opts);
    let tex = store.get(id).unwrap();
    assert_eq!(tex.levels.len(), 3);
    assert_eq!(tex.levels[0].len(), 5 * 3 * 4);
    assert_eq!(tex.levels[1].len(), 2 * 4);
    assert_eq!(tex.levels[2].len(), 4);

    // Test 2x2 box filtering on a simple 2x2 image
    let pixels_2x2 = vec![
        10, 20, 30, 40, // (0,0)
        50, 60, 70, 80, // (1,0)
        90, 100, 110, 120, // (0,1)
        130, 140, 150, 160, // (1,1)
    ];
    let id_2x2 = store.create_rgba("2x2", 2, 2, pixels_2x2, opts);
    let tex_2x2 = store.get(id_2x2).unwrap();
    assert_eq!(tex_2x2.levels.len(), 2);
    // (10+50+90+130+2)/4 = (280+2)/4 = 70
    // (20+60+100+140+2)/4 = (320+2)/4 = 80
    // (30+70+110+150+2)/4 = (360+2)/4 = 90
    // (40+80+120+160+2)/4 = (400+2)/4 = 100
    assert_eq!(tex_2x2.levels[1], vec![70, 80, 90, 100]);
}

#[test]
fn test_update_rgba_clipping_and_version() {
    let mut store = TextureStore::new();
    let opts = TextureOptions {
        mipmaps: true,
        wrap: Wrap::Clamp,
        filter: Filter::Linear,
    };
    let id = store.create_rgba("update_test", 4, 4, vec![0; 4 * 4 * 4], opts);
    let init_ver = store.get(id).unwrap().version;

    // Update partial region 2x2 at (1, 1)
    let patch = vec![255; 2 * 2 * 4];
    store.update_rgba(id, 1, 1, 2, 2, &patch);

    let tex = store.get(id).unwrap();
    assert_eq!(tex.version, init_ver + 1);

    // Verify level 0 pixel at (1, 1) is 255
    let idx = ((4 + 1) * 4) as usize;
    assert_eq!(&tex.levels[0][idx..idx + 4], &[255, 255, 255, 255]);
    // Pixel at (0, 0) is still 0
    assert_eq!(&tex.levels[0][0..4], &[0, 0, 0, 0]);

    // Test out of range clipping (e.g. at x=3, y=3 with size 3x3)
    let large_patch = vec![128; 3 * 3 * 4];
    store.update_rgba(id, 3, 3, 3, 3, &large_patch);
    let tex = store.get(id).unwrap();
    assert_eq!(tex.version, init_ver + 2);
    // (3, 3) should be updated
    let idx_33 = ((3 * 4 + 3) * 4) as usize;
    assert_eq!(&tex.levels[0][idx_33..idx_33 + 4], &[128, 128, 128, 128]);

    // Update completely outside bounds should not crash
    store.update_rgba(id, 10, 10, 2, 2, &patch);
    // Update with 0 width or height should be no-op
    store.update_rgba(id, 0, 0, 0, 0, &[]);
    // Non-existent id should be no-op
    store.update_rgba(TextureId(999), 0, 0, 2, 2, &patch);
}

#[test]
fn test_load_bytes_and_cache() {
    let mut store = TextureStore::new();

    // Decode embedded resources
    let id_file = store
        .load_bytes(
            "file.png",
            gource_draw::resources::FILE_PNG,
            TextureOptions::plain(),
        )
        .expect("load file.png");
    let id_user = store
        .load_bytes(
            "user.png",
            gource_draw::resources::USER_PNG,
            TextureOptions::plain(),
        )
        .expect("load user.png");
    let id_beam = store
        .load_bytes(
            "beam.png",
            gource_draw::resources::BEAM_PNG,
            TextureOptions::plain(),
        )
        .expect("load beam.png");

    assert_ne!(id_file, id_user);
    assert_ne!(id_file, id_beam);

    // Loading same name & options should return cached id
    let id_file_again = store
        .load_bytes(
            "file.png",
            gource_draw::resources::FILE_PNG,
            TextureOptions::plain(),
        )
        .unwrap();
    assert_eq!(id_file, id_file_again);

    // Invalid bytes error
    let err = store.load_bytes("bad", b"not an image", TextureOptions::plain());
    assert!(err.is_err());
}

#[test]
fn test_load_file_all_supported_formats() {
    let dir = tempdir().unwrap();

    let rgba_formats = [
        ("test.png", image::ImageFormat::Png),
        ("test.bmp", image::ImageFormat::Bmp),
        ("test.gif", image::ImageFormat::Gif),
        ("test.tga", image::ImageFormat::Tga),
    ];

    let img_rgba: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_pixel(4, 4, Rgba([100, 150, 200, 255]));

    let mut store = TextureStore::new();

    for (filename, format) in rgba_formats {
        let path = dir.path().join(filename);
        img_rgba
            .save_with_format(&path, format)
            .expect("save image");

        let id = store
            .load_file(&path, TextureOptions::plain())
            .expect("load file");
        let size = store.size(id);
        assert_eq!(size, UVec2::new(4, 4));

        // Loading again returns cached id
        let id_cached = store.load_file(&path, TextureOptions::plain()).unwrap();
        assert_eq!(id, id_cached);
    }

    // Test JPEG with RGB
    let img_rgb: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_pixel(4, 4, Rgb([100, 150, 200]));
    let jpeg_path = dir.path().join("test.jpeg");
    img_rgb
        .save_with_format(&jpeg_path, image::ImageFormat::Jpeg)
        .expect("save jpeg");
    let id_jpeg = store
        .load_file(&jpeg_path, TextureOptions::plain())
        .expect("load jpeg");
    assert_eq!(store.size(id_jpeg), UVec2::new(4, 4));

    // Non-existent file error
    let err = store.load_file(
        &dir.path().join("does_not_exist.png"),
        TextureOptions::plain(),
    );
    assert!(err.is_err());

    // File with unknown extension and corrupt content
    let unknown_path = dir.path().join("file_without_ext");
    std::fs::write(&unknown_path, b"corrupted bytes").unwrap();
    let err_unknown = store.load_file(&unknown_path, TextureOptions::plain());
    assert!(err_unknown.is_err());

    let unknown_ext = dir.path().join("file.xyz");
    std::fs::write(&unknown_ext, b"corrupted bytes").unwrap();
    let err_ext = store.load_file(&unknown_ext, TextureOptions::plain());
    assert!(err_ext.is_err());

    // Test TGA without magic bytes where format is guessed from extension
    let tga_ext_path = dir.path().join("test_guess.tga");
    let mut tga_bytes = vec![0u8; 18];
    tga_bytes[2] = 2; // uncompressed truecolor
    tga_bytes[12] = 1;
    tga_bytes[14] = 1;
    tga_bytes[16] = 24;
    tga_bytes.extend_from_slice(&[255, 0, 0]);
    std::fs::write(&tga_ext_path, &tga_bytes).unwrap();
    let id_tga_guess = store
        .load_file(&tga_ext_path, TextureOptions::plain())
        .expect("load tga by extension");
    assert_eq!(store.size(id_tga_guess), UVec2::new(1, 1));

    // Test TextureStore::default()
    let def_store = TextureStore::default();
    assert!(def_store.get(TextureId::WHITE).is_some());
}

#[test]
fn test_texture_options_default_and_remove_unknown() {
    let default_opts = TextureOptions::default();
    assert!(default_opts.mipmaps);
    assert_eq!(default_opts.wrap, Wrap::Clamp);
    assert_eq!(default_opts.filter, Filter::Linear);

    let mut store = TextureStore::new();
    // Remove non-existent ID should not crash
    store.remove(TextureId(999));

    // Update non-mipmapped texture
    let id_plain = store.create_rgba("plain", 2, 2, vec![0; 16], TextureOptions::plain());
    store.update_rgba(id_plain, 0, 0, 1, 1, &[255, 255, 255, 255]);
    assert_eq!(store.get(id_plain).unwrap().levels.len(), 1);
}

#[test]
fn test_texture_advanced_formats_and_sdl_image_parity() {
    let dir = tempdir().unwrap();
    let mut store = TextureStore::new();

    // Helper functions for PNG generation without external crate dependencies
    fn crc32_simple(buf: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in buf {
            crc ^= b as u32;
            for _ in 0..8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ 0xedb8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc ^ 0xffff_ffff
    }

    fn make_zlib_store(data: &[u8]) -> Vec<u8> {
        let mut s1 = 1u32;
        let mut s2 = 0u32;
        for &b in data {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        let adler = (s2 << 16) | s1;

        let mut out = Vec::with_capacity(data.len() + 11);
        out.extend_from_slice(&[0x78, 0x01]); // zlib header
        out.push(0x01); // BFINAL=1, BTYPE=00
        let len = data.len() as u16;
        let nlen = len ^ 0xffff;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(&adler.to_be_bytes());
        out
    }

    fn png_chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::with_capacity(data.len() + 12);
        chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
        chunk.extend_from_slice(tag);
        chunk.extend_from_slice(data);
        let tag_data: Vec<u8> = tag.iter().chain(data.iter()).copied().collect();
        let crc = crc32_simple(&tag_data);
        chunk.extend_from_slice(&crc.to_be_bytes());
        chunk
    }

    // 1. Greyscale PNG (8-bit)
    let gray_png = {
        let mut data = Vec::new();
        let raw = [0u8, 64, 128, 0, 192, 255]; // 2 rows, filter=0
        let compressed = make_zlib_store(&raw);
        data.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.push(8);
        ihdr.push(0); // greyscale
        ihdr.extend_from_slice(&[0, 0, 0]);
        data.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
        data.extend_from_slice(&png_chunk(b"IDAT", &compressed));
        data.extend_from_slice(&png_chunk(b"IEND", &[]));
        data
    };
    let gray_path = dir.path().join("gray.png");
    std::fs::write(&gray_path, &gray_png).unwrap();
    let id_gray = store
        .load_file(&gray_path, TextureOptions::plain())
        .expect("load gray.png");
    let tex_gray = store.get(id_gray).unwrap();
    assert_eq!(tex_gray.width, 2);
    assert_eq!(tex_gray.height, 2);
    assert_eq!(&tex_gray.levels[0][0..4], &[64, 64, 64, 255]);
    assert_eq!(&tex_gray.levels[0][4..8], &[128, 128, 128, 255]);

    // 2. Greyscale + Alpha PNG (8-bit)
    let ga_png = {
        let mut data = Vec::new();
        let raw = [0u8, 64, 255, 128, 128, 0, 192, 64, 255, 0];
        let compressed = make_zlib_store(&raw);
        data.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.push(8);
        ihdr.push(4); // greyscale + alpha
        ihdr.extend_from_slice(&[0, 0, 0]);
        data.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
        data.extend_from_slice(&png_chunk(b"IDAT", &compressed));
        data.extend_from_slice(&png_chunk(b"IEND", &[]));
        data
    };
    let ga_path = dir.path().join("ga.png");
    std::fs::write(&ga_path, &ga_png).unwrap();
    let id_ga = store
        .load_file(&ga_path, TextureOptions::plain())
        .expect("load ga.png");
    let tex_ga = store.get(id_ga).unwrap();
    assert_eq!(&tex_ga.levels[0][0..4], &[64, 64, 64, 255]);
    assert_eq!(&tex_ga.levels[0][4..8], &[128, 128, 128, 128]);

    // 3. Paletted PNG with tRNS chunk
    let pal_png = {
        let mut data = Vec::new();
        data.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.push(8);
        ihdr.push(3); // indexed color
        ihdr.extend_from_slice(&[0, 0, 0]);
        data.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
        let plte = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
        data.extend_from_slice(&png_chunk(b"PLTE", &plte));
        let trns = [0u8, 128, 255, 64];
        data.extend_from_slice(&png_chunk(b"tRNS", &trns));
        let raw = [0u8, 0, 1, 0, 2, 3];
        let compressed = make_zlib_store(&raw);
        data.extend_from_slice(&png_chunk(b"IDAT", &compressed));
        data.extend_from_slice(&png_chunk(b"IEND", &[]));
        data
    };
    let pal_path = dir.path().join("paletted.png");
    std::fs::write(&pal_path, &pal_png).unwrap();
    let id_pal = store
        .load_file(&pal_path, TextureOptions::plain())
        .expect("load paletted.png");
    let tex_pal = store.get(id_pal).unwrap();
    assert_eq!(&tex_pal.levels[0][0..4], &[255, 0, 0, 0]);
    assert_eq!(&tex_pal.levels[0][4..8], &[0, 255, 0, 128]);

    // 4. 16-bit PNG (RGB 16-bit scaled down to 8-bit)
    let rgb16_png = {
        let mut data = Vec::new();
        data.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.push(16);
        ihdr.push(2); // RGB
        ihdr.extend_from_slice(&[0, 0, 0]);
        data.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
        let mut raw = vec![0u8];
        raw.extend_from_slice(&0xFFFFu16.to_be_bytes());
        raw.extend_from_slice(&0x7FFFu16.to_be_bytes());
        raw.extend_from_slice(&0x0000u16.to_be_bytes());
        let compressed = make_zlib_store(&raw);
        data.extend_from_slice(&png_chunk(b"IDAT", &compressed));
        data.extend_from_slice(&png_chunk(b"IEND", &[]));
        data
    };
    let rgb16_path = dir.path().join("rgb16.png");
    std::fs::write(&rgb16_path, &rgb16_png).unwrap();
    let id_16 = store
        .load_file(&rgb16_path, TextureOptions::plain())
        .expect("load rgb16.png");
    let tex_16 = store.get(id_16).unwrap();
    assert_eq!(tex_16.levels[0][0], 255);
    assert!((tex_16.levels[0][1] as i32 - 127).abs() <= 1);
    assert_eq!(tex_16.levels[0][2], 0);
    assert_eq!(tex_16.levels[0][3], 255);

    // 5. BMP 24-bit
    let bmp24_data = {
        let mut data = Vec::new();
        // BITMAPFILEHEADER
        data.extend_from_slice(b"BM");
        data.extend_from_slice(&(14u32 + 40 + 16).to_le_bytes()); // bfSize
        data.extend_from_slice(&0u32.to_le_bytes()); // reserved
        data.extend_from_slice(&54u32.to_le_bytes()); // bfOffBits
        // BITMAPINFOHEADER
        data.extend_from_slice(&40u32.to_le_bytes()); // biSize
        data.extend_from_slice(&2i32.to_le_bytes()); // biWidth
        data.extend_from_slice(&2i32.to_le_bytes()); // biHeight (bottom-up)
        data.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        data.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
        data.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
        data.extend_from_slice(&16u32.to_le_bytes()); // biSizeImage
        data.extend_from_slice(&2835i32.to_le_bytes());
        data.extend_from_slice(&2835i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        // 2x2 row padding: 2*3 = 6 + 2 padding = 8 bytes per row
        // bottom row: red (BGR 0,0,255), green (BGR 0,255,0)
        data.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        // top row: blue (BGR 255,0,0), white (BGR 255,255,255)
        data.extend_from_slice(&[255, 0, 0, 255, 255, 255, 0, 0]);
        data
    };
    let bmp24_path = dir.path().join("bmp24.bmp");
    std::fs::write(&bmp24_path, &bmp24_data).unwrap();
    let id_bmp24 = store
        .load_file(&bmp24_path, TextureOptions::plain())
        .expect("load bmp24.bmp");
    let tex_bmp24 = store.get(id_bmp24).unwrap();
    assert_eq!(tex_bmp24.width, 2);
    assert_eq!(tex_bmp24.height, 2);
    // Top-left is blue (255, 0, 0) in RGB
    assert_eq!(&tex_bmp24.levels[0][0..4], &[0, 0, 255, 255]);

    // 6. BMP 32-bit (BITMAPV5HEADER with alpha mask)
    let bmp32_data = {
        let mut data = Vec::new();
        let header_bytes: [u8; 138] = [
            66, 77, 154, 0, 0, 0, 0, 0, 0, 0, 138, 0, 0, 0, 124, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0,
            1, 0, 32, 0, 3, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 66, 71, 82, 115, 143, 194, 245,
            40, 81, 184, 30, 21, 30, 133, 235, 1, 51, 51, 51, 19, 102, 102, 102, 38, 102, 102, 102,
            6, 153, 153, 153, 9, 61, 10, 215, 3, 40, 92, 143, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        data.extend_from_slice(&header_bytes);
        // bottom row: (B, G, R, A)
        data.extend_from_slice(&[0, 0, 255, 128, 0, 255, 0, 200]);
        // top row
        data.extend_from_slice(&[255, 0, 0, 255, 255, 255, 255, 64]);
        data
    };
    let bmp32_path = dir.path().join("bmp32.bmp");
    std::fs::write(&bmp32_path, &bmp32_data).unwrap();
    let id_bmp32 = store
        .load_file(&bmp32_path, TextureOptions::plain())
        .expect("load bmp32.bmp");
    let tex_bmp32 = store.get(id_bmp32).unwrap();
    assert_eq!(&tex_bmp32.levels[0][0..4], &[0, 0, 255, 255]);
    assert_eq!(&tex_bmp32.levels[0][4..8], &[255, 255, 255, 64]);

    // 7. Uncompressed TGA (24-bit TrueColor)
    let tga_uncomp_data = {
        let mut data = vec![0u8; 18];
        data[2] = 2; // uncompressed truecolor
        data[12] = 2; // width low
        data[14] = 2; // height low
        data[16] = 24; // bits per pixel
        data[17] = 0x20; // top-down
        // 4 pixels BGR
        data.extend_from_slice(&[0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255]);
        data
    };
    let tga_u_path = dir.path().join("uncomp.tga");
    std::fs::write(&tga_u_path, &tga_uncomp_data).unwrap();
    let id_tu = store
        .load_file(&tga_u_path, TextureOptions::plain())
        .expect("load uncomp.tga");
    assert_eq!(store.size(id_tu), UVec2::new(2, 2));

    // 8. RLE Compressed TGA (24-bit TrueColor)
    let tga_rle_data = {
        let mut data = vec![0u8; 18];
        data[2] = 10; // RLE truecolor
        data[12] = 2;
        data[14] = 2;
        data[16] = 24;
        data[17] = 0x20;
        // RLE packet: 4 identical red pixels (packet count = 0x80 | 3) followed by 1 BGR pixel
        data.extend_from_slice(&[0x83, 0, 0, 255]);
        data
    };
    let tga_r_path = dir.path().join("rle.tga");
    std::fs::write(&tga_r_path, &tga_rle_data).unwrap();
    let id_tr = store
        .load_file(&tga_r_path, TextureOptions::plain())
        .expect("load rle.tga");
    let tex_tr = store.get(id_tr).unwrap();
    assert_eq!(&tex_tr.levels[0][0..4], &[255, 0, 0, 255]);
    assert_eq!(&tex_tr.levels[0][4..8], &[255, 0, 0, 255]);
}

#[test]
fn test_texture_odd_box_filter_and_cache_keys() {
    let mut store = TextureStore::new();

    // Test non-power-of-two odd sizes: 7x5
    // Chain should be: 7x5 -> 3x2 -> 1x1
    let w = 7;
    let h = 5;
    let mut pixels = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            pixels[idx] = (x * 10) as u8;
            pixels[idx + 1] = (y * 20) as u8;
            pixels[idx + 2] = 100;
            pixels[idx + 3] = 255;
        }
    }
    let opts = TextureOptions {
        mipmaps: true,
        wrap: Wrap::Repeat,
        filter: Filter::Nearest,
    };
    let id = store.create_rgba("odd_chain", w as u32, h as u32, pixels.clone(), opts);
    let tex = store.get(id).unwrap();
    assert_eq!(tex.levels.len(), 3);
    assert_eq!(tex.levels[0].len(), 7 * 5 * 4);
    assert_eq!(tex.levels[1].len(), 3 * 2 * 4);
    assert_eq!(tex.levels[2].len(), 4);

    // Verify box filter on odd dimensions:
    // For level 1 at dst (x=2, y=1):
    // src_x0 = 4, src_x1 = min(5, 6) = 5
    // src_y0 = 2, src_y1 = min(3, 4) = 3
    // p00=(4,2), p10=(5,2), p01=(4,3), p11=(5,3)
    // Red channel: p00=40, p10=50, p01=40, p11=50 -> sum = 180 -> (180+2)/4 = 45
    let lvl1_p21 = &tex.levels[1][((3 + 2) * 4)..((3 + 2) * 4 + 4)];
    assert_eq!(lvl1_p21[0], 45);

    // Test caching with same path but different options
    let dir = tempdir().unwrap();
    let img_path = dir.path().join("cached_test.png");
    let img_buf: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(2, 2, Rgba([10, 20, 30, 40]));
    img_buf
        .save_with_format(&img_path, image::ImageFormat::Png)
        .unwrap();

    let opts1 = TextureOptions {
        mipmaps: false,
        wrap: Wrap::Clamp,
        filter: Filter::Linear,
    };
    let opts2 = TextureOptions {
        mipmaps: true,
        wrap: Wrap::Repeat,
        filter: Filter::Nearest,
    };

    let id1 = store.load_file(&img_path, opts1).unwrap();
    let id2 = store.load_file(&img_path, opts2).unwrap();
    assert_ne!(
        id1, id2,
        "Different options must yield different cache entries"
    );

    let id1_again = store.load_file(&img_path, opts1).unwrap();
    assert_eq!(
        id1, id1_again,
        "Same path and options must return cached id"
    );
}

#[test]
fn test_update_rgba_edge_clipping_and_id_stability() {
    let mut store = TextureStore::new();
    let opts = TextureOptions::plain();

    // 4x4 texture
    let id = store.create_rgba("edge_clip", 4, 4, vec![0; 4 * 4 * 4], opts);

    // 1. Clipping at right edge: x=3, y=1, w=3, h=2 (only 1 column fits)
    let patch_right = vec![50; 3 * 2 * 4];
    store.update_rgba(id, 3, 1, 3, 2, &patch_right);
    let tex = store.get(id).unwrap();
    // (3, 1) and (3, 2) should be 50
    let idx_31 = ((4 + 3) * 4) as usize;
    let idx_32 = ((2 * 4 + 3) * 4) as usize;
    assert_eq!(&tex.levels[0][idx_31..idx_31 + 4], &[50, 50, 50, 50]);
    assert_eq!(&tex.levels[0][idx_32..idx_32 + 4], &[50, 50, 50, 50]);

    // 2. Clipping at bottom edge: x=1, y=3, w=2, h=3 (only 1 row fits)
    let patch_bottom = vec![75; 2 * 3 * 4];
    store.update_rgba(id, 1, 3, 2, 3, &patch_bottom);
    let tex = store.get(id).unwrap();
    let idx_13 = ((3 * 4 + 1) * 4) as usize;
    let idx_23 = ((3 * 4 + 2) * 4) as usize;
    assert_eq!(&tex.levels[0][idx_13..idx_13 + 4], &[75, 75, 75, 75]);
    assert_eq!(&tex.levels[0][idx_23..idx_23 + 4], &[75, 75, 75, 75]);

    // 3. Clipping at top-left: x=0, y=0, w=2, h=2
    let patch_tl = vec![99; 2 * 2 * 4];
    store.update_rgba(id, 0, 0, 2, 2, &patch_tl);
    let tex = store.get(id).unwrap();
    assert_eq!(&tex.levels[0][0..4], &[99, 99, 99, 99]);

    // 4. Test remove and re-add id stability (IDs are strictly monotonic and never reused)
    let id_a = store.create_rgba("tex_a", 2, 2, vec![0; 16], opts);
    let id_b = store.create_rgba("tex_b", 2, 2, vec![0; 16], opts);
    assert_eq!(id_b.0, id_a.0 + 1);

    store.remove(id_a);
    assert!(store.get(id_a).is_none());

    // Creating a new texture after removal does not reuse id_a
    let id_c = store.create_rgba("tex_c", 2, 2, vec![0; 16], opts);
    assert_eq!(id_c.0, id_b.0 + 1);

    // 5. Test error messages for missing and corrupt files
    let dir = tempdir().unwrap();
    let non_existent = dir.path().join("missing.png");
    let err_missing = store.load_file(&non_existent, opts).unwrap_err();
    assert!(
        err_missing.to_string().contains("failed to read"),
        "Unexpected error: {}",
        err_missing
    );

    let corrupt_path = dir.path().join("corrupt.png");
    std::fs::write(&corrupt_path, b"not a png image").unwrap();
    let err_corrupt = store.load_file(&corrupt_path, opts).unwrap_err();
    assert!(
        err_corrupt.to_string().contains("failed to decode"),
        "Unexpected error: {}",
        err_corrupt
    );
}

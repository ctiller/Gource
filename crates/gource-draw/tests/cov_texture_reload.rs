use glam::UVec2;
use gource_draw::texture::{TextureError, TextureOptions, TextureStore};
use image::{ImageBuffer, Rgba};
use tempfile::tempdir;

#[test]
fn test_reload_files_normal_flow() {
    let dir = tempdir().expect("tempdir");
    let img_path = dir.path().join("dynamic_icon.png");

    // 1. Write an initial 4x4 image
    let img1: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
    img1.save_with_format(&img_path, image::ImageFormat::Png)
        .expect("save initial image");

    let mut store = TextureStore::new();
    let opts = TextureOptions {
        mipmaps: true,
        ..TextureOptions::plain()
    };
    let id = store.load_file(&img_path, opts).expect("load initial file");

    let tex_before = store.get(id).expect("texture must exist");
    assert_eq!(tex_before.size(), UVec2::new(4, 4));
    assert_eq!(tex_before.version, 1);
    assert_eq!(tex_before.levels.len(), 3); // 4x4 -> 2x2 -> 1x1

    // 2. Overwrite file with an 8x8 image (different dimensions and contents)
    let img2: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(8, 8, Rgba([0, 255, 0, 255]));
    img2.save_with_format(&img_path, image::ImageFormat::Png)
        .expect("save updated image");

    // 3. Call reload_files()
    let errors = store.reload_files();
    assert!(errors.is_empty(), "reload should succeed without errors");

    // 4. Assert same ID, new size, version bumped, mipmaps regenerated
    let tex_after = store.get(id).expect("texture still exists with same id");
    assert_eq!(tex_after.size(), UVec2::new(8, 8));
    assert_eq!(tex_after.version, 2);
    assert_eq!(tex_after.levels.len(), 4); // 8x8 -> 4x4 -> 2x2 -> 1x1
    assert_eq!(tex_after.levels[0][0..4], [0, 255, 0, 255]);
    assert_eq!(store.size(id), UVec2::new(8, 8));

    // Reload with mipmaps disabled
    let img_no_mip_path = dir.path().join("no_mip.png");
    let img_no_mip: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_pixel(6, 6, Rgba([10, 20, 30, 255]));
    img_no_mip
        .save_with_format(&img_no_mip_path, image::ImageFormat::Png)
        .expect("save no_mip image");

    let id_no_mip = store
        .load_file(&img_no_mip_path, TextureOptions::plain())
        .expect("load no_mip file");
    let tex_no_mip = store.get(id_no_mip).unwrap();
    assert_eq!(tex_no_mip.levels.len(), 1);
    assert_eq!(tex_no_mip.version, 1);

    // Overwrite and reload
    let img_no_mip_2: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_pixel(12, 12, Rgba([50, 60, 70, 255]));
    img_no_mip_2
        .save_with_format(&img_no_mip_path, image::ImageFormat::Png)
        .expect("overwrite no_mip image");

    let errors2 = store.reload_files();
    assert!(errors2.is_empty());
    let tex_no_mip_after = store.get(id_no_mip).unwrap();
    assert_eq!(tex_no_mip_after.size(), UVec2::new(12, 12));
    assert_eq!(tex_no_mip_after.levels.len(), 1);
    assert_eq!(tex_no_mip_after.version, 2);
}

#[test]
fn test_reload_files_deleted_or_corrupted() {
    let dir = tempdir().expect("tempdir");

    // File 1: will be deleted
    let del_path = dir.path().join("will_delete.png");
    let img1: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
    img1.save_with_format(&del_path, image::ImageFormat::Png)
        .unwrap();

    // File 2: will be corrupted
    let corrupt_path = dir.path().join("will_corrupt.png");
    let img2: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(2, 2, Rgba([4, 5, 6, 255]));
    img2.save_with_format(&corrupt_path, image::ImageFormat::Png)
        .unwrap();

    let mut store = TextureStore::new();
    let id_del = store
        .load_file(&del_path, TextureOptions::plain())
        .expect("load del file");
    let id_corrupt = store
        .load_file(&corrupt_path, TextureOptions::plain())
        .expect("load corrupt file");

    let del_before_version = store.get(id_del).unwrap().version;
    let corrupt_before_version = store.get(id_corrupt).unwrap().version;

    // Delete file 1
    std::fs::remove_file(&del_path).expect("remove file");

    // Corrupt file 2 with garbage bytes
    std::fs::write(&corrupt_path, b"not a valid png file at all!").expect("write garbage");

    let errors = store.reload_files();
    assert_eq!(errors.len(), 2, "both files should return errors");

    let mut seen_del = false;
    let mut seen_corrupt = false;
    for err in &errors {
        match err {
            TextureError::Io { path, .. } => {
                if path.contains("will_delete.png") {
                    seen_del = true;
                }
            }
            TextureError::Decode { name, .. } => {
                if name.contains("will_corrupt.png") {
                    seen_corrupt = true;
                }
            }
        }
    }
    assert!(seen_del, "expected Io error for deleted file");
    assert!(seen_corrupt, "expected Decode error for corrupted file");

    // A texture whose file can no longer be read keeps its old pixels and version
    let del_after = store.get(id_del).unwrap();
    assert_eq!(del_after.version, del_before_version);
    assert_eq!(del_after.size(), UVec2::new(2, 2));

    let corrupt_after = store.get(id_corrupt).unwrap();
    assert_eq!(corrupt_after.version, corrupt_before_version);
    assert_eq!(corrupt_after.size(), UVec2::new(2, 2));
}

#[test]
fn test_reload_files_removed_textures_not_reloaded() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("removed.png");
    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();

    let mut store = TextureStore::new();
    let id = store
        .load_file(&path, TextureOptions::plain())
        .expect("load file");

    // Remove the texture
    store.remove(id);
    assert!(store.get(id).is_none());

    // Delete the file on disk. If reload_files tried to reload it, it would error.
    std::fs::remove_file(&path).unwrap();

    let errors = store.reload_files();
    assert!(
        errors.is_empty(),
        "removed texture should not be reloaded even if file was deleted"
    );
}

#[test]
fn test_decode_file_extension_guessing_branches() {
    let dir = tempdir().expect("tempdir");

    // 1. File without extension and no recognized magic bytes
    let no_ext_path = dir.path().join("no_extension");
    std::fs::write(&no_ext_path, b"random arbitrary payload").unwrap();
    let mut store = TextureStore::new();
    let err_no_ext = store.load_file(&no_ext_path, TextureOptions::plain());
    assert!(err_no_ext.is_err());

    // 2. File with unrecognized extension
    let unknown_ext_path = dir.path().join("test_file.unknownxyz");
    std::fs::write(&unknown_ext_path, b"random arbitrary payload").unwrap();
    let err_unknown_ext = store.load_file(&unknown_ext_path, TextureOptions::plain());
    assert!(err_unknown_ext.is_err());

    // 3. TGA file without magic bytes (tests lines 171-173 where format is guessed from extension)
    let tga_path = dir.path().join("raw_format.tga");
    let mut tga_bytes = vec![0u8; 18];
    tga_bytes[2] = 2; // uncompressed truecolor
    tga_bytes[12] = 2; // width low byte
    tga_bytes[14] = 2; // height low byte
    tga_bytes[16] = 24; // 24 bpp (BGR)
    tga_bytes.extend_from_slice(&[
        255, 0, 0, // pixel (0,0)
        0, 255, 0, // pixel (1,0)
        0, 0, 255, // pixel (0,1)
        255, 255, 255, // pixel (1,1)
    ]);
    std::fs::write(&tga_path, &tga_bytes).unwrap();
    let tga_id = store
        .load_file(&tga_path, TextureOptions::plain())
        .expect("load TGA file");
    assert_eq!(store.size(tga_id), UVec2::new(2, 2));

    // Reload the TGA file to ensure reload_files works with format guessing too
    let reload_errs = store.reload_files();
    assert!(reload_errs.is_empty());
}

#[test]
fn test_update_rgba_assertion_failure() {
    let mut store = TextureStore::new();
    let id = store.create_rgba("test_assert", 2, 2, vec![0; 16], TextureOptions::plain());

    // Calling update_rgba with buffer len not equal to w * h * 4 panics with assertion
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.update_rgba(id, 0, 0, 2, 2, &[0; 15]);
    }));
    assert!(result.is_err(), "update_rgba must assert buffer length");
}
